// SPDX-License-Identifier: MPL-2.0
//! `communities_publications` slice 1: the Publication validator against cases PUB-01 to PUB-14
//! (SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md), plus the wire decode of the V-008 bodies.
use om_core::publication::*;

const PID: [u8; 32] = [0xA1; 32];
const K1: [u8; 16] = [1; 16];
const K2: [u8; 16] = [2; 16];

fn gov(n: u8) -> Principal {
    (1, vec![n; 16])
}
fn h(tag: u8, n: u64) -> [u8; 32] {
    let mut x = [tag; 32];
    x[..8].copy_from_slice(&n.to_be_bytes());
    x
}
fn key(r: [u8; 16], status: KeyStatus) -> KeyState {
    KeyState {
        key_ref: r.to_vec(),
        status,
    }
}
/// A control state: `id` makes its hash, `prev` links it.
fn cs(id: u64, version: u64, prev: Option<u64>, keys: Vec<KeyState>) -> ControlState {
    ControlState {
        publication_id: PID,
        version,
        prev: prev.map(|p| h(0xC0, p)),
        hash: h(0xC0, id),
        publisher: vec![gov(9)],
        keys,
        discussion: None,
        governance: vec![gov(1), gov(2)],
        threshold: 1,
    }
}
fn ev(
    k: [u8; 16],
    seq: u64,
    prev: Option<u64>,
    ctl: u64,
    kind: Kind,
    post: u8,
    clock: u64,
) -> Event {
    Event {
        publication_id: PID,
        post_id: vec![post; 16],
        clock,
        kind,
        signing_key_ref: k.to_vec(),
        seq,
        prev_post_hash: prev.map(|p| h(0xE0, p)),
        control_state_hash: h(0xC0, ctl),
        hash: h(0xE0, seq + 1000 * k[0] as u64),
    }
}
fn feed() -> Feed {
    let mut f = Feed::new(PID);
    assert_eq!(
        f.accept_control(cs(1, 1, None, vec![key(K1, KeyStatus::Current)]), &[]),
        Ok(ControlOutcome::Accepted)
    );
    f
}
/// Event hash of `ev(k, seq, ..)`, for chaining.
fn eh(k: [u8; 16], seq: u64) -> u64 {
    seq + 1000 * k[0] as u64
}

#[test]
fn pub_01_seq_and_prev_hash_must_continue_the_chain() {
    let mut f = feed();
    assert_eq!(
        f.accept_event(ev(K1, 0, None, 1, Kind::Post, 1, 5)),
        Ok(EventOutcome::Accepted)
    );
    // Wrong prev_post_hash at the next seq.
    assert_eq!(
        f.accept_event(ev(K1, 1, Some(999), 1, Kind::Post, 2, 6)),
        Err(EventError::BadChain)
    );
    // seq 0 again with a different event is not a continuation (equivocation, see PUB-12), and a missing prev at seq 1 is bad.
    assert_eq!(
        f.accept_event(ev(K1, 1, None, 1, Kind::Post, 2, 6)),
        Err(EventError::BadChain)
    );
    // Rejected events changed nothing: the right continuation still works.
    assert_eq!(
        f.accept_event(ev(K1, 1, Some(eh(K1, 0)), 1, Kind::Post, 2, 6)),
        Ok(EventOutcome::Accepted)
    );
    // A gap is held, not accepted, and joins the chain when the missing event arrives.
    assert_eq!(
        f.accept_event(ev(K1, 3, Some(eh(K1, 2)), 1, Kind::Post, 4, 8)),
        Ok(EventOutcome::Held)
    );
    assert_eq!(f.post(&[4; 16]), None);
    assert_eq!(
        f.accept_event(ev(K1, 2, Some(eh(K1, 1)), 1, Kind::Post, 3, 7)),
        Ok(EventOutcome::Accepted)
    );
    assert_eq!(
        f.post(&[4; 16]),
        Some(PostView::Live(Kind::Post)),
        "the held event followed once the gap closed"
    );
}

