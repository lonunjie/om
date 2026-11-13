// SPDX-License-Identifier: MPL-2.0
//! Ordinary groups, slice 2 (ADR-0177): the MLS-engine boundary, driven through the TEST-ONLY fake engine.
//!
//! Three devices on two accounts (A1 for account A; B1 and B2 for account B) each hold their own `AppState` and
//! engine. These tests exercise Core's logic around an engine: the `GroupCrypto` contract, the versioned storage
//! adapter, the ADR-0163 journal (pending/committed/failed, crash replay of identical bytes), `GroupCommitBinding`
//! signing and verification, and same-parent commit ordering (ADR-0173). The fake provides no cryptography, so
//! nothing here is evidence about MLS; it is evidence about Core. Delivery of Welcomes and Commits between devices is
//! a direct call: the invite/accept/transport path is slice 3.

use om_codec::{Value, encode};
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::sig::DeviceSigner;
use om_core::groups::binding::*;
use om_core::groups::journal::*;
use om_core::groups::mls::*;
use om_core::groups::store::{GroupCryptoRec, STORE_VERSION};
use om_core::groups::*;
use om_core::state::AppState;

/// The engine under test: the TEST-ONLY fake by default; stock OpenMLS with `--features groups-openmls`
/// (which depends on the proposed ADR-0187). The scenarios are identical; fake-only tests are `cfg`-gated.
#[cfg(not(feature = "groups-openmls"))]
mod engine {
    pub use om_core::groups::fake::{
        FAKE_TAG as TAG, FakeGroupCrypto as Eng, FakeLoader as Loader,
    };
    pub fn new(id: &[u8]) -> Eng {
        Eng::new(id)
    }
}
#[cfg(feature = "groups-openmls")]
mod engine {
    pub use om_core::groups::openmls::{
        OPENMLS_TAG as TAG, OpenMlsGroupCrypto as Eng, OpenMlsLoader as Loader,
    };
    pub fn new(id: &[u8]) -> Eng {
        Eng::new(id).expect("engine")
    }
}

const ADMIN: u8 = 0xa0;
const MEMBER: u8 = 0xa1;
const NOW: u64 = 1_800_000_000_000;

fn id16(n: u8) -> Vec<u8> {
    vec![n; 16]
}
fn acct(n: u8) -> Principal {
    Principal::account(vec![n; 32])
}
fn m(e: Vec<(u64, Value)>) -> Value {
    Value::Map(e.into_iter().map(|(k, v)| (Value::Uint(k), v)).collect())
}
fn dig(h: &[u8]) -> Value {
    m(vec![(0, Value::Uint(1)), (1, Value::Bytes(h.to_vec()))])
}
fn p_val(p: &Principal) -> Value {
    m(vec![
        (0, Value::Uint(p.kind)),
        (1, Value::Bytes(p.id.clone())),
    ])
}

/// A device: its own state, engine, signer and deterministic RNG.
struct Dev {
    app: AppState,
    eng: engine::Eng,
    signer: DeviceSigner,
    rng: InsecureTestRng,
    id: Vec<u8>,
}

impl Dev {
    fn new(tag: u8) -> Self {
        let mut rng = InsecureTestRng::from_label(&[b'g', tag]);
        Dev {
            app: AppState::default(),
            eng: engine::new(&id16(tag)),
            signer: DeviceSigner::generate(&mut rng),
            rng,
            id: id16(tag),
        }
    }
    fn pk(&self) -> Vec<u8> {
        self.signer.public()
    }
    fn kp(&mut self) -> Vec<u8> {
        let mut env = CryptoEnv {
            rng: &mut self.rng,
            now_ms: NOW,
        };
        self.eng.key_package(&mut env).unwrap().bytes
    }
}

/// Run `f` with an env drawn from the device's own RNG.
macro_rules! env {
    ($d:expr) => {
        &mut CryptoEnv {
            rng: &mut $d.rng,
            now_ms: NOW,
        }
    };
}

const CONV: [u8; 32] = [9; 32];
const GID: [u8; 24] = [0x6a; 24];

