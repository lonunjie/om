// SPDX-License-Identifier: Apache-2.0
//! Multi-process persistence scenario (V-001 criterion 1). Each step is executed by a FRESH
//! process (native or wasm32-wasip1): load `world.cbor`, check canonical re-encoding,
//! perform the step, save. Out-of-order and skipped messages span process restarts.
//! The RNG for step k is the insecure test RNG labelled "persist-step-k", so native, WASI
//! and mixed runs must produce byte-identical worlds.

use crate::cbor::{self, V};
use crate::rng::insecure_test_rng::InsecureTestRng;
use crate::store::{Device, Provider};
use crate::{sha256, Error};

#[derive(Clone, Copy, PartialEq)]
enum Who {
    A,
    B,
}

enum Step {
    Init,
    Start,
    Send(Who, Vec<String>),
    Recv(Who, Vec<String>),
}

const ROUNDS: usize = 60;
const BURST: usize = 3;

fn steps() -> Vec<Step> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let mut st = vec![
        Step::Init,
        Step::Start,
        Step::Send(Who::A, s(&["a1", "a2", "a3"])),
        Step::Recv(Who::B, s(&["a3", "a1"])), // a2 withheld
        Step::Send(Who::B, s(&["b1", "b2", "b3"])),
        Step::Recv(Who::A, s(&["b3", "b1"])), // b2 withheld
        Step::Send(Who::A, s(&["a4"])),
        Step::Recv(Who::B, s(&["a2", "a4"])), // skipped key from the previous chain, after reloads
        Step::Recv(Who::A, s(&["b2"])),
    ];
    for r in 0..ROUNDS {
        let xs: Vec<String> = (0..BURST).map(|i| format!("x{r}.{i}")).collect();
        let ys: Vec<String> = (0..BURST).map(|i| format!("y{r}.{i}")).collect();
        let rev = |v: &Vec<String>| {
            let mut w = v.clone();
            w.reverse();
            w
        };
        st.push(Step::Send(Who::A, xs.clone()));
        st.push(Step::Recv(Who::B, rev(&xs)));
        st.push(Step::Send(Who::B, ys.clone()));
        st.push(Step::Recv(Who::A, rev(&ys)));
    }
    st
}

pub fn n_steps() -> usize {
    steps().len()
}

struct World {
    alice: Device,
    bob: Device,
    provider: Provider,
    queue: Vec<(String, Vec<u8>)>,
    max_ratchet_steps: u64,
    delivered: u64,
}

impl World {
    fn to_bytes(&self) -> Vec<u8> {
        cbor::encode(&V::M(vec![
            (0, V::T("om-v001-proto-persist-world".into())),
            (1, self.alice.to_cbor()),
            (2, self.bob.to_cbor()),
            (3, self.provider.to_cbor()),
            (4, V::A(self.queue.iter().map(|(l, e)| V::A(vec![V::T(l.clone()), V::B(e.clone())])).collect())),
            (5, V::U(self.max_ratchet_steps)),
            (6, V::U(self.delivered)),
        ]))
    }
    fn from_bytes(b: &[u8]) -> Result<Self, Error> {
        let v = cbor::decode(b)?;
        if v.get(0)?.t()? != "om-v001-proto-persist-world" {
            return Err(Error::Decode("world tag"));
        }
        let mut queue = vec![];
        for e in v.get(4)?.a()? {
            let a = e.a()?;
            queue.push((a[0].t()?.to_string(), a[1].b()?.to_vec()));
        }
        Ok(World {
            alice: Device::from_cbor(v.get(1)?)?,
            bob: Device::from_cbor(v.get(2)?)?,
            provider: Provider::from_cbor(v.get(3)?)?,
            queue,
            max_ratchet_steps: v.get(5)?.u()?,
            delivered: v.get(6)?.u()?,
        })
    }
}

fn plaintext(label: &str) -> Vec<u8> {
    format!("OM persisted message {label}").into_bytes()
}

/// Execute step `k` against `world` bytes (None for step 0). Returns new world bytes.
pub fn step(k: usize, world: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let all = steps();
    let st = all.get(k).ok_or("no such step")?;
    let mut rng = InsecureTestRng::from_label(format!("persist-step-{k}").as_bytes());
    let mut w = match (st, world) {
        (Step::Init, _) => {
            let alice = Device::new(&mut rng);
            let mut bob = Device::new(&mut rng);
            let mut provider = Provider::default();
            bob.trust_device(&alice).map_err(|e| e.to_string())?;
            let up = bob.generate_one_time(2, 2, &mut rng);
            provider.publish(&bob, up);
            World { alice, bob, provider, queue: vec![], max_ratchet_steps: 0, delivered: 0 }
        }
        (_, Some(b)) => {
            let w = World::from_bytes(b).map_err(|e| format!("load: {e}"))?;
            if w.to_bytes() != b {
                return Err("reloaded world does not re-encode byte-identically (non-canonical state)".into());
            }
            w
        }
        (_, None) => return Err("missing world".into()),
    };
    match st {
        Step::Init => {}
        Step::Start => {
            let (acs, cred) = w.provider.view(&w.bob.device_id);
            let f = w.provider.fetch(&w.bob.device_id).map_err(|e| e.to_string())?;
            w.alice.start_session(&f, &Device::peer_view(&acs, &cred), &mut rng).map_err(|e| e.to_string())?;
        }
        Step::Send(who, labels) => {
            for l in labels {
                let (s, peer) = if *who == Who::A { (&mut w.alice, w.bob.ik()) } else { (&mut w.bob, w.alice.ik()) };
                let env = s.encrypt(&peer, &plaintext(l)).map_err(|e| format!("encrypt {l}: {e}"))?.0;
                let v = cbor::decode(&env).unwrap();
                let h = crate::session::parse_header(v.a().unwrap()[2].b().unwrap()).unwrap();
                w.max_ratchet_steps = w.max_ratchet_steps.max(h.pn + h.n);
                w.queue.push((l.clone(), env));
            }
        }
        Step::Recv(who, labels) => {
            for l in labels {
                let i = w.queue.iter().position(|(x, _)| x == l).ok_or(format!("{l} not queued"))?;
                let (_, env) = w.queue.remove(i);
                let r = if *who == Who::A { &mut w.alice } else { &mut w.bob };
                let (_, pt, _) = r.decrypt(&env, &mut rng).map_err(|e| format!("decrypt {l}: {e}"))?;
                if pt != plaintext(l) {
                    return Err(format!("plaintext mismatch for {l}"));
                }
                w.delivered += 1;
            }
        }
    }
    Ok(w.to_bytes())
}

/// Summary line for a saved world.
pub fn summary(world: &[u8]) -> Result<serde_json::Value, String> {
    let w = World::from_bytes(world).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "world_sha256": crate::hex(&sha256(&[world])),
        "world_bytes": world.len(),
        "delivered": w.delivered,
        "queued": w.queue.len(),
        "max_chain_position": w.max_ratchet_steps,
        "alice_skipped_keys": w.alice.sessions.values().map(|s| s.dr.ec.skipped.len()).sum::<usize>(),
        "bob_skipped_keys": w.bob.sessions.values().map(|s| s.dr.ec.skipped.len()).sum::<usize>(),
    }))
}
