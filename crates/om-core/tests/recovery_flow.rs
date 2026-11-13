// SPDX-License-Identifier: MPL-2.0
//! Recovery successor verification (work group E, slice 2): V-002 recovery rows over real signed states, conformance
//! ID-03 (a verifier that was withheld the request waits its OWN full interval) and ID-06 (a revoked DeviceID never
//! returns), plus the genesis -> request -> completion lifecycle.
use om_core::control::{
    self, Ctx, Recovery, RecoveryObservation, RecoveryVerdict, Verdict, verify_recovery_successor,
};
use om_core::crypto::binding::Acs;
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::store::Device;
use om_core::recovery::{
    self, RecoveryPolicy, RecoveryRequest, RecoveryRoot, RecoveryVeto, sign_request, sign_veto,
};
use std::collections::BTreeSet;

const DELAY: u64 = 259_200;

struct World {
    rng: InsecureTestRng,
    devs: Vec<Device>,
    new_dev: Device,
    parent: Acs,
    root: RecoveryRoot,
    policy: RecoveryPolicy,
}

fn world(label: &[u8], n: usize) -> World {
    let mut rng = InsecureTestRng::from_label(label);
    let first = Device::new(&mut rng);
    let mut devs = vec![first];
    while devs.len() < n {
        let mut d = Device::new(&mut rng);
        d.account_id = devs[0].account_id;
        devs.push(d);
    }
    let mut new_dev = Device::new(&mut rng);
    new_dev.account_id = devs[0].account_id;
    let root = RecoveryRoot::generate(&mut rng);
    let policy = RecoveryPolicy::default_for(root.auth_public_key(0), 0);
    let mut parent = devs[0].acs();
    parent.control = devs.iter().map(|d| d.device_id).collect();
    parent.credentials = devs
        .iter()
        .map(|d| (d.device_id, d.credential().hash()))
        .collect();
    parent.recovery_commitment = Some(policy.hash());
    World {
        rng,
        devs,
        new_dev,
        parent,
        root,
        policy,
    }
}

impl World {
    fn creds(&self) -> Vec<Vec<u8>> {
        let mut c: Vec<_> = self.devs.iter().map(|d| d.credential().body()).collect();
        c.push(self.new_dev.credential().body());
        c
    }
    fn next_policy(&self) -> RecoveryPolicy {
        let mut p = RecoveryPolicy::default_for(self.root.auth_public_key(1), 1);
        p.delay_seconds = self.policy.delay_seconds;
        p
    }
    fn child(&self) -> Acs {
        let p = &self.parent;
        let mut c = p.clone();
        c.generation = p.generation + 1;
        c.previous = Some(p.hash());
        let mut revoked = p.revoked.clone();
        revoked.extend(p.control.iter().chain(p.messaging.iter()));
        c.control = BTreeSet::from([self.new_dev.device_id]);
        c.quarantined.clear();
        c.messaging.clear();
        c.revoked = revoked;
        c.credentials = vec![(self.new_dev.device_id, self.new_dev.credential().hash())];
        c.key_epochs.clear();
        c.recovery_commitment = Some(self.next_policy().hash());
        c
    }
    fn request_for(&self, parent_hash: [u8; 32], epoch: u64) -> RecoveryRequest {
        RecoveryRequest {
            account_id: self.parent.account_id,
            recovery_epoch: epoch,
            new_device_id: self.new_dev.device_id,
            new_device_credential: self.new_dev.credential().hash(),
            parent_state_hash: parent_hash,
            request_nonce: [9; 16],
        }
    }
    fn request(&mut self) -> Vec<u8> {
        let req = self.request_for(self.parent.hash(), 0);
        sign_request(&req, &self.root.auth_signer(0), &mut self.rng)
    }
    fn sign_child(&mut self, child: &Acs, cosigners: &[usize]) -> Vec<u8> {
        let cs: Vec<_> = cosigners
            .iter()
            .map(|i| (&self.devs[*i].device_id, &self.devs[*i].signer))
            .collect();
        control::sign_acs_recovery(
            &child.body(),
            &child.account_id,
            &self.root.auth_signer(0),
            &cs,
            &mut self.rng,
        )
    }
    fn verify(
        &self,
        child: &[u8],
        req: &[u8],
        elapsed: u64,
        vetoes: &[Vec<u8>],
    ) -> Result<RecoveryVerdict, om_core::crypto::Error> {
        verify_recovery_successor(
            &self.parent,
            child,
            &self.creds(),
            &self.policy,
            &self.next_policy(),
            req,
            &RecoveryObservation {
                elapsed_seconds: elapsed,
                vetoes,
            },
        )
    }
    fn veto_by(&mut self, i: usize, req: &[u8]) -> Vec<u8> {
        let r = recovery::open_request(req, self.policy.recovery_key().unwrap()).unwrap();
        let v = RecoveryVeto {
            account_id: r.account_id,
            recovery_epoch: r.recovery_epoch,
            request_digest: r.digest(),
        };
        sign_veto(
            &v,
            &self.devs[i].device_id,
            &self.devs[i].signer,
            &mut self.rng,
        )
    }
}

