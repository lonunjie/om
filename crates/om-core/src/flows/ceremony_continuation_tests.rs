// SPDX-License-Identifier: MPL-2.0
//! ADR-0195: the continuation form of the link approval. An account at the protocol's limits (a chain of 65 states,
//! 272 credentials) links end to end between two profile states, and a missing, repeated, reordered, swapped, altered
//! or late chunk, or a restart in the middle, never leaves a half-adopted account.
#![cfg(feature = "insecure-test-rng")]

use super::*;
use crate::crypto::rng::insecure_test_rng::InsecureTestRng;
use crate::crypto::store::Device;
use crate::route::RelayEntry;
use crate::state::{LinkOffer, LinkRequestRec, ProfileState};

const NOW_MS: u64 = 1_790_000_000_000;
const EXPIRES_MS: u64 = NOW_MS + CEREMONY_TTL_MS;
const HOME: &str = "om-test://home";

fn drive<T>(
    st: &mut ProfileState,
    rng: &mut InsecureTestRng,
    now_ms: u64,
    f: impl FnOnce(&mut Tx<'_, InsecureTestRng>) -> T,
) -> T {
    let (mut offers, open) = (vec![], vec![]);
    let mut tx = Tx {
        st,
        rng,
        now_ms,
        changed: vec![],
        open_profiles: &open,
        offers: &mut offers,
        blob_limits: None,
    };
    f(&mut tx)
}

type Fields = Vec<(u64, V)>;

struct Fx {
    rng: InsecureTestRng,
    /// The new device's profile before any message arrived.
    fresh: ProfileState,
    /// The approval, then the chunks, as sealed on the wire.
    wire: Vec<Vec<u8>>,
    offer: LinkOffer,
    states: usize,
    creds: usize,
}

impl Fx {
    /// 64 states and 271 credentials, so the approval itself makes 65 and 272. A real account reaches that count only
    /// through history: 255 messaging devices were linked and all but sixteen were removed again (a person never has
    /// more than a few active devices, and the approval owes its notice to each of them).
    fn full() -> Fx {
        Fx::build(194, 61, Some(16), 16)
    }

    fn build(first_add: usize, singles: usize, keep: Option<usize>, controls: usize) -> Fx {
        let mut rng = InsecureTestRng::from_label(b"continuation-full-account");
        let mut ctl = vec![Device::new(&mut rng)];
        while ctl.len() < controls {
            let mut d = Device::new(&mut rng);
            d.account_id = ctl[0].account_id;
            ctl.push(d);
        }
        let account = ctl[0].account_id;
        let mut g = ctl[0].acs();
        g.control = ctl.iter().map(|d| d.device_id).collect();
        g.credentials = ctl
            .iter()
            .map(|d| (d.device_id, d.credential().hash()))
            .collect();
        let all: Vec<_> = ctl.iter().map(|d| (&d.device_id, &d.signer)).collect();
        let mut creds: Vec<Vec<u8>> = ctl.iter().map(|d| d.credential().body()).collect();
        let mut chain = vec![control::sign_acs(&g.body(), &all, &mut rng)];
        let mut head = g;
        for n in std::iter::once(first_add).chain(std::iter::repeat_n(1, singles)) {
            let mut child = head.clone();
            child.generation = head.generation + 1;
            child.previous = Some(head.hash());
            for _ in 0..n {
                let mut d = Device::new(&mut rng);
                d.account_id = account;
                child.messaging.insert(d.device_id);
                child.credentials.push((d.device_id, d.credential().hash()));
                creds.push(d.credential().body());
            }
            let signed = control::sign_acs(
                &child.body(),
                &[(&ctl[0].device_id, &ctl[0].signer)],
                &mut rng,
            );
            head = control::verify_successor(&head, &signed, &creds).expect("a valid successor");
            chain.push(signed);
        }
        if let Some(keep) = keep {
            let mut child = head.clone();
            child.generation = head.generation + 1;
            child.previous = Some(head.hash());
            let drop: Vec<[u8; 16]> = head.messaging.iter().copied().skip(keep).collect();
            for d in drop {
                child.messaging.remove(&d);
                child.revoked.insert(d);
                child.credentials.retain(|(x, _)| *x != d);
            }
            let signed = control::sign_acs(
                &child.body(),
                &[(&ctl[0].device_id, &ctl[0].signer)],
                &mut rng,
            );
            control::verify_successor(&head, &signed, &creds).expect("a valid removal");
            chain.push(signed);
        }

        // The approving device, holding that account, with one pending request from the new device.
        let newcomer = Device::new(&mut rng);
        let mut approver = ProfileState::empty("approver".into());
        approver.device = Some(ctl[0].clone());
        approver.app.acs_chain = chain.iter().map(|c| hex(c)).collect();
        approver.app.acs_credentials = creds.iter().map(|c| hex(c)).collect();
        let secret = [5u8; 16];
        let (a_cap, b_recv) = (vec![1u8; 16], vec![2u8; 16]);
        let relay = |cap: Vec<u8>| RelayEntry {
            relay_uri: HOME.into(),
            capability: cap,
            priority: 0,
            expires_at: 0,
        };
        let nonce = [7u8; 16];
        let req_digest = crypto::sha256(&[b"the request bytes"]);
        let request_id = request_id_of(&newcomer.device_id);
        let offer = LinkOffer {
            operation_id: "offer".into(),
            secret: hex(&secret).into(),
            a_mailbox: "aa".into(),
            a_route: relay(a_cap.clone()),
            b_mailbox: hex(&recv_mailbox(&b_recv)),
            b_route: relay(vec![3u8; 16]),
            b_recv: hex(&b_recv),
            link: None,
            created_ms: NOW_MS,
            expires_at: EXPIRES_MS,
            state: LinkState::AwaitingApproval,
            request: Some(LinkRequestRec {
                request_id: request_id.clone(),
                device_id: hex(&newcomer.device_id),
                signing_pk: hex(&newcomer.signer.public()),
                ik: hex(&newcomer.ik()),
                label: "New device".into(),
                nonce: hex(&nonce),
                code: String::new(),
                request_digest: hex(&req_digest),
                received_ms: NOW_MS,
            }),
            introduced: vec![],
            history_sent: 0,
            roster_sent: 0,
            approval_undelivered: false,
        };
        approver
            .app
            .link_offers
            .insert("offer".into(), offer.clone());
        let approved = drive(&mut approver, &mut rng, NOW_MS, |tx| {
            tx.approve_inner(&request_id)
        });
        assert_eq!(approved, Ok(()), "the approval of a full-size account");
        let wire: Vec<Vec<u8>> = approver
            .app
            .outbox
            .iter()
            .filter_map(|e| match (&e.call, &e.purpose) {
                (NetCall::Deposit { bytes, mailbox, .. }, Purpose::DepositCeremony { .. })
                    if *mailbox == vec![3u8; 16] =>
                {
                    Some(bytes.clone())
                }
                _ => None,
            })
            .collect();
        let new_chain = approver.app.acs_chain.len();
        let new_creds = approver.app.acs_credentials.len();

        let mut fresh = ProfileState::empty("newcomer".into());
        fresh.device = Some(newcomer);
        fresh.app.link_join = Some(LinkJoin {
            operation_id: "ask".into(),
            secret: hex(&secret).into(),
            relay_uri: HOME.into(),
            a_cap: hex(&a_cap),
            b_mailbox: hex(&recv_mailbox(&b_recv)),
            b_recv: hex(&b_recv),
            created_ms: NOW_MS,
            expires_at: EXPIRES_MS,
            label: "New device".into(),
            nonce: hex(&nonce),
            request_id,
            code: String::new(),
            request_digest: hex(&req_digest),
            roster_seen: vec![],
            state: LinkState::AwaitingApproval,
            approved: false,
            roster_expected: 0,
            roster_done: 0,
            approver: String::new(),
            history_expected: 0,
            history_done: 0,
            history_ignored: 0,
            history_messages: 0,
            history_seen: vec![],
            history_degraded: false,
            cont: None,
        });
        Fx {
            rng,
            fresh,
            wire,
            offer,
            states: new_chain,
            creds: new_creds,
        }
    }

    fn keys(&self) -> CeremonyKeys {
        offer_keys(&self.offer).expect("keys")
    }

    /// The plaintext of one message on the wire.
    fn plain(&self, i: usize) -> V {
        cbor::decode(&open_msg(&self.keys(), ROLE_RESP, &self.wire[i]).expect("opens")).unwrap()
    }

    /// An authentic message with the given plaintext (sealed by the key holder).
    fn seal(&mut self, v: &V) -> Vec<u8> {
        let keys = self.keys();
        let mut st = ProfileState::empty("sealer".into());
        drive(&mut st, &mut self.rng, NOW_MS, |tx| {
            tx.seal_msg(&keys, ROLE_RESP, &cbor::encode(v), EXPIRES_MS / 1000)
        })
        .expect("seals")
    }

    /// Deliver messages to a copy of the new device's profile, at `now_ms`.
    fn feed(&mut self, st: &mut ProfileState, now_ms: u64, msgs: &[Vec<u8>]) {
        for m in msgs {
            drive(st, &mut self.rng, now_ms, |tx| tx.on_join_inbound(m));
        }
    }
}

fn adopted(st: &ProfileState) -> bool {
    !st.app.acs_chain.is_empty()
        || !st.app.acs_credentials.is_empty()
        || st.app.link_join.as_ref().is_some_and(|j| j.approved)
}

fn join_state(st: &ProfileState) -> LinkState {
    st.app.link_join.as_ref().expect("a join").state
}

fn tip_of(st: &ProfileState) -> [u8; 32] {
    let chain: Vec<Vec<u8>> = st.app.acs_chain.iter().map(|h| unhex(h).unwrap()).collect();
    let creds: Vec<Vec<u8>> = st
        .app
        .acs_credentials
        .iter()
        .map(|h| unhex(h).unwrap())
        .collect();
    control::verify_chain(&chain, &creds).unwrap().hash()
}

#[test]
fn a_65_state_account_with_272_credentials_links_end_to_end() {
    let mut fx = Fx::full();
    assert_eq!(fx.states, 65);
    assert_eq!(fx.creds, 272);
    assert!(fx.wire.len() > 2, "an approval and several chunks");
    // The approval itself stays small and carries a manifest, not the account.
    let approval = fx.plain(0);
    assert!(approval.opt(2).is_none() && approval.opt(3).is_none());
    assert!(approval.opt(12).is_some() && approval.opt(13).is_some());
    let n = cbor::encode(&approval).len();
    assert!(
        n < 16_384,
        "a single small message, not the account: {n} bytes"
    );

    let mut st = fx.fresh.clone();
    let wire = fx.wire.clone();
    fx.feed(&mut st, NOW_MS + 1000, &wire[..1]);
    assert!(!adopted(&st), "the approval alone adopts nothing");
    assert_eq!(join_state(&st), LinkState::AwaitingApproval);
    fx.feed(&mut st, NOW_MS + 2000, &wire[1..]);
    assert_eq!(join_state(&st), LinkState::Linked);
    assert_eq!(st.app.acs_chain.len(), 65);
    assert_eq!(st.app.acs_credentials.len(), 272);
    assert!(st.app.link_join.as_ref().unwrap().cont.is_none());
    let mine = st.device.as_ref().unwrap().device_id;
    let chain: Vec<Vec<u8>> = st.app.acs_chain.iter().map(|h| unhex(h).unwrap()).collect();
    let creds: Vec<Vec<u8>> = st
        .app
        .acs_credentials
        .iter()
        .map(|h| unhex(h).unwrap())
        .collect();
    let head = control::verify_chain(&chain, &creds).expect("the adopted chain verifies");
    assert!(head.messaging.contains(&mine));
    assert_eq!(
        head.credentials.len(),
        33,
        "the removed devices' credentials are kept as bodies, not listed"
    );
    assert_eq!(creds.len(), 272);
}

#[test]
fn a_small_account_keeps_the_inline_approval() {
    let fx = Fx::build(2, 1, None, 2);
    assert_eq!(fx.wire.len(), 1, "no continuation for an account that fits");
    let approval = fx.plain(0);
    assert!(approval.opt(2).is_some() && approval.opt(3).is_some());
    assert!(approval.opt(12).is_none() && approval.opt(13).is_none());
}

#[test]
fn repeated_and_reordered_chunks_still_link_exactly_once() {
    let mut fx = Fx::full();
    let wire = fx.wire.clone();
    // Duplicates of every chunk, in reverse order, with the approval first (as the queue delivers it).
    let mut msgs = vec![wire[0].clone()];
    for c in wire[1..].iter().rev() {
        msgs.push(c.clone());
        msgs.push(c.clone());
    }
    msgs.push(wire[0].clone()); // a replayed approval is a no-op
    let mut st = fx.fresh.clone();
    fx.feed(&mut st, NOW_MS + 1000, &msgs);
    assert_eq!(join_state(&st), LinkState::Linked);
    assert_eq!(st.app.acs_chain.len(), 65);
    let tip = tip_of(&st);
    // The same transfer, in order, gives the same account.
    let mut st2 = fx.fresh.clone();
    fx.feed(&mut st2, NOW_MS + 1000, &wire);
    assert_eq!(tip_of(&st2), tip);
}

#[test]
fn a_missing_chunk_adopts_nothing_and_the_ceremony_ends_at_expiry() {
    let mut fx = Fx::full();
    for skip in [1, fx.wire.len() / 2, fx.wire.len() - 1] {
        let msgs: Vec<Vec<u8>> = fx
            .wire
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != skip)
            .map(|(_, m)| m.clone())
            .collect();
        let mut st = fx.fresh.clone();
        fx.feed(&mut st, NOW_MS + 1000, &msgs);
        assert!(!adopted(&st), "chunk {skip} withheld");
        assert_eq!(join_state(&st), LinkState::AwaitingApproval);
        // Time runs out: the ceremony is over and the buffer is gone.
        drive(&mut st, &mut fx.rng, EXPIRES_MS, |tx| {
            tx.expire_ceremonies()
        });
        assert!(!adopted(&st));
        assert!(matches!(
            join_state(&st),
            LinkState::Failed | LinkState::Expired
        ));
        let j = st.app.link_join.as_ref().unwrap();
        assert!(j.cont.is_none() && j.secret.is_empty());
        // A chunk that arrives after expiry is not accepted.
        let late = fx.wire[skip].clone();
        fx.feed(&mut st, EXPIRES_MS + 1, &[late]);
        assert!(!adopted(&st));
    }
}

