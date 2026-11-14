// SPDX-License-Identifier: MPL-2.0
//! The real MLS engine (stock OpenMLS 0.9.0) behind `GroupCrypto`, driven directly: three devices on two accounts
//! (A1; B1 and B2). DEPENDS ON ADR-0187 (a proposed delegated draft, not accepted). The Core-level scenarios
//! (`groups_loopback.rs`, `groups_flow.rs`) run against this engine too, with `--features groups-openmls`.
#![cfg(feature = "groups-openmls")]

use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::groups::mls::*;
use om_core::groups::openmls::{OPENMLS_TAG, OpenMlsGroupCrypto, OpenMlsLoader};

const GID: &[u8] = &[0x6a; 24];
const CTX: &[u8] = &[9; 32];

struct D {
    e: OpenMlsGroupCrypto,
    rng: InsecureTestRng,
}
impl D {
    fn new(n: u8) -> D {
        D {
            e: OpenMlsGroupCrypto::new(&[n; 16]).unwrap(),
            rng: InsecureTestRng::from_label(&[b'o', n]),
        }
    }
}
macro_rules! env {
    ($d:expr) => {
        &mut CryptoEnv {
            rng: &mut $d.rng,
            now_ms: 0,
        }
    };
}

fn world() -> (D, D, D) {
    let (mut a, mut b1, mut b2) = (D::new(0xa1), D::new(0xb1), D::new(0xb2));
    a.e.create_group(env!(a), GID, CTX).unwrap();
    let k1 = b1.e.key_package(env!(b1)).unwrap();
    let k2 = b2.e.key_package(env!(b2)).unwrap();
    let p = a.e.add_commit(env!(a), GID, &[k1.bytes, k2.bytes]).unwrap();
    assert_eq!(a.e.pending(GID).unwrap().unwrap(), p);
    assert_eq!(p.resulting_epoch, 1);
    a.e.merge_pending(GID).unwrap();
    let w = p.welcome.unwrap();
    for b in [&mut b1, &mut b2] {
        let (gid, info) = b.e.process_welcome(env!(b), &w).unwrap();
        assert_eq!((gid.as_slice(), info.epoch), (GID, 1));
    }
    (a, b1, b2)
}

#[test]
fn three_devices_converge_with_equal_exporter_and_members() {
    let (a, b1, b2) = world();
    let x = a.e.export_secret(GID, "om-test", b"ctx", 32).unwrap();
    assert_eq!(x.len(), 32);
    for d in [&b1, &b2] {
        assert_eq!(d.e.export_secret(GID, "om-test", b"ctx", 32).unwrap(), x);
        assert_eq!(d.e.epoch(GID).unwrap(), 1);
        assert_eq!(d.e.members(GID).unwrap(), a.e.members(GID).unwrap());
    }
    assert_eq!(
        a.e.members(GID).unwrap(),
        vec![vec![0xa1; 16], vec![0xb1; 16], vec![0xb2; 16]]
    );
    // a different label or context gives a different secret
    assert_ne!(a.e.export_secret(GID, "om-other", b"ctx", 32).unwrap(), x);
    assert_ne!(a.e.export_secret(GID, "om-test", b"x", 32).unwrap(), x);
}

#[test]
fn key_packages_are_distinct_and_single_use() {
    let (mut a, mut b1, _b2) = world();
    let k1 = b1.e.key_package(env!(b1)).unwrap();
    let k2 = b1.e.key_package(env!(b1)).unwrap();
    assert_ne!(k1.bytes, k2.bytes);
    assert_eq!(k1.reference, om_core::crypto::sha256(&[&k1.bytes]));
    let mut c = D::new(0xc1);
    let kc = c.e.key_package(env!(c)).unwrap();
    let p = a.e.add_commit(env!(a), GID, &[kc.bytes]).unwrap();
    a.e.merge_pending(GID).unwrap();
    let w = p.welcome.unwrap();
    c.e.process_welcome(env!(c), &w).unwrap();
    assert_eq!(
        c.e.process_welcome(env!(c), &w).unwrap_err().0,
        "no_matching_key_package"
    );
}