/// A creates the group on device A1 (genesis state, MLS group, bound ids).
fn create(a1: &mut Dev) -> GroupState {
    let g = genesis(&GenesisParams {
        conversation_id: CONV.to_vec(),
        creator: acct(1),
        creator_incarnation: id16(0x11),
        creator_device: a1.id.clone(),
        admin_role_id: id16(ADMIN),
        member_role_id: id16(MEMBER),
        name: "Book club".into(),
    })
    .unwrap();
    a1.app.group_install_genesis(&g).unwrap();
    a1.app.group_bind_mls_id(&CONV, &GID).unwrap();
    a1.eng.create_group(env!(a1), &GID, &CONV).unwrap();
    a1.app.group_crypto_save(&a1.eng);
    g
}

fn intent_body(parent: &GroupState, n: u8, op_id: u64, args: Value) -> Vec<u8> {
    let reducing = matches!(op_id, 2 | 3 | 5 | 11 | 16);
    encode(&m(vec![
        (0, Value::Uint(29)),
        (1, Value::Uint(1)),
        (2, Value::Bytes(parent.conversation_id.clone())),
        (3, dig(&parent.hash())),
        (4, Value::Uint(parent.state_version)),
        (5, Value::Bytes(id16(n))),
        (6, Value::Uint(op_id)),
        (7, Value::Bool(reducing)),
        (8, args),
        (9, m(vec![(0, Value::Uint(1))])),
    ]))
}

fn admit_body(parent: &GroupState, n: u8, who: u8, inc: u8, roles: &[u8]) -> Vec<u8> {
    let mut e = vec![
        (0, p_val(&acct(who))),
        (1, Value::Bytes(id16(inc))),
        (3, Value::Uint(1)),
    ];
    if !roles.is_empty() {
        e.push((
            2,
            Value::Array(roles.iter().map(|r| Value::Bytes(id16(*r))).collect()),
        ));
    }
    intent_body(parent, n, op::MEMBER_ADMIT, m(e))
}

fn remove_body(parent: &GroupState, n: u8, who: u8, inc: u8) -> Vec<u8> {
    let a = m(vec![(0, p_val(&acct(who))), (1, Value::Bytes(id16(inc)))]);
    intent_body(parent, n, op::MEMBER_REMOVE, a)
}

fn head(d: &Dev) -> GroupState {
    d.app.group(&CONV).unwrap().unwrap()
}

/// Give `joiner` the head state and join it from the Welcome (slice 3 will carry both over the 1:1).
fn join(joiner: &mut Dev, welcome: &[u8], state: &GroupState) {
    let (gid, info) = joiner.eng.process_welcome(env!(joiner), welcome).unwrap();
    assert_eq!(gid, GID);
    assert_eq!(info.epoch, state.mls_epoch_ref);
    joiner
        .app
        .groups
        .insert(om_core::crypto::hex(&CONV), GroupRec::of(state));
    joiner.app.group_bind_mls_id(&CONV, &GID).unwrap();
    joiner.app.group_crypto_save(&joiner.eng);
}

fn prep(
    d: &mut Dev,
    body: &[u8],
    actor: u8,
    plan: &CommitPlan,
    kind: u64,
) -> (String, Publication) {
    let e = d.app.group_journal_begin(body, &[actor; 32]).unwrap();
    let p = d
        .app
        .group_journal_prepare(&e.intent_id, plan, kind, &mut d.eng, env!(d), &d.signer)
        .unwrap();
    (e.intent_id, p)
}

/// A creates, then admits account B (admin, devices B1 and B2); all three converge at epoch 1.
fn world() -> (Dev, Dev, Dev) {
    let (mut a1, mut b1, mut b2) = (Dev::new(0xa1), Dev::new(0xb1), Dev::new(0xb2));
    let g = create(&mut a1);
    let kps = vec![b1.kp(), b2.kp()];
    let (id, p) = prep(
        &mut a1,
        &admit_body(&g, 1, 2, 0x22, &[ADMIN]),
        1,
        &CommitPlan::Add(kps),
        DEFAULT_BINDING_KIND,
    );
    a1.app.group_journal_published(&id).unwrap();
    let s2 = a1.app.group_journal_accept(&id, &mut a1.eng).unwrap();
    let w = p.welcome.unwrap();
    join(&mut b1, &w, &s2);
    join(&mut b2, &w, &s2);
    (a1, b1, b2)
}