#[test]
fn chunks_that_arrive_after_expiry_adopt_nothing() {
    let mut fx = Fx::full();
    let wire = fx.wire.clone();
    let mut st = fx.fresh.clone();
    fx.feed(&mut st, NOW_MS + 1000, &wire[..2]);
    fx.feed(&mut st, EXPIRES_MS, &wire[2..]);
    assert!(!adopted(&st));
    assert!(
        st.app.link_join.as_ref().unwrap().cont.is_some() || join_state(&st) != LinkState::Linked
    );
}

#[test]
fn an_altered_chunk_is_dropped_and_a_forged_one_ends_the_ceremony() {
    let mut fx = Fx::full();
    let wire = fx.wire.clone();
    // Ciphertext altered in transit: it does not open, so it is as if it never came.
    let mut bad = wire.clone();
    let mid = bad[2].len() / 2;
    bad[2][mid] ^= 1;
    let mut st = fx.fresh.clone();
    fx.feed(&mut st, NOW_MS + 1000, &bad);
    assert!(
        !adopted(&st),
        "a chunk that does not open leaves the set incomplete"
    );
    assert_eq!(join_state(&st), LinkState::AwaitingApproval);

    // Authentic frames with the wrong content: one state byte changed.
    let mut forged = wire.clone();
    let mut v = fx.plain(2);
    if let V::M(m) = &mut v {
        for (k, val) in m.iter_mut() {
            if *k == 2
                && let V::A(items) = val
                && let Some(V::B(b)) = items.first_mut()
            {
                let n = b.len() - 1;
                b[n] ^= 1;
            }
        }
    }
    forged[2] = fx.seal(&v);
    let mut st = fx.fresh.clone();
    fx.feed(&mut st, NOW_MS + 1000, &forged);
    assert!(
        !adopted(&st),
        "the digest of the set does not match the manifest"
    );
    assert_eq!(join_state(&st), LinkState::Failed);
    assert!(st.app.link_join.as_ref().unwrap().cont.is_none());

    // Two chunks with their indices exchanged: the assembled order differs, the digest does not match.
    let (mut a, mut b) = (fx.plain(1), fx.plain(2));
    for (v, idx) in [(&mut a, 1u64), (&mut b, 0u64)] {
        if let V::M(m) = v {
            for (k, val) in m.iter_mut() {
                if *k == 1 {
                    *val = V::U(idx);
                }
            }
        }
    }
    let mut swapped = wire.clone();
    swapped[1] = fx.seal(&a);
    swapped[2] = fx.seal(&b);
    let mut st = fx.fresh.clone();
    fx.feed(&mut st, NOW_MS + 1000, &swapped);
    assert!(!adopted(&st));
    assert_eq!(join_state(&st), LinkState::Failed);
}