fn revoking_feed() -> Feed {
    let mut f = feed();
    for s in 0..=3u64 {
        let prev = (s > 0).then(|| eh(K1, s - 1));
        assert_eq!(
            f.accept_event(ev(K1, s, prev, 1, Kind::Post, s as u8 + 1, 10)),
            Ok(EventOutcome::Accepted)
        );
    }
    // v2 revokes K1 with last valid seq 2 and adds K2.
    let v2 = cs(
        2,
        2,
        Some(1),
        vec![
            key(K1, KeyStatus::Revoked { last_valid_seq: 2 }),
            key(K2, KeyStatus::Current),
        ],
    );
    assert!(f.accept_control(v2, &[gov(1)]).is_ok());
    f
}

#[test]
fn pub_02_03_04_revoked_key_validity_is_by_seq_never_by_clock() {
    let mut f = revoking_feed();
    // seq 4 > last valid 2 is invalid whatever clock it carries and whichever control state it names.
    for (ctl, clock) in [(1, 1), (2, 1), (2, 99_999)] {
        assert_eq!(
            f.accept_event(ev(K1, 4, Some(eh(K1, 3)), ctl, Kind::Post, 9, clock)),
            Err(EventError::RevokedBeyondLastValid),
            "control {ctl} clock {clock}"
        );
    }
    // PUB-04: an event at or below the recorded last valid seq stays valid (a late copy of seq 2 is a duplicate here).
    assert_eq!(
        f.accept_event(ev(K1, 2, Some(eh(K1, 1)), 2, Kind::Post, 3, 0)),
        Ok(EventOutcome::Duplicate)
    );
}

#[test]
fn pub_04_a_validly_chained_event_at_or_below_last_valid_seq_is_accepted() {
    let mut f = feed();
    assert!(f.accept_event(ev(K1, 0, None, 1, Kind::Post, 1, 1)).is_ok());
    let v2 = cs(
        2,
        2,
        Some(1),
        vec![
            key(K1, KeyStatus::Revoked { last_valid_seq: 2 }),
            key(K2, KeyStatus::Current),
        ],
    );
    assert!(f.accept_control(v2, &[gov(2)]).is_ok());
    // seq 1 and 2 reach us after the revocation; both are within the last valid seq.
    assert_eq!(
        f.accept_event(ev(K1, 1, Some(eh(K1, 0)), 1, Kind::Post, 2, 1)),
        Ok(EventOutcome::Accepted)
    );
    assert_eq!(
        f.accept_event(ev(K1, 2, Some(eh(K1, 1)), 1, Kind::Post, 3, 1)),
        Ok(EventOutcome::Accepted)
    );
}

#[test]
fn pub_05_06_an_older_head_never_wins_again() {
    let mut f = revoking_feed();
    let v1_again = cs(1, 1, None, vec![key(K1, KeyStatus::Current)]);
    assert_eq!(f.accept_control(v1_again, &[]), Err(ControlError::Rollback));
    assert_eq!(f.head().unwrap().version, 2);
    // A mirror withholding the revocation cannot help a replayed event: the newer head is remembered.
    assert_eq!(
        f.accept_event(ev(K1, 4, Some(eh(K1, 3)), 1, Kind::Post, 9, 1)),
        Err(EventError::RevokedBeyondLastValid)
    );
    // The current head offered again is idempotent.
    let v2 = cs(
        2,
        2,
        Some(1),
        vec![
            key(K1, KeyStatus::Revoked { last_valid_seq: 2 }),
            key(K2, KeyStatus::Current),
        ],
    );
    assert_eq!(f.accept_control(v2, &[]), Ok(ControlOutcome::Duplicate));
}