#[test]
fn create_and_add_member_converges_three_devices_on_one_epoch() {
    let (a1, b1, b2) = world();
    for d in [&a1, &b1, &b2] {
        assert_eq!(d.eng.epoch(&GID).unwrap(), 1);
        assert_eq!(head(d).mls_epoch_ref, 1);
        assert_eq!(head(d).hash(), head(&a1).hash());
    }
    let ex = |d: &Dev| d.eng.export_secret(&GID, "om-test", b"ctx", 32).unwrap();
    assert_eq!(ex(&a1), ex(&b1));
    assert_eq!(ex(&a1), ex(&b2));
    assert_ne!(
        ex(&a1),
        a1.eng.export_secret(&GID, "other", b"ctx", 32).unwrap()
    );
    assert_eq!(a1.eng.members(&GID).unwrap().len(), 3);
    // the entry is committed and the group sendable again
    let e = a1.app.group_journal.values().next().unwrap();
    assert_eq!(e.state, JournalState::Accepted);
    assert_eq!(e.state.outcome(), Outcome::Committed);
    assert!(a1.app.group_may_send(&CONV));
}

#[test]
fn no_message_under_a_tentative_branch() {
    let (mut a1, _b1, _b2) = world();
    let g = head(&a1);
    assert!(a1.app.group_may_send(&CONV));
    let (id, _) = prep(
        &mut a1,
        &remove_body(&g, 2, 2, 0x22),
        1,
        &CommitPlan::Remove(vec![id16(0xb1), id16(0xb2)]),
        DEFAULT_BINDING_KIND,
    );
    assert!(!a1.app.group_may_send(&CONV), "prepared candidate");
    a1.app.group_journal_published(&id).unwrap();
    assert!(!a1.app.group_may_send(&CONV), "published, unconfirmed");
    // a relay ack changes nothing: the head is still the parent
    assert_eq!(head(&a1).mls_epoch_ref, 1);
    a1.app.group_journal_accept(&id, &mut a1.eng).unwrap();
    assert!(a1.app.group_may_send(&CONV));
}

#[test]
fn a_second_open_intent_for_the_group_is_refused() {
    let (mut a1, _b1, _b2) = world();
    let g = head(&a1);
    a1.app
        .group_journal_begin(&remove_body(&g, 2, 2, 0x22), &[1; 32])
        .unwrap();
    let e = a1
        .app
        .group_journal_begin(&remove_body(&g, 3, 2, 0x22), &[1; 32])
        .unwrap_err();
    assert_eq!(e.0, "journal_busy");
    // the same intent again is idempotent, a different body under its id is not
    assert!(
        a1.app
            .group_journal_begin(&remove_body(&g, 2, 2, 0x22), &[1; 32])
            .is_ok()
    );
    assert_eq!(
        a1.app
            .group_journal_begin(&admit_body(&g, 2, 5, 0x55, &[]), &[1; 32])
            .unwrap_err()
            .0,
        "intent_id_reused"
    );
    // an intent that would not apply is never made durable
    let bad = a1
        .app
        .group_journal_begin(&remove_body(&g, 9, 2, 0x99), &[1; 32]);
    assert!(bad.is_err());
    assert_eq!(a1.app.group_journal.len(), 2);
}

/// Simulate a crash: the app state goes through JSON, and the engine is rebuilt from the stored record.
fn reboot(d: &mut Dev) {
    let json = serde_json::to_string(&d.app).unwrap();
    d.app = serde_json::from_str(&json).unwrap();
    d.eng = d
        .app
        .group_crypto_open::<engine::Loader>()
        .unwrap()
        .unwrap();
}