#[test]
fn a_manifest_with_the_wrong_tag_or_bounds_is_refused_on_arrival() {
    let mut fx = Fx::full();
    let wire = fx.wire.clone();
    let edit = |fx: &mut Fx, f: &dyn Fn(&mut Fields)| {
        let mut v = fx.plain(0);
        if let V::M(m) = &mut v {
            f(m);
        }
        fx.seal(&v)
    };
    let cases: Vec<Vec<u8>> = vec![
        // The tag of the manifest is wrong.
        edit(&mut fx, &|m| {
            for (k, val) in m.iter_mut() {
                if *k == 13 {
                    *val = V::B(vec![0; 32]);
                }
            }
        }),
        // More chunks announced than the bound allows (the tag then no longer matches either).
        edit(&mut fx, &|m| {
            for (k, val) in m.iter_mut() {
                if *k == 12 {
                    *val = V::A(vec![
                        V::B(vec![0; 32]),
                        V::U(65),
                        V::U(272),
                        V::U(CONT_MAX_CHUNKS as u64 + 1),
                    ]);
                }
            }
        }),
        // The chain is also inline: one form only.
        edit(&mut fx, &|m| m.push((2, V::A(vec![])))),
        // The inner binding is changed; the manifest tag covers it.
        edit(&mut fx, &|m| {
            for (k, val) in m.iter_mut() {
                if *k == 11 {
                    *val = V::B(vec![9; 32]);
                }
            }
        }),
    ];
    for (i, case) in cases.into_iter().enumerate() {
        let mut st = fx.fresh.clone();
        fx.feed(&mut st, NOW_MS + 1000, &[case]);
        fx.feed(&mut st, NOW_MS + 1000, &wire[1..]);
        assert!(!adopted(&st), "case {i}");
        assert_eq!(join_state(&st), LinkState::Failed, "case {i}");
    }
}