#[test]
fn receivers_apply_a_commit_and_a_removed_device_learns_it() {
    let (mut a, mut b1, mut b2) = world();
    let p = a.e.remove_commit(env!(a), GID, &[vec![0xb2; 16]]).unwrap();
    // a receiver can read it before applying, and a second pending is refused
    let info = b1.e.inspect_commit(GID, &p.commit).unwrap();
    assert_eq!(
        (
            info.from_epoch,
            info.resulting_epoch,
            info.committer.clone()
        ),
        (1, 2, vec![0xa1; 16])
    );
    // the author reads its own pending Commit without the update-path leaf (Core made it; nothing to prove)
    let own = a.e.inspect_commit(GID, &p.commit).unwrap();
    assert_eq!(
        (own.from_epoch, own.resulting_epoch, own.committer),
        (1, 2, vec![0xa1; 16])
    );
    assert!(own.added.is_empty());
    assert_eq!(
        a.e.add_commit(env!(a), GID, &[]).unwrap_err().0,
        "commit_pending"
    );
    assert_eq!(
        a.e.encrypt_application(env!(a), GID, b"x").unwrap_err().0,
        "commit_pending"
    );
    a.e.merge_pending(GID).unwrap();
    assert_eq!(
        b1.e.process_commit(env!(b1), GID, &p.commit).unwrap().epoch,
        2
    );
    let gone = b2.e.process_commit(env!(b2), GID, &p.commit).unwrap();
    assert!(!gone.still_member);
    assert_eq!(
        b2.e.encrypt_application(env!(b2), GID, b"x").unwrap_err().0,
        "unknown_group"
    );
    assert_eq!(
        a.e.export_secret(GID, "l", b"", 16).unwrap(),
        b1.e.export_secret(GID, "l", b"", 16).unwrap()
    );
    // the removed device cannot read a later message
    let ct = a.e.encrypt_application(env!(a), GID, b"after").unwrap();
    assert!(b2.e.decrypt_application(GID, &ct).is_err());
    assert_eq!(
        b1.e.decrypt_application(GID, &ct).unwrap().plaintext,
        b"after"
    );
    // an old-epoch commit is refused, and a committer cannot remove itself
    assert_eq!(
        b1.e.process_commit(env!(b1), GID, &p.commit).unwrap_err().0,
        "wrong_epoch"
    );
    assert_eq!(
        a.e.remove_commit(env!(a), GID, &[vec![0xa1; 16]])
            .unwrap_err()
            .0,
        "cannot_remove_self"
    );
    assert_eq!(
        a.e.remove_commit(env!(a), GID, &[vec![0x77; 16]])
            .unwrap_err()
            .0,
        "not_a_member"
    );
}

#[test]
fn application_messages_round_trip_with_sender_and_epoch() {
    let (mut a, mut b1, mut b2) = world();
    let ct = a.e.encrypt_application(env!(a), GID, b"hello").unwrap();
    assert!(!ct.windows(5).any(|w| w == b"hello"));
    for b in [&mut b1, &mut b2] {
        let d = b.e.decrypt_application(GID, &ct).unwrap();
        assert_eq!(
            (d.plaintext.as_slice(), d.sender, d.epoch),
            (&b"hello"[..], vec![0xa1; 16], 1)
        );
    }
    // a replayed ciphertext is refused by the engine itself
    assert!(b1.e.decrypt_application(GID, &ct).is_err());
    // another group is unknown
    assert_eq!(
        b1.e.decrypt_application(&[1; 24], &ct).unwrap_err().0,
        "unknown_group"
    );
}

#[test]
fn a_discarded_commit_leaves_the_epoch_and_a_new_one_can_be_made() {
    let (mut a, _b1, _b2) = world();
    let p = a.e.remove_commit(env!(a), GID, &[vec![0xb1; 16]]).unwrap();
    a.e.discard_pending(GID).unwrap();
    assert_eq!(a.e.pending(GID).unwrap(), None);
    assert_eq!(a.e.epoch(GID).unwrap(), 1);
    let q = a.e.remove_commit(env!(a), GID, &[vec![0xb1; 16]]).unwrap();
    assert_ne!(p.commit, q.commit);
}

#[test]
fn snapshot_restores_a_pending_commit_byte_for_byte_and_the_engine_keeps_working() {
    let (mut a, mut b1, _b2) = world();
    let p = a.e.remove_commit(env!(a), GID, &[vec![0xb2; 16]]).unwrap();
    let snap = a.e.snapshot();
    assert!(snap.starts_with(b"OMMLSv01"));
    let mut a2 = OpenMlsLoader::restore(&snap).unwrap();
    assert_eq!(a2.engine_tag(), OPENMLS_TAG);
    assert_eq!(a2.device_id(), &[0xa1; 16]);
    assert_eq!(a2.pending(GID).unwrap().unwrap(), p);
    assert_eq!(a2.snapshot(), snap, "equal state gives equal bytes");
    let mut r = InsecureTestRng::from_label(b"r");
    let e = &mut CryptoEnv {
        rng: &mut r,
        now_ms: 0,
    };
    a2.merge_pending(GID).unwrap();
    b1.e.process_commit(env!(b1), GID, &p.commit).unwrap();
    // the restored engine can sign and encrypt in the new epoch
    let ct = a2.encrypt_application(e, GID, b"restored").unwrap();
    assert_eq!(
        b1.e.decrypt_application(GID, &ct).unwrap().plaintext,
        b"restored"
    );
    // corrupt and newer snapshots are refused, never guessed
    assert!(OpenMlsLoader::restore(&snap[..snap.len() - 3]).is_err());
    let mut newer = snap.clone();
    newer[11] = 9;
    assert_eq!(
        OpenMlsLoader::restore(&newer).err().unwrap().0,
        "snapshot_too_new"
    );
    assert_eq!(
        OpenMlsLoader::restore(b"nope").err().unwrap().0,
        "snapshot_corrupt"
    );
}

#[test]
fn garbage_input_and_a_duplicate_group_are_refused() {
    let (mut a, mut b1, _b2) = world();
    assert_eq!(
        b1.e.process_commit(env!(b1), GID, b"junk").unwrap_err().0,
        "message_invalid"
    );
    assert_eq!(
        a.e.add_commit(env!(a), GID, &[b"junk".to_vec()])
            .unwrap_err()
            .0,
        "key_package_invalid"
    );
    assert_eq!(
        b1.e.process_welcome(env!(b1), b"junk").unwrap_err().0,
        "welcome_invalid"
    );
    // creating the same group twice is refused
    assert_eq!(
        a.e.create_group(env!(a), GID, CTX).unwrap_err().0,
        "group_exists"
    );
}