#[test]
fn crash_after_prepare_replays_identical_bytes() {
    let (mut a1, _b1, _b2) = world();
    let g = head(&a1);
    let (id, first) = prep(
        &mut a1,
        &remove_body(&g, 2, 2, 0x22),
        1,
        &CommitPlan::Remove(vec![id16(0xb1), id16(0xb2)]),
        DEFAULT_BINDING_KIND,
    );
    // crash before publication, and again after an unconfirmed publication
    reboot(&mut a1);
    let r = a1.app.group_journal_recover(&mut a1.eng).unwrap();
    assert_eq!(r, vec![Recovery::Retransmit(id.clone())]);
    let again = a1.app.group_journal_replay(&id, &a1.eng).unwrap();
    assert_eq!(again, first, "identical commit, welcome and signed binding");
    a1.app.group_journal_published(&id).unwrap();
    reboot(&mut a1);
    a1.app.group_journal_published(&id).unwrap();
    let e = &a1.app.group_journal[&id];
    assert_eq!(e.state, JournalState::PublishedUnconfirmed);
    assert_eq!(e.publish_attempts, 2);
    assert_eq!(a1.app.group_journal_replay(&id, &a1.eng).unwrap(), first);
    // acceptance after the restart completes with the same candidate
    let s = a1.app.group_journal_accept(&id, &mut a1.eng).unwrap();
    assert_eq!(s.mls_epoch_ref, 2);
    assert_eq!(a1.eng.epoch(&GID).unwrap(), 2);
    // the accepted candidate is idempotent: a second accept is refused, nothing repeats
    assert_eq!(
        a1.app.group_journal_accept(&id, &mut a1.eng).unwrap_err().0,
        "bad_journal_state"
    );
}

#[test]
fn crash_after_intent_only_asks_for_a_fresh_prepare_and_lost_prepared_state_fails_closed() {
    let (mut a1, _b1, _b2) = world();
    let g = head(&a1);
    let e = a1
        .app
        .group_journal_begin(&remove_body(&g, 2, 2, 0x22), &[1; 32])
        .unwrap();
    reboot(&mut a1);
    assert_eq!(
        a1.app.group_journal_recover(&mut a1.eng).unwrap(),
        vec![Recovery::Prepare(e.intent_id.clone())]
    );
    // Prepare, then lose the engine's prepared state (restore the engine from before the prepare).
    let old_engine = <engine::Loader as GroupCryptoLoader>::restore(&a1.eng.snapshot()).unwrap();
    a1.app
        .group_journal_prepare(
            &e.intent_id,
            &CommitPlan::Remove(vec![id16(0xb1), id16(0xb2)]),
            DEFAULT_BINDING_KIND,
            &mut a1.eng,
            env!(a1),
            &a1.signer,
        )
        .unwrap();
    a1.eng = old_engine;
    let r = a1.app.group_journal_recover(&mut a1.eng).unwrap();
    assert_eq!(r, vec![Recovery::Abandoned(e.intent_id.clone())]);
    let en = &a1.app.group_journal[&e.intent_id];
    assert_eq!(en.state, JournalState::Abandoned);
    assert_eq!(en.state.outcome(), Outcome::Failed);
    assert_eq!(en.reason.as_deref(), Some("prepared_state_lost"));
    assert!(a1.app.group_may_send(&CONV));
    // nothing was rebuilt: a replay is refused
    assert!(a1.app.group_journal_replay(&e.intent_id, &a1.eng).is_err());
}