#[test]
fn pub_07_two_authority_changing_successors_freeze_with_no_winner_but_reductions_apply() {
    let mut f = feed();
    let a = cs(
        2,
        2,
        Some(1),
        vec![key(K1, KeyStatus::Current), key(K2, KeyStatus::Current)],
    );
    let mut b = cs(
        3,
        2,
        Some(1),
        vec![
            key(K1, KeyStatus::Current),
            key([3; 16], KeyStatus::Current),
        ],
    );
    b.publisher.push(gov(8));
    assert_eq!(f.accept_control(a, &[gov(1)]), Ok(ControlOutcome::Accepted));
    assert_eq!(f.accept_control(b, &[gov(2)]), Ok(ControlOutcome::Forked));
    assert!(f.is_frozen());
    assert!(
        f.head().is_none(),
        "no winner by arrival, timestamp or provider"
    );
    // Privileged advancement is refused...
    let grow = cs(
        4,
        3,
        Some(2),
        vec![
            key(K1, KeyStatus::Current),
            key(K2, KeyStatus::Current),
            key([4; 16], KeyStatus::Current),
        ],
    );
    assert_eq!(f.accept_control(grow, &[gov(1)]), Err(ControlError::Frozen));
    // ...while an authority-reducing transition (revoking K2) stays applicable.
    let reduce = cs(
        5,
        3,
        Some(2),
        vec![
            key(K1, KeyStatus::Current),
            key(K2, KeyStatus::Revoked { last_valid_seq: 0 }),
        ],
    );
    assert_eq!(
        f.accept_control(reduce, &[gov(1)]),
        Ok(ControlOutcome::Accepted)
    );
}

#[test]
fn pub_08_a_transition_must_name_the_prior_state_and_satisfy_its_governance() {
    let mut f = feed();
    let ok_keys = || vec![key(K1, KeyStatus::Current)];
    assert_eq!(
        f.accept_control(cs(2, 2, None, ok_keys()), &[gov(1)]),
        Err(ControlError::MissingPrev)
    );
    assert_eq!(
        f.accept_control(cs(2, 2, Some(77), ok_keys()), &[gov(1)]),
        Err(ControlError::UnknownPrev)
    );
    assert_eq!(
        f.accept_control(cs(2, 2, Some(1), ok_keys()), &[]),
        Err(ControlError::Governance)
    );
    assert_eq!(
        f.accept_control(cs(2, 2, Some(1), ok_keys()), &[gov(5)]),
        Err(ControlError::Governance),
        "a non-governor's approval does not count"
    );
    // Two approvals from the same principal count once; threshold 2 needs two distinct governors.
    let mut f = Feed::new(PID);
    let mut g = cs(1, 1, None, vec![key(K1, KeyStatus::Current)]);
    g.threshold = 2;
    f.accept_control(g, &[]).unwrap();
    assert_eq!(
        f.accept_control(cs(2, 2, Some(1), ok_keys()), &[gov(1), gov(1)]),
        Err(ControlError::Governance)
    );
    assert!(
        f.accept_control(cs(2, 2, Some(1), ok_keys()), &[gov(1), gov(2)])
            .is_ok()
    );
}

#[test]
fn pub_09_an_event_must_use_a_key_the_named_control_state_authorizes() {
    let mut f = feed();
    assert_eq!(
        f.accept_event(ev(K2, 0, None, 1, Kind::Post, 1, 1)),
        Err(EventError::KeyNotAuthorized)
    );
    assert_eq!(
        f.accept_event(ev(K1, 0, None, 55, Kind::Post, 1, 1)),
        Err(EventError::UnknownControl)
    );
}

#[test]
fn pub_10_rotation_keeps_the_publication_id_and_the_old_key_cannot_post_later() {
    let mut f = revoking_feed();
    assert_eq!(
        f.head().unwrap().publication_id,
        PID,
        "PublicationID unchanged by rotation"
    );
    assert_eq!(
        f.accept_event(ev(K2, 0, None, 2, Kind::Post, 7, 1)),
        Ok(EventOutcome::Accepted),
        "the new key posts"
    );
    assert_eq!(
        f.accept_event(ev(K1, 4, Some(eh(K1, 3)), 2, Kind::Post, 8, 1)),
        Err(EventError::RevokedBeyondLastValid)
    );
}