fn complete(v: Result<RecoveryVerdict, om_core::crypto::Error>) -> (Acs, bool) {
    match v.expect("accepted") {
        RecoveryVerdict::Complete { child, immediate } => (child, immediate),
        other => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn lifecycle_genesis_request_completion_alone_after_the_full_delay() {
    let mut w = world(b"rec-lifecycle", 1);
    let req = w.request();
    let child = w.child();
    let signed = w.sign_child(&child, &[]);
    // Before the delay: pending, with the remaining time of THIS verifier's interval.
    assert_eq!(
        w.verify(&signed, &req, 0, &[]).unwrap(),
        RecoveryVerdict::Pending {
            remaining_seconds: DELAY
        }
    );
    assert_eq!(
        w.verify(&signed, &req, DELAY - 1, &[]).unwrap(),
        RecoveryVerdict::Pending {
            remaining_seconds: 1
        }
    );
    let (done, immediate) = complete(w.verify(&signed, &req, DELAY, &[]));
    assert!(!immediate);
    assert_eq!(done, child);
    // The new device is the sole controller, the old one is permanently revoked, the epoch rotated.
    assert_eq!(done.control, BTreeSet::from([w.new_dev.device_id]));
    assert!(done.revoked.contains(&w.devs[0].device_id));
    assert_eq!(done.generation, w.parent.generation + 1);
    assert_eq!(done.previous, Some(w.parent.hash()));
    assert_eq!(done.recovery_commitment, Some(w.next_policy().hash()));
    assert_eq!(w.next_policy().epoch(), 1);
}

#[test]
fn a_controller_cosignature_completes_immediately() {
    let mut w = world(b"rec-cosign", 3);
    let req = w.request();
    let child = w.child();
    let signed = w.sign_child(&child, &[1]);
    // recovery vote + one co-signer = 2 = floor(3/2)+1, with no waiting at all.
    let (_, immediate) = complete(w.verify(&signed, &req, 0, &[]));
    assert!(immediate);
    // A quarantined controller cannot co-sign.
    let mut q = world(b"rec-cosign-q", 3);
    q.parent.quarantined.insert(q.devs[1].device_id);
    let req = q.request();
    let child = q.child();
    let signed = q.sign_child(&child, &[1]);
    assert!(q.verify(&signed, &req, DELAY, &[]).is_err());
}

#[test]
fn partial_cosign_is_not_a_majority_for_larger_accounts() {
    // N=5: majority 3; recovery + one co-signer is 2, and the V-002 relation does not fall back to the delay path.
    let mut w = world(b"rec-n5", 5);
    let req = w.request();
    let child = w.child();
    let signed = w.sign_child(&child, &[1]);
    assert!(w.verify(&signed, &req, DELAY, &[]).is_err());
    let signed = w.sign_child(&child, &[1, 2]);
    assert!(complete(w.verify(&signed, &req, 0, &[])).1);
}

#[test]
fn a_veto_by_a_live_controller_aborts_even_after_the_delay_or_with_a_cosign() {
    let mut w = world(b"rec-veto", 3);
    let req = w.request();
    let child = w.child();
    let alone = w.sign_child(&child, &[]);
    let cosigned = w.sign_child(&child, &[1]);
    let veto = w.veto_by(2, &req);
    assert!(
        w.verify(&alone, &req, DELAY * 10, std::slice::from_ref(&veto))
            .is_err()
    );
    assert!(w.verify(&cosigned, &req, 0, &[veto]).is_err());
    // Without it the same recovery alone completes after the delay.
    assert!(w.verify(&alone, &req, DELAY, &[]).is_ok());
}

#[test]
fn only_a_valid_veto_of_this_request_by_a_live_controller_counts() {
    let mut w = world(b"rec-veto-ignored", 3);
    w.parent.quarantined.insert(w.devs[2].device_id);
    let req = w.request();
    let child = w.child();
    let alone = w.sign_child(&child, &[]);
    // A quarantined controller is not live: its veto is ignored.
    let q = w.veto_by(2, &req);
    // A veto naming another request digest is ignored.
    let other_req = {
        let r = w.request_for(w.parent.hash(), 0);
        RecoveryRequest {
            request_nonce: [1; 16],
            ..r
        }
    };
    let other_veto = {
        let v = RecoveryVeto {
            account_id: other_req.account_id,
            recovery_epoch: 0,
            request_digest: other_req.digest(),
        };
        sign_veto(&v, &w.devs[1].device_id, &w.devs[1].signer, &mut w.rng)
    };
    // A veto signed with a key the parent does not commit for that id is ignored.
    let forged = {
        let r = recovery::open_request(&req, w.policy.recovery_key().unwrap()).unwrap();
        let v = RecoveryVeto {
            account_id: r.account_id,
            recovery_epoch: 0,
            request_digest: r.digest(),
        };
        sign_veto(&v, &w.devs[1].device_id, &w.devs[0].signer, &mut w.rng)
    };
    let junk = vec![0xffu8; 10];
    assert!(
        w.verify(&alone, &req, DELAY, &[q, other_veto, forged, junk])
            .is_ok()
    );
}

#[test]
fn id_03_a_verifier_withheld_the_request_waits_its_own_full_interval() {
    let mut w = world(b"rec-id03", 1);
    let req = w.request();
    let signed = w.sign_child(&w.child(), &[]);
    // Verifier A observed the request for the whole delay and accepts.
    assert!(complete(w.verify(&signed, &req, DELAY, &[])).0.generation > 0);
    // Verifier B first sees the same bytes later: its own interval has only just begun. Another verifier having
    // accepted (and any request or Home timestamp) changes nothing for it.
    for elapsed in [0, 1, DELAY / 2, DELAY - 1] {
        assert!(matches!(
            w.verify(&signed, &req, elapsed, &[]).unwrap(),
            RecoveryVerdict::Pending { .. }
        ));
    }
}

#[test]
fn id_06_a_revoked_device_id_can_never_be_the_recovery_target() {
    let mut w = world(b"rec-id06", 2);
    // The target was revoked by an earlier transition; the same hardware must enrol under a fresh DeviceID.
    w.parent.revoked.insert(w.new_dev.device_id);
    w.parent.recovery_commitment = Some(w.policy.hash());
    let req = w.request();
    let child = w.child();
    let signed = w.sign_child(&child, &[0]);
    assert!(w.verify(&signed, &req, DELAY, &[]).is_err());
}

#[test]
fn stale_target_enrolled_or_parent_advanced_meanwhile() {
    // The request names a device that another transition enrolled in the meantime.
    let mut w = world(b"rec-stale", 2);
    let req = w.request();
    let old_parent = w.parent.clone();
    w.parent.messaging.insert(w.new_dev.device_id);
    w.parent.generation += 1;
    w.parent.previous = Some(old_parent.hash());
    w.parent
        .credentials
        .push((w.new_dev.device_id, w.new_dev.credential().hash()));
    let child = w.child();
    let signed = w.sign_child(&child, &[0]);
    assert!(w.verify(&signed, &req, DELAY, &[]).is_err());
    // Any other transition moves the parent hash: the old request no longer binds.
    let mut w = world(b"rec-stale2", 2);
    let req = w.request();
    w.parent.generation += 1;
    w.parent.transparency = Some([5; 32]);
    let signed = w.sign_child(&w.child(), &[0]);
    assert!(w.verify(&signed, &req, DELAY, &[]).is_err());
}

#[test]
fn request_bindings_epoch_parent_hash_policy_and_signer() {
    let mut w = world(b"rec-binding", 1);
    let child = w.child();
    let signed = w.sign_child(&child, &[]);
    let sign = |w: &mut World, r: RecoveryRequest, epoch: u64| {
        sign_request(&r, &w.root.auth_signer(epoch), &mut w.rng)
    };
    // Wrong epoch in the request (signed with the epoch key it claims).
    let r = w.request_for(w.parent.hash(), 1);
    let bad = sign(&mut w, r, 1);
    assert!(w.verify(&signed, &bad, DELAY, &[]).is_err());
    // Wrong parent state hash.
    let r = w.request_for([0xee; 32], 0);
    let bad = sign(&mut w, r, 0);
    assert!(w.verify(&signed, &bad, DELAY, &[]).is_err());
    // Another account.
    let mut r = w.request_for(w.parent.hash(), 0);
    r.account_id = [0x42; 32];
    let bad = sign(&mut w, r, 0);
    assert!(w.verify(&signed, &bad, DELAY, &[]).is_err());
    // A request naming a different credential than the child commits to.
    let mut r = w.request_for(w.parent.hash(), 0);
    r.new_device_credential = [3; 32];
    let bad = sign(&mut w, r, 0);
    assert!(w.verify(&signed, &bad, DELAY, &[]).is_err());
    // Signed by another root.
    let other = RecoveryRoot::from_bytes([0x77; 32]);
    let r = w.request_for(w.parent.hash(), 0);
    let bad = sign_request(&r, &other.auth_signer(0), &mut w.rng);
    assert!(w.verify(&signed, &bad, DELAY, &[]).is_err());
    // A policy that is not the committed one, or is not on.
    let req = w.request();
    let mut other_policy = w.policy.clone();
    other_policy.delay_seconds = 86_400;
    assert!(
        verify_recovery_successor(
            &w.parent,
            &signed,
            &w.creds(),
            &other_policy,
            &w.next_policy(),
            &req,
            &RecoveryObservation {
                elapsed_seconds: DELAY,
                vetoes: &[]
            }
        )
        .is_err()
    );
    // A successor signed only by a controller (no recovery signature) is not a recovery successor.
    let controller_only = control::sign_acs(
        &child.body(),
        &[(&w.devs[0].device_id, &w.devs[0].signer)],
        &mut w.rng,
    );
    assert!(w.verify(&controller_only, &req, DELAY, &[]).is_err());
    // ... and the plain verifier never counts a recovery-role signature.
    assert!(control::verify_successor(&w.parent, &signed, &w.creds()).is_err());
    // Recovery signature over a different body.
    let mut tampered = child.clone();
    tampered.generation += 1;
    let tampered_signed = {
        let good = w.sign_child(&child, &[]);
        let (v, _) = (om_core::crypto::cbor::decode(&good).unwrap(), ());
        let sigs = v.get(2).unwrap().clone();
        om_core::crypto::cbor::encode(&om_core::crypto::cbor::V::M(vec![
            (1, om_core::crypto::cbor::V::B(tampered.body())),
            (2, sigs),
        ]))
    };
    assert!(w.verify(&tampered_signed, &req, DELAY, &[]).is_err());
}

#[test]
fn completion_shape_and_commitment_rotation_are_enforced() {
    let mut w = world(b"rec-shape", 2);
    let req = w.request();
    let check = |w: &mut World, f: &dyn Fn(&mut Acs)| {
        let mut c = w.child();
        f(&mut c);
        let signed = w.sign_child(&c, &[0]);
        w.verify(&signed, &req, DELAY, &[]).is_err()
    };
    let ok = {
        let c = w.child();
        w.sign_child(&c, &[0])
    };
    assert!(w.verify(&ok, &req, 0, &[]).is_ok());
    let old = w.devs[0].device_id;
    let parent_commitment = w.parent.recovery_commitment;
    let extra = [0xAB; 16];
    assert!(
        check(&mut w, &|c| {
            c.control.insert(old);
            c.revoked.remove(&old);
        }),
        "earlier controller kept"
    );
    assert!(
        check(&mut w, &|c| {
            c.messaging.insert(extra);
        }),
        "extra messaging device"
    );
    assert!(check(&mut w, &|c| {
        c.quarantined
            .insert(c.control.iter().next().copied().unwrap());
    }));
    assert!(
        check(&mut w, &|c| {
            c.revoked.remove(&old);
        }),
        "an earlier controller is not revoked"
    );
    assert!(
        check(&mut w, &|c| {
            c.recovery_commitment = Some([1; 32]);
        }),
        "commitment is not the next policy"
    );
    assert!(
        check(&mut w, &|c| {
            c.recovery_commitment = parent_commitment;
        }),
        "commitment not rotated"
    );
    assert!(check(&mut w, &|c| {
        c.recovery_commitment = None;
    }));
    assert!(check(&mut w, &|c| {
        c.generation += 1;
    }));
    assert!(check(&mut w, &|c| {
        c.previous = Some([1; 32]);
    }));
    assert!(
        check(&mut w, &|c| {
            c.constitutional = (2, Some(1));
        }),
        "constitutional change takes no recovery vote"
    );
    assert!(check(&mut w, &|c| {
        c.transparency = Some([8; 32]);
    }));
    // A next policy that skips an epoch or changes the delay is refused.
    let mut skip = RecoveryPolicy::default_for(w.root.auth_public_key(2), 2);
    skip.delay_seconds = DELAY;
    let mut c = w.child();
    c.recovery_commitment = Some(skip.hash());
    let signed_skip = w.sign_child(&c, &[0]);
    let r = verify_recovery_successor(
        &w.parent,
        &signed_skip,
        &w.creds(),
        &w.policy,
        &skip,
        &req,
        &RecoveryObservation {
            elapsed_seconds: DELAY,
            vetoes: &[],
        },
    );
    assert!(r.is_err(), "epoch must increment by exactly one");
}

#[test]
fn a_recovery_vote_never_resolves_a_safety_freeze_alone_or_as_a_completion() {
    // N=3, two quarantined: voting 1 < majority 2, so the parent is frozen.
    let mut w = world(b"rec-freeze", 3);
    w.parent.quarantined.insert(w.devs[1].device_id);
    w.parent.quarantined.insert(w.devs[2].device_id);
    let req = w.request();
    let child = w.child();
    for cos in [vec![], vec![0]] {
        let signed = w.sign_child(&child, &cos);
        assert!(w.verify(&signed, &req, DELAY * 5, &[]).is_err());
    }
    // The relation itself (V-002 rows): alone never, and a co-signed vote only for a restore/revoke resolution.
    let pm = control::model_of(&w.parent).unwrap();
    let cm = control::model_of(&child).unwrap();
    let alone = Ctx {
        recovery: Some(Recovery {
            elapsed: DELAY * 5,
            delay: DELAY,
            veto: false,
        }),
        ..Ctx::default()
    };
    assert!(matches!(
        control::authorize(&pm, &cm, &alone),
        Verdict::Reject(_)
    ));
}

#[test]
fn a_recovery_vote_never_counts_for_a_fork_bridge() {
    let w = world(b"rec-bridge", 3);
    let pm = control::model_of(&w.parent).unwrap();
    let mut cm = pm.clone();
    cm.generation += 1;
    cm.fork_ack = Some(vec!["a".into(), "b".into()]);
    let signers: BTreeSet<String> = pm.control.iter().take(1).cloned().collect();
    let ctx = Ctx {
        signers,
        recovery: Some(Recovery {
            elapsed: DELAY * 2,
            delay: DELAY,
            veto: false,
        }),
        ..Ctx::default()
    };
    assert!(
        matches!(control::authorize(&pm, &cm, &ctx), Verdict::Reject(_)),
        "vote + one device is not a bridge"
    );
    let all: BTreeSet<String> = pm.control.iter().take(2).cloned().collect();
    let ctx = Ctx {
        signers: all,
        recovery: Some(Recovery {
            elapsed: DELAY * 2,
            delay: DELAY,
            veto: false,
        }),
        ..Ctx::default()
    };
    assert!(
        matches!(control::authorize(&pm, &cm, &ctx), Verdict::Reject(_)),
        "a recovery vote is refused outright on a bridge"
    );
}