#[test]
fn a_losing_candidate_is_superseded_when_the_winner_arrives() {
    let (mut a1, mut b1, _b2) = world();
    let g = head(&b1);
    // B1 (an admin) prepares an admission of account C on the epoch-1 parent ...
    let kp_c = Dev::new(0xc1).kp();
    let (id, _) = prep(
        &mut b1,
        &admit_body(&g, 3, 3, 0x33, &[]),
        2,
        &CommitPlan::Add(vec![kp_c]),
        DEFAULT_BINDING_KIND,
    );
    // ... but meanwhile A1's commit (remove B) is canonical here, so B1's head moves.
    let ga = head(&a1);
    let (aid, ap) = prep(
        &mut a1,
        &remove_body(&ga, 4, 2, 0x22),
        1,
        &CommitPlan::Remove(vec![id16(0xb1), id16(0xb2)]),
        DEFAULT_BINDING_KIND,
    );
    a1.app.group_journal_accept(&aid, &mut a1.eng).unwrap();
    // B1 learns of the new head by receiving the commit: its own candidate loses the race and is superseded.
    let ib = remove_body(&ga, 4, 2, 0x22);
    let out = b1
        .app
        .group_receive_commit(
            &ap.signed_binding,
            &a1.pk(),
            &ap.commit,
            &ib,
            &[1; 32],
            &mut b1.eng,
            env!(b1),
        )
        .unwrap();
    assert_eq!(
        out,
        ReceiveOutcome::Applied {
            epoch: 2,
            still_member: false,
            superseded: Some(id.clone())
        }
    );
    let e = &b1.app.group_journal[&id];
    assert_eq!(e.state, JournalState::Superseded);
    assert_eq!(e.reason.as_deref(), Some("lost_same_parent"));
}

#[test]
fn same_parent_race_has_one_canonical_winner_on_every_device() {
    // A1 (removal of B, a reduction) and B1 (admission of C, not a reduction) both build on the epoch-1 parent.
    let (mut a1, mut b1, mut b2) = world();
    let g = head(&a1);
    let body_a = remove_body(&g, 5, 2, 0x22);
    let body_b = admit_body(&g, 6, 3, 0x33, &[]);
    let kp_c = Dev::new(0xc1).kp();
    let (ida, pa) = prep(
        &mut a1,
        &body_a,
        1,
        &CommitPlan::Remove(vec![id16(0xb1), id16(0xb2)]),
        KIND_COORDINATOR_SEQUENCED,
    );
    let (idb, pb) = prep(
        &mut b1,
        &body_b,
        2,
        &CommitPlan::Add(vec![kp_c]),
        KIND_COORDINATOR_SEQUENCED,
    );
    a1.app.group_journal_published(&ida).unwrap();
    b1.app.group_journal_published(&idb).unwrap();

    // The ADR-0173 key: a reduction sorts before a non-reduction.
    let ka = verify_binding(&pa.signed_binding, &a1.pk()).unwrap();
    let kb = verify_binding(&pb.signed_binding, &b1.pk()).unwrap();
    assert!(ka.binding.ordering_key(&ka.body) < kb.binding.ordering_key(&kb.body));

    // A1 sees B1's commit: it keeps its own (Loses), and its candidate is untouched.
    let out = a1
        .app
        .group_receive_commit(
            &pb.signed_binding,
            &b1.pk(),
            &pb.commit,
            &body_b,
            &[2; 32],
            &mut a1.eng,
            env!(a1),
        )
        .unwrap();
    assert_eq!(out, ReceiveOutcome::Loses);
    assert_eq!(
        a1.app.group_journal[&ida].state,
        JournalState::PublishedUnconfirmed
    );
    // Its own commit coming back is recognised as its own candidate, then accepted.
    assert_eq!(
        a1.app
            .group_receive_commit(
                &pa.signed_binding,
                &a1.pk(),
                &pa.commit,
                &body_a,
                &[1; 32],
                &mut a1.eng,
                env!(a1)
            )
            .unwrap(),
        ReceiveOutcome::OwnCandidate(ida.clone())
    );
    a1.app.group_journal_accept(&ida, &mut a1.eng).unwrap();

    // B2 (no candidate) takes the winner; the loser's commit is then stale and refused.
    let out = b2
        .app
        .group_receive_commit(
            &pa.signed_binding,
            &a1.pk(),
            &pa.commit,
            &body_a,
            &[1; 32],
            &mut b2.eng,
            env!(b2),
        )
        .unwrap();
    assert!(matches!(
        out,
        ReceiveOutcome::Applied {
            epoch: 2,
            still_member: false,
            superseded: None
        }
    ));
    assert_eq!(
        b2.app
            .group_receive_commit(
                &pb.signed_binding,
                &b1.pk(),
                &pb.commit,
                &body_b,
                &[2; 32],
                &mut b2.eng,
                env!(b2)
            )
            .unwrap_err()
            .0,
        "stale_parent"
    );
    // B1 supersedes its own candidate and lands on the same head as A1.
    let out = b1
        .app
        .group_receive_commit(
            &pa.signed_binding,
            &a1.pk(),
            &pa.commit,
            &body_a,
            &[1; 32],
            &mut b1.eng,
            env!(b1),
        )
        .unwrap();
    assert!(matches!(
        out,
        ReceiveOutcome::Applied {
            superseded: Some(_),
            ..
        }
    ));
    assert_eq!(head(&b1).hash(), head(&a1).hash());
    assert_eq!(head(&b2).hash(), head(&a1).hash());
    assert_eq!(b1.app.group_journal[&idb].state.outcome(), Outcome::Failed);
}