#[test]
fn a_restart_in_the_middle_of_the_transfer_resumes_and_still_verifies_everything() {
    let mut fx = Fx::full();
    let wire = fx.wire.clone();
    let half = wire.len() / 2;
    let mut st = fx.fresh.clone();
    fx.feed(&mut st, NOW_MS + 1000, &wire[..half]);
    assert!(!adopted(&st));
    // Restart: the application state goes through its stored form, the device keys are reloaded.
    let json = serde_json::to_string(&st.app).expect("stored");
    let mut again = ProfileState::empty("newcomer".into());
    again.device = st.device.clone();
    again.app = serde_json::from_str(&json).expect("loaded");
    assert_eq!(
        again
            .app
            .link_join
            .as_ref()
            .unwrap()
            .cont
            .as_ref()
            .map(|c| c.got.len()),
        Some(half - 1)
    );
    fx.feed(&mut again, NOW_MS + 5000, &wire[half..]);
    assert_eq!(join_state(&again), LinkState::Linked);
    assert_eq!(again.app.acs_chain.len(), 65);

    // A restart that outlives the ceremony does not resume it.
    let mut late = ProfileState::empty("newcomer".into());
    late.device = st.device.clone();
    late.app = serde_json::from_str(&json).expect("loaded");
    drive(&mut late, &mut fx.rng, EXPIRES_MS + 1, |tx| {
        tx.expire_ceremonies()
    });
    fx.feed(&mut late, EXPIRES_MS + 2, &wire[half..]);
    assert!(!adopted(&late));
}