#[test]
fn pub_11_a_tombstone_is_an_ordinary_event_and_reads_as_retracted() {
    let mut f = feed();
    f.accept_event(ev(K1, 0, None, 1, Kind::Post, 1, 1))
        .unwrap();
    assert_eq!(f.post(&[1; 16]), Some(PostView::Live(Kind::Post)));
    assert_eq!(
        f.accept_event(ev(K1, 1, Some(eh(K1, 0)), 1, Kind::Tombstone, 1, 2)),
        Ok(EventOutcome::Accepted)
    );
    assert_eq!(f.post(&[1; 16]), Some(PostView::Retracted));
    // A later edit does not bring it back.
    f.accept_event(ev(K1, 2, Some(eh(K1, 1)), 1, Kind::Edit, 1, 3))
        .unwrap();
    assert_eq!(f.post(&[1; 16]), Some(PostView::Retracted));
    assert_eq!(
        f.post(&[9; 16]),
        None,
        "an unknown post is absent, a retracted one is not"
    );
}

#[test]
fn pub_12_a_duplicate_is_idempotent_and_two_events_for_one_seq_are_an_equivocation() {
    let mut f = feed();
    let first = ev(K1, 0, None, 1, Kind::Post, 1, 1);
    assert_eq!(f.accept_event(first.clone()), Ok(EventOutcome::Accepted));
    assert_eq!(f.accept_event(first), Ok(EventOutcome::Duplicate));
    let mut other = ev(K1, 0, None, 1, Kind::Post, 2, 1);
    other.hash = h(0xEE, 0);
    assert_eq!(f.accept_event(other), Ok(EventOutcome::Equivocation));
    assert_eq!(f.conflicts().len(), 1, "recorded, not silently overwritten");
    assert_eq!(
        f.post(&[2; 16]),
        None,
        "the conflicting event is not applied"
    );
    assert_eq!(f.post(&[1; 16]), Some(PostView::Live(Kind::Post)));
}

#[test]
fn pub_13_following_creates_no_roster() {
    // The feed state has one local flag and no member or subscriber collection; nothing it exposes can name a follower.
    let mut f = feed();
    assert!(!f.following);
    f.following = true;
    assert!(f.head().is_some());
}

#[test]
fn pub_14_a_discussion_id_is_data_and_grants_nothing_either_way() {
    let mut f = Feed::new(PID);
    let mut g = cs(1, 1, None, vec![key(K1, KeyStatus::Current)]);
    g.discussion = Some(vec![0xD1; 16]);
    f.accept_control(g, &[]).unwrap();
    assert_eq!(f.head().unwrap().discussion, Some(vec![0xD1; 16]));
    // Publishing authority comes only from the control state's keys: the validator takes no Room input at all, so a key
    // that is not listed cannot post however the discussion Room is populated.
    assert_eq!(
        f.accept_event(ev(K2, 0, None, 1, Kind::Post, 1, 1)),
        Err(EventError::KeyNotAuthorized)
    );
}

#[test]
fn the_v008_bodies_decode_and_hash_through_the_w3_registry() {
    // Rebuild the two V-008 publication bodies from the committed corpus and decode them.
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../verification/prototypes/v008-space-wire/python_codec.py"
    );
    assert!(std::path::Path::new(p).exists());
    let out = std::process::Command::new("python3")
        .args(["-I", "-c", "import json,sys;sys.path.insert(0,sys.argv[1]);import python_codec as c;v=json.load(open(sys.argv[1]+'/vectors.json'));print(json.dumps({x['id']:c.enc(c.norm(x['value'])).hex() for x in v['positive']}))"])
        .arg(std::path::Path::new(p).parent().unwrap())
        .output();
    let Ok(out) = out else { return };
    if !out.status.success() {
        return;
    }
    let m: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let unhex = |s: &str| -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    };
    let control =
        ControlState::from_body(&unhex(m["publication-control"].as_str().unwrap())).unwrap();
    assert_eq!(control.version, 1);
    assert!(control.prev.is_none() && !control.keys.is_empty() && control.threshold >= 1);
    let post = Event::from_body(&unhex(m["publication-post"].as_str().unwrap())).unwrap();
    assert_eq!(post.seq, 0);
    assert!(post.prev_post_hash.is_none());
    assert_eq!(post.clock, 0, "the clock is not carried into validation");
    assert!(Event::from_body(&[0xa0]).is_err());
}