#[test]
fn a_re_observed_accepted_commit_is_idempotent() {
    let (mut a1, mut b1, mut b2) = world();
    let g = head(&a1);
    let body = remove_body(&g, 7, 2, 0x22);
    let (id, p) = prep(
        &mut a1,
        &body,
        1,
        &CommitPlan::Remove(vec![id16(0xb1), id16(0xb2)]),
        KIND_COORDINATOR_SEQUENCED,
    );
    a1.app.group_journal_accept(&id, &mut a1.eng).unwrap();
    let recv = |d: &mut Dev| {
        d.app.group_receive_commit(
            &p.signed_binding,
            &a1.pk(),
            &p.commit,
            &body,
            &[1; 32],
            &mut d.eng,
            env!(d),
        )
    };
    assert!(matches!(
        recv(&mut b1).unwrap(),
        ReceiveOutcome::Applied { .. }
    ));
    let before = serde_json::to_string(&b1.app).unwrap();
    assert_eq!(recv(&mut b1).unwrap(), ReceiveOutcome::Duplicate);
    assert_eq!(
        serde_json::to_string(&b1.app).unwrap(),
        before,
        "nothing repeated"
    );
    assert!(matches!(
        recv(&mut b2).unwrap(),
        ReceiveOutcome::Applied { .. }
    ));
}

#[test]
fn binding_signature_is_by_the_committer_device_and_rejects_tampering() {
    let (mut a1, b1, _b2) = world();
    let g = head(&a1);
    let (_id, p) = prep(
        &mut a1,
        &remove_body(&g, 8, 2, 0x22),
        1,
        &CommitPlan::Remove(vec![id16(0xb1)]),
        KIND_COORDINATOR_SEQUENCED,
    );
    let v = verify_binding(&p.signed_binding, &a1.pk()).unwrap();
    assert_eq!(v.binding.committer_device, a1.id);
    assert_eq!(v.binding.mls_commit_hash, mls_commit_hash(&p.commit));
    assert_eq!(v.binding.previous_mls_epoch, 1);
    assert_eq!(v.binding.resulting_mls_epoch, 2);
    assert_eq!(v.binding.binding_kind, KIND_COORDINATOR_SEQUENCED);
    // another device's key
    assert_eq!(
        verify_binding(&p.signed_binding, &b1.pk()).unwrap_err().0,
        "bad_signature"
    );
    // a flipped byte in the body
    let mut t = p.signed_binding.clone();
    let pos = t
        .windows(v.body.len())
        .position(|w| w == v.body.as_slice())
        .unwrap()
        + v.body.len() / 2;
    t[pos] ^= 1;
    assert!(verify_binding(&t, &a1.pk()).is_err());
    // a signature by B1 over a body that names A1 as committer
    let rnd = [7u8; 32];
    let forged = sign_binding(&b1.signer, &b1.id, &v.body, rnd);
    assert_eq!(
        verify_binding(&forged, &b1.pk()).unwrap_err().0,
        "signer_not_committer"
    );
    // the binding names the exact Commit by hash
    assert_ne!(mls_commit_hash(&p.commit), mls_commit_hash(b"other"));
}

#[test]
fn a_binding_that_breaks_the_adr_0176_rules_is_never_built() {
    let base = BindingParams {
        conversation_id: CONV.to_vec(),
        binding_kind: KIND_COORDINATOR_SEQUENCED,
        reducing: false,
        previous_state_hash: [1; 32],
        previous_state_version: 2,
        previous_mls_epoch: 1,
        resulting_mls_epoch: 2,
        intent_hash: Some([3; 32]),
        mls_commit_hash: [4; 32],
        resulting_state_hash: [5; 32],
        committer_device: id16(0xa1),
        readded_devices: vec![],
    };
    assert!(binding_body(&base).is_ok());
    let mut x = base.clone();
    x.resulting_mls_epoch = 5;
    assert_eq!(binding_body(&x).unwrap_err().0, "epoch_step");
    let mut x = base.clone();
    x.binding_kind = KIND_DIRECT_REDUCTION;
    assert_eq!(binding_body(&x).unwrap_err().0, "direct_not_reducing");
    let mut x = base.clone();
    x.intent_hash = None;
    assert_eq!(binding_body(&x).unwrap_err().0, "intent_hash_missing");
    let mut x = base;
    x.resulting_state_hash = x.previous_state_hash;
    assert_eq!(binding_body(&x).unwrap_err().0, "state_unchanged");
}

// Fake-only: it pins reproducible randomness and Core's clock, which the stock OpenMLS engine cannot take per call
// (ADR-0187 decision 3); tests/groups_openmls.rs covers the real engine's equivalents.
#[cfg(not(feature = "groups-openmls"))]
#[test]
fn the_engine_takes_its_randomness_and_clock_from_core() {
    // identical RNG streams give identical KeyPackages; an expired KeyPackage is refused by Core's clock
    let mut x = Dev::new(0xc1);
    let mut y = Dev::new(0xc1);
    assert_eq!(x.kp(), y.kp());
    let old = x.kp();
    let mut a1 = Dev::new(0xa1);
    create(&mut a1);
    let late = NOW + 4 * 24 * 3600 * 1000;
    let e = a1
        .eng
        .add_commit(
            &mut CryptoEnv {
                rng: &mut a1.rng,
                now_ms: late,
            },
            &GID,
            &[old],
        )
        .unwrap_err();
    assert_eq!(e.0, "key_package_expired");
    // a Welcome is for the KeyPackage that was released, once
    let mut c = Dev::new(0xc2);
    let kp = c.kp();
    let p = a1.eng.add_commit(env!(a1), &GID, &[kp]).unwrap();
    a1.eng.merge_pending(&GID).unwrap();
    let w = p.welcome.unwrap();
    c.eng.process_welcome(env!(c), &w).unwrap();
    // the KeyPackage is single use
    assert_eq!(
        c.eng.process_welcome(env!(c), &w).unwrap_err().0,
        "no_matching_key_package"
    );
}

// ------------------------------------------------------------------ storage adapter

#[test]
fn storage_record_is_versioned_tagged_and_checked() {
    let (a1, _b1, _b2) = world();
    let rec = a1.app.group_crypto.clone().unwrap();
    assert_eq!(rec.version, STORE_VERSION);
    assert_eq!(rec.engine, engine::TAG);
    let json = serde_json::to_value(&rec).unwrap();
    assert!(json["snapshotHash"].is_string());
    assert!(rec.open::<engine::Loader>().is_ok());

    // a newer OM record version is refused, not guessed
    let mut newer = rec.clone();
    newer.version = STORE_VERSION + 1;
    assert_eq!(
        newer.open::<engine::Loader>().err().unwrap().0,
        "store_too_new"
    );
    // another engine's record is refused
    let mut other = rec.clone();
    other.engine = "other-engine-v1".into();
    assert_eq!(
        other.open::<engine::Loader>().err().unwrap().0,
        "engine_mismatch"
    );
    // a corrupted blob is noticed before an engine sees it
    let mut bad = rec.clone();
    bad.snapshot.replace_range(0..2, "00");
    assert_eq!(
        bad.open::<engine::Loader>().err().unwrap().0,
        "record_corrupt"
    );
    // an extra field written by a later version is ignored (additive), a missing optional field is not needed
    let mut j = json.clone();
    j["futureField"] = serde_json::json!({"x": 1});
    let back: GroupCryptoRec = serde_json::from_value(j).unwrap();
    assert_eq!(back, rec);
}

#[test]
fn records_written_before_this_slice_still_load() {
    // an AppState JSON from slice 1 has none of the slice 2 fields
    let (a1, _b1, _b2) = world();
    let mut j = serde_json::to_value(&a1.app).unwrap();
    let o = j.as_object_mut().unwrap();
    for k in [
        "groupCrypto",
        "groupMlsIds",
        "groupJournal",
        "groupAccepted",
    ] {
        assert!(o.remove(k).is_some(), "{k} is written");
    }
    let old: AppState = serde_json::from_value(j).unwrap();
    assert!(old.group_crypto.is_none() && old.group_journal.is_empty());
    assert!(old.group(&CONV).unwrap().is_some(), "slice 1 head survives");
    assert!(old.group_crypto_open::<engine::Loader>().unwrap().is_none());
    // and a default state round-trips
    let d: AppState =
        serde_json::from_str(&serde_json::to_string(&AppState::default()).unwrap()).unwrap();
    assert_eq!(d, AppState::default());
}

#[test]
fn journal_keeps_a_bounded_tail_of_finished_entries() {
    let (mut a1, _b1, _b2) = world();
    for n in 0..40u8 {
        let g = head(&a1);
        let e = a1
            .app
            .group_journal_begin(&remove_body(&g, 100 + n, 2, 0x99), &[1; 32]);
        // an intent for an absent incarnation never becomes durable
        assert!(e.is_err());
    }
    // abandon-and-retry cycles on a valid intent
    let g = head(&a1);
    for n in 0..40u8 {
        let e = a1
            .app
            .group_journal_begin(&admit_body(&g, 150 + n, 3, 0x33, &[]), &[1; 32])
            .unwrap();
        a1.app
            .group_journal_abandon(&e.intent_id, &mut a1.eng, "test")
            .unwrap();
    }
    let terminal = a1
        .app
        .group_journal
        .values()
        .filter(|e| e.state.is_terminal())
        .count();
    assert!(terminal <= 16, "{terminal}");
}

#[test]
fn restart_with_a_moved_parent_supersedes_and_never_regenerates() {
    let (mut a1, _b1, _b2) = world();
    let g = head(&a1);
    let (id, _) = prep(
        &mut a1,
        &remove_body(&g, 2, 2, 0x22),
        1,
        &CommitPlan::Remove(vec![id16(0xb1), id16(0xb2)]),
        KIND_COORDINATOR_SEQUENCED,
    );
    // the canonical head moves while this device was down (a state it learned on reconnect)
    let kp = Dev::new(0xc1).kp();
    let moved = apply_other_head(&g, &kp);
    a1.app
        .groups
        .insert(om_core::crypto::hex(&CONV), GroupRec::of(&moved));
    reboot(&mut a1);
    assert_eq!(
        a1.app.group_journal_recover(&mut a1.eng).unwrap(),
        vec![Recovery::Superseded(id.clone())]
    );
    assert_eq!(a1.app.group_journal[&id].state, JournalState::Superseded);
    assert_eq!(
        a1.eng.pending(&GID).unwrap(),
        None,
        "tentative state dropped"
    );
    assert_eq!(a1.eng.epoch(&GID).unwrap(), 1);
    assert!(a1.app.group_may_send(&CONV));
}

/// A different canonical successor of `g` (admission of account C), as a device that missed it would later learn.
fn apply_other_head(g: &GroupState, _kp: &[u8]) -> GroupState {
    let i = Intent::decode(&admit_body(g, 40, 3, 0x33, &[])).unwrap();
    apply(g, &i, &acct(1), g.mls_epoch_ref + 1).unwrap()
}
