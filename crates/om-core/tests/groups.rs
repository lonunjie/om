// SPDX-License-Identifier: MPL-2.0
//! Ordinary groups, slice 1 (ADR-0177): the typed `GroupControlState` model. Replays the V-004 semantic corpus
//! (`verification/vectors/v004-group-wire-semantic.json.gz`, ADR-0176 section 3) through the product validators and
//! checks the reducer, the hash chain, persistence and a property over random intent sequences.

use om_codec::cbor::from_diag;
use om_codec::{Value, encode};
use om_core::groups::*;
use om_core::state::AppState;
use proptest::prelude::*;
use serde_json::Value as J;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn corpus() -> Vec<J> {
    let out = std::process::Command::new("gzip")
        .arg("-dc")
        .arg(root().join("verification/vectors/v004-group-wire-semantic.json.gz"))
        .output()
        .expect("gzip");
    assert!(out.status.success());
    let v: J = serde_json::from_slice(&out.stdout).unwrap();
    v["cases"].as_array().unwrap().clone()
}

fn reg() -> om_codec::Registry {
    om_codec::Registry::load()
}

#[test]
fn semantic_corpus_agrees_with_the_product_validators() {
    let mut checked = 0;
    for c in corpus() {
        let kind = c["kind"].as_str().unwrap();
        if kind == "scenario" {
            continue; // bundle admission needs Room and ACS context: a later slice
        }
        let id = c["id"].as_str().unwrap();
        let v = from_diag(&c["value"]);
        let body = encode(&v);
        let verdict: Result<(), GroupError> = match kind {
            "state" => GroupState::decode(&body).map(|_| ()),
            "intent" => Intent::decode(&body).map(|_| ()),
            "binding" => Binding::decode(&body).map(|_| ()),
            "bundle" => reg()
                .check_body("GroupHistoryBundle", &body)
                .map_err(GroupError)
                .and_then(|_| validate_bundle_structure(&v)),
            k => panic!("unknown kind {k}"),
        };
        match (c["expect"].as_str().unwrap(), verdict) {
            ("accept", Ok(())) => {}
            ("reject", Err(e)) => assert_eq!(Some(e.0), c["reason"].as_str(), "{id}"),
            (want, got) => panic!("{id}: expected {want}, got {got:?}"),
        }
        checked += 1;
    }
    assert_eq!(checked, 86);
}

#[test]
fn accepted_states_round_trip_and_reencode_canonically() {
    for c in corpus() {
        if c["kind"] != "state" || c["expect"] != "accept" {
            continue;
        }
        let s = GroupState::decode(&encode(&from_diag(&c["value"]))).unwrap();
        // the sets are written sorted, so a re-encode may reorder an unsorted vector but must decode to the same state
        let again = GroupState::decode(&s.encode()).unwrap();
        assert_eq!(s.hash(), again.hash());
        assert_eq!(again.encode(), s.encode());
    }
}

// ------------------------------------------------------------------ helpers

fn id16(n: u8) -> Vec<u8> {
    vec![n; 16]
}
fn acct(n: u8) -> Principal {
    Principal::account(vec![n; 32])
}
fn p_val(p: &Principal) -> Value {
    m(vec![
        (0, Value::Uint(p.kind)),
        (1, Value::Bytes(p.id.clone())),
    ])
}
fn m(e: Vec<(u64, Value)>) -> Value {
    Value::Map(e.into_iter().map(|(k, v)| (Value::Uint(k), v)).collect())
}
fn dig(h: &[u8]) -> Value {
    m(vec![(0, Value::Uint(1)), (1, Value::Bytes(h.to_vec()))])
}
fn bytes(b: Vec<u8>) -> Value {
    Value::Bytes(b)
}

const CREATOR: u8 = 1;
const ADMIN: u8 = 0xa0;
const MEMBER: u8 = 0xa1;

fn world() -> GroupState {
    genesis(&GenesisParams {
        conversation_id: vec![9; 32],
        creator: acct(CREATOR),
        creator_incarnation: id16(0x11),
        creator_device: id16(0xd1),
        admin_role_id: id16(ADMIN),
        member_role_id: id16(MEMBER),
        name: "Book club".into(),
        protection: 1,
    })
    .unwrap()
}

/// Build, encode and decode an intent through the product decoder (so the codec and section 3 run too).
fn intent(parent: &GroupState, op: u64, kind: u64, args: Value) -> Result<Intent, GroupError> {
    let reducing = matches!(op, 2 | 3 | 5 | 11 | 16);
    let body = encode(&m(vec![
        (0, Value::Uint(29)),
        (1, Value::Uint(1)),
        (2, bytes(parent.conversation_id.clone())),
        (3, dig(&parent.hash())),
        (4, Value::Uint(parent.state_version)),
        (5, bytes(id16(0x77))),
        (6, Value::Uint(op)),
        (7, Value::Bool(reducing)),
        (8, args),
        (9, m(vec![(0, Value::Uint(kind))])),
    ]));
    Intent::decode(&body)
}

fn admit_args(who: &Principal, inc: u8, roles: &[u8]) -> Value {
    let mut e = vec![(0, p_val(who)), (1, bytes(id16(inc))), (3, Value::Uint(1))];
    if !roles.is_empty() {
        e.push((
            2,
            Value::Array(roles.iter().map(|r| bytes(id16(*r))).collect()),
        ));
    }
    m(e)
}

fn admit(
    s: &GroupState,
    actor: &Principal,
    who: u8,
    inc: u8,
    roles: &[u8],
) -> Result<GroupState, GroupError> {
    let i = intent(s, op::MEMBER_ADMIT, 1, admit_args(&acct(who), inc, roles))?;
    apply(s, &i, actor, s.mls_epoch_ref + 1)
}

// ------------------------------------------------------------------ genesis and reducer

#[test]
fn genesis_is_valid_and_encodes_for_the_codec() {
    let g = world();
    assert_eq!(g.state_version, 1);
    assert!(g.previous_state_hash.is_none());
    assert_eq!(g.mls_epoch_ref, 0);
    assert_eq!(g.governance.approval_threshold, 1);
    assert_eq!(GroupState::decode(&g.encode()).unwrap(), g);
    let admin = g.capabilities_of(&acct(CREATOR));
    for c in [
        "room.post",
        "room.invite",
        "room.manage_governance",
        "room.manage_roles",
    ] {
        assert!(admin.contains(c), "{c}");
    }
    assert!(
        !admin.contains("room.start_call"),
        "a W2 capability is not a W1 grant"
    );
}

#[test]
fn admit_chains_the_hash_and_the_new_member_gets_member_capabilities() {
    let g = world();
    let s2 = admit(&g, &acct(CREATOR), 2, 0x22, &[MEMBER]).unwrap();
    assert_eq!(s2.state_version, 2);
    assert_eq!(s2.previous_state_hash.as_deref(), Some(g.hash().as_slice()));
    assert_eq!(s2.mls_epoch_ref, 1);
    assert!(s2.capabilities_of(&acct(2)).contains("room.post"));
    // Recommended (ADR-0189 section 3): anyone in the group can add people (invite), only admins admit
    assert!(s2.capabilities_of(&acct(2)).contains("room.invite"));
    assert!(
        !s2.capabilities_of(&acct(2))
            .contains("room.manage_admission")
    );
    // a Member cannot admit: room.manage_admission is not in the Member role
    assert_eq!(
        admit(&s2, &acct(2), 3, 0x33, &[MEMBER]).unwrap_err().0,
        "not_authorized"
    );
    // the same person twice, and an unknown role
    assert_eq!(
        admit(&s2, &acct(CREATOR), 2, 0x44, &[]).unwrap_err().0,
        "already_member"
    );
    assert_eq!(
        admit(&s2, &acct(CREATOR), 3, 0x33, &[0xee]).unwrap_err().0,
        "role_unknown"
    );
}

#[test]
fn an_intent_for_another_parent_or_epoch_is_refused() {
    let g = world();
    let i = intent(
        &g,
        op::PROFILE_SET,
        1,
        m(vec![(0, m(vec![(0, Value::Text("New".into()))]))]),
    )
    .unwrap();
    let s2 = apply(&g, &i, &acct(CREATOR), 1).unwrap();
    assert_eq!(s2.profile.name, "New");
    assert_eq!(
        apply(&s2, &i, &acct(CREATOR), 2).unwrap_err().0,
        "stale_parent"
    );
    assert_eq!(
        apply(&g, &i, &acct(CREATOR), 5).unwrap_err().0,
        "epoch_step"
    );
}

#[test]
fn remove_targets_an_exact_incarnation_and_takes_the_members_invites_and_seat() {
    let g = world();
    let s2 = admit(&g, &acct(CREATOR), 2, 0x22, &[MEMBER]).unwrap();
    let rm = |s: &GroupState, inc: u8| {
        let a = m(vec![(0, p_val(&acct(2))), (1, bytes(id16(inc)))]);
        intent(s, op::MEMBER_REMOVE, 1, a)
    };
    // ABA: a stale incarnation removes nothing
    assert_eq!(
        apply(&s2, &rm(&s2, 0x99).unwrap(), &acct(CREATOR), 2)
            .unwrap_err()
            .0,
        "target_not_member"
    );
    let s3 = apply(&s2, &rm(&s2, 0x22).unwrap(), &acct(CREATOR), 2).unwrap();
    assert!(s3.member(&acct(2)).is_none());
    // a Member may not remove
    let s2b = admit(&s2, &acct(CREATOR), 3, 0x33, &[MEMBER]).unwrap();
    assert_eq!(
        apply(&s2b, &rm(&s2b, 0x22).unwrap(), &acct(3), 3)
            .unwrap_err()
            .0,
        "not_authorized"
    );
}

#[test]
fn leave_is_self_only_and_the_last_member_cannot_leave() {
    let g = world();
    let s2 = admit(&g, &acct(CREATOR), 2, 0x22, &[MEMBER]).unwrap();
    let leave = |s: &GroupState, who: u8, inc: u8| {
        intent(
            s,
            op::MEMBER_LEAVE,
            3,
            m(vec![(0, p_val(&acct(who))), (1, bytes(id16(inc)))]),
        )
    };
    // nobody leaves on another's behalf
    assert_eq!(
        apply(&s2, &leave(&s2, 2, 0x22).unwrap(), &acct(CREATOR), 2)
            .unwrap_err()
            .0,
        "not_authorized"
    );
    let s3 = apply(&s2, &leave(&s2, 2, 0x22).unwrap(), &acct(2), 2).unwrap();
    assert_eq!(s3.membership.len(), 1);
    // the creator is the only member and the only governance principal
    assert_eq!(
        apply(&s3, &leave(&s3, 1, 0x11).unwrap(), &acct(CREATOR), 3)
            .unwrap_err()
            .0,
        "last_member"
    );
    // with another member present but governance held only by the creator, leaving would empty governance
    assert_eq!(
        apply(&s2, &leave(&s2, 1, 0x11).unwrap(), &acct(CREATOR), 2)
            .unwrap_err()
            .0,
        "governance_would_empty"
    );
}

fn sorted(grants: &[&str]) -> Vec<Value> {
    let mut v: Vec<Value> = grants.iter().map(|g| Value::Text(g.to_string())).collect();
    v.sort_by_key(encode);
    v.dedup();
    v
}

fn role_args(id: u8, grants: &[&str]) -> Value {
    m(vec![(
        0,
        m(vec![
            (0, bytes(id16(id))),
            (1, Value::Text("Mod".into())),
            (2, Value::Array(sorted(grants))),
        ]),
    )])
}

#[test]
fn role_edits_are_grants_inside_the_actors_ceiling() {
    let g = world();
    let mut s = admit(&g, &acct(CREATOR), 2, 0x22, &[MEMBER]).unwrap();
    // the creator defines a moderator role that may manage roles but not governance
    let i = intent(
        &s,
        op::ROLE_DEFINE,
        1,
        role_args(
            0xb0,
            &["room.post", "room.manage_roles", "room.remove_member"],
        ),
    )
    .unwrap();
    s = apply(&s, &i, &acct(CREATOR), 2).unwrap();
    let i = intent(
        &s,
        op::ROLE_ASSIGN,
        1,
        m(vec![
            (0, p_val(&acct(2))),
            (1, bytes(id16(0x22))),
            (2, Value::Array(vec![bytes(id16(0xb0))])),
        ]),
    )
    .unwrap();
    s = apply(&s, &i, &acct(CREATOR), 3).unwrap();
    // the moderator cannot mint what it does not hold, nor a protected capability
    for bad in [
        &["room.invite"][..],
        &["room.manage_governance"][..],
        &["room.post", "room.pin"][..],
    ] {
        let i = intent(&s, op::ROLE_DEFINE, 1, role_args(0xb1, bad)).unwrap();
        assert_eq!(
            apply(&s, &i, &acct(2), 4).unwrap_err().0,
            "role_exceeds_ceiling",
            "{bad:?}"
        );
    }
    // nor assign the Admin role (it is outside the ceiling), even to itself
    let i = intent(
        &s,
        op::ROLE_ASSIGN,
        1,
        m(vec![
            (0, p_val(&acct(2))),
            (1, bytes(id16(0x22))),
            (2, Value::Array(vec![bytes(id16(ADMIN))])),
        ]),
    )
    .unwrap();
    assert_eq!(
        apply(&s, &i, &acct(2), 4).unwrap_err().0,
        "role_exceeds_ceiling"
    );
    // within its ceiling it may define a narrower role
    let i = intent(&s, op::ROLE_DEFINE, 1, role_args(0xb2, &["room.post"])).unwrap();
    assert!(apply(&s, &i, &acct(2), 4).is_ok());
}

#[test]
fn unsupported_operations_and_evidence_kinds_are_refused_not_approximated() {
    let g = world();
    let i = intent(
        &g,
        op::GOVERNANCE_SET,
        1,
        m(vec![(1, Value::Array(vec![bytes(id16(0xd1))]))]),
    )
    .unwrap();
    assert_eq!(
        apply(&g, &i, &acct(CREATOR), 1).unwrap_err().0,
        "unsupported_operation"
    );
    // an admit carrying a join_ref is the invite-link path (slice 3)
    let mut e = vec![
        (0, p_val(&acct(2))),
        (1, bytes(id16(0x22))),
        (3, Value::Uint(1)),
        (4, bytes(id16(0x55))),
    ];
    e.sort_by_key(|(k, _)| *k);
    let i = intent(&g, op::MEMBER_ADMIT, 1, m(e)).unwrap();
    assert_eq!(
        apply(&g, &i, &acct(CREATOR), 1).unwrap_err().0,
        "unsupported_operation"
    );
}

#[test]
fn the_product_cap_is_a_policy_constant_below_the_wire_cap() {
    let mut s = world();
    for n in 2..=PRODUCT_MEMBER_CAP as u8 {
        s = admit(&s, &acct(CREATOR), n, 0x40 + n, &[MEMBER]).unwrap();
    }
    assert_eq!(s.membership.len(), PRODUCT_MEMBER_CAP);
    assert_eq!(
        admit(&s, &acct(CREATOR), 200, 0xf0, &[MEMBER])
            .unwrap_err()
            .0,
        "group_full"
    );
}

// ------------------------------------------------------------------ persistence

#[test]
fn the_head_persists_in_app_state_and_survives_a_profile_round_trip() {
    let g = world();
    let mut app = AppState::default();
    app.group_install_genesis(&g).unwrap();
    assert_eq!(app.group_install_genesis(&g).unwrap_err().0, "group_exists");
    let i = intent(
        &g,
        op::PROFILE_SET,
        1,
        m(vec![(0, m(vec![(0, Value::Text("Renamed".into()))]))]),
    )
    .unwrap();
    let next = app.group_apply(&i, &acct(CREATOR), 1).unwrap();
    // a replay of the same intent is stale against the new head and changes nothing
    assert_eq!(
        app.group_apply(&i, &acct(CREATOR), 1).unwrap_err().0,
        "stale_parent"
    );
    assert_eq!(app.group(&g.conversation_id).unwrap().unwrap(), next);

    let json = serde_json::to_string(&app).unwrap();
    let back: AppState = serde_json::from_str(&json).unwrap();
    assert_eq!(back.group(&g.conversation_id).unwrap().unwrap(), next);
    // a corrupted record is noticed, not trusted
    let mut bad = back.clone();
    bad.groups.values_mut().next().unwrap().head_hash = "00".repeat(32);
    assert_eq!(
        bad.group(&g.conversation_id).unwrap_err().0,
        "record_corrupt"
    );
    // an older record without the field loads with no groups
    let mut j: J = serde_json::from_str(&json).unwrap();
    j.as_object_mut().unwrap().remove("groups");
    let old: AppState = serde_json::from_value(j).unwrap();
    assert!(old.groups.is_empty());
}

// ------------------------------------------------------------------ binding order key

#[test]
fn the_ordering_key_ranks_reduction_then_sequenced_then_device_then_hash() {
    let mk = |kind: u64, reducing: bool, dev: u8| Binding {
        conversation_id: vec![9; 32],
        binding_kind: kind,
        reducing,
        previous_state_hash: vec![1; 32],
        previous_state_version: 1,
        previous_mls_epoch: 0,
        resulting_mls_epoch: 1,
        intent_hash: Some(vec![2; 32]),
        mls_commit_hash: vec![3; 32],
        resulting_state_hash: vec![4; 32],
        committer_device: id16(dev),
        has_readded: false,
    };
    let k = |b: &Binding| b.ordering_key(b"x");
    assert!(
        k(&mk(2, true, 9)) < k(&mk(1, false, 1)),
        "a reduction beats a non-reduction"
    );
    assert!(
        k(&mk(1, true, 9)) < k(&mk(2, true, 1)),
        "a coordinator-sequenced Commit beats a direct one"
    );
    assert!(
        k(&mk(1, true, 1)) < k(&mk(1, true, 2)),
        "then the lower committer DeviceID"
    );
}

// ------------------------------------------------------------------ property

#[derive(Clone, Debug)]
enum Step {
    Admit(u8, u8, bool),
    Remove(u8, u8),
    Leave(u8),
    Profile(u8),
    Define(u8, u8),
    Assign(u8, u8),
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        (2u8..8, 0u8..4, any::<bool>()).prop_map(|(a, b, c)| Step::Admit(a, b, c)),
        (1u8..8, 0u8..4).prop_map(|(a, b)| Step::Remove(a, b)),
        (1u8..8).prop_map(Step::Leave),
        (0u8..8).prop_map(Step::Profile),
        (0u8..8, 0u8..8).prop_map(|(a, b)| Step::Define(a, b)),
        (1u8..8, 0u8..4).prop_map(|(a, b)| Step::Assign(a, b)),
    ]
}

const CAP_POOL: [&str; 7] = [
    "room.post",
    "room.pin",
    "room.invite",
    "room.manage_roles",
    "room.manage_admission",
    "room.remove_member",
    "room.manage_governance",
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// A sequence of intents, applied by arbitrary actors, never breaks the state rules: every Ok result validates,
    /// chains to its parent's hash, advances version and epoch by one, and re-decodes to itself.
    #[test]
    fn valid_intent_sequences_never_break_the_state_rules(steps in proptest::collection::vec((1u8..8, step()), 1..40)) {
        let mut s = world();
        for (actor, st) in steps {
            let actor = acct(actor);
            let role = |n: u8| id16([ADMIN, MEMBER, 0xb0, 0xb1][(n % 4) as usize]);
            let built = match st {
                Step::Admit(w, inc, as_admin) => intent(
                    &s, op::MEMBER_ADMIT, 1,
                    admit_args(&acct(w), 0x20 + inc, &[if as_admin { ADMIN } else { MEMBER }]),
                ),
                Step::Remove(w, inc) => intent(&s, op::MEMBER_REMOVE, 1,
                    m(vec![(0, p_val(&acct(w))), (1, bytes(id16(0x20 + inc)))])),
                Step::Leave(w) => {
                    let inc = s.member(&acct(w)).map(|x| x.incarnation.clone()).unwrap_or(id16(0));
                    intent(&s, op::MEMBER_LEAVE, 3, m(vec![(0, p_val(&acct(w))), (1, bytes(inc))]))
                }
                Step::Profile(n) => intent(&s, op::PROFILE_SET, 1,
                    m(vec![(0, m(vec![(0, Value::Text(format!("g{n}")))]))])),
                Step::Define(idn, mask) => {
                    let grants: Vec<&str> = CAP_POOL.iter().enumerate()
                        .filter(|(i, _)| mask & (1 << (i % 3)) != 0 || *i == (mask as usize % 7))
                        .map(|(_, c)| *c).collect();
                    intent(&s, op::ROLE_DEFINE, 1, role_args(0xb0 + idn % 2, &grants))
                }
                Step::Assign(w, rn) => {
                    let inc = s.member(&acct(w)).map(|x| x.incarnation.clone()).unwrap_or(id16(0));
                    intent(&s, op::ROLE_ASSIGN, 1, m(vec![
                        (0, p_val(&acct(w))), (1, bytes(inc)), (2, Value::Array(vec![bytes(role(rn))])),
                    ]))
                }
            };
            let Ok(i) = built else { continue };
            if let Ok(next) = apply(&s, &i, &actor, s.mls_epoch_ref + 1) {
                prop_assert!(validate_state(&next).is_ok());
                let parent_hash = s.hash();
                prop_assert_eq!(next.previous_state_hash.as_deref(), Some(parent_hash.as_slice()));
                prop_assert_eq!(next.state_version, s.state_version + 1);
                prop_assert_eq!(next.mls_epoch_ref, s.mls_epoch_ref + 1);
                prop_assert!(!next.membership.is_empty());
                prop_assert!(next.membership.len() <= PRODUCT_MEMBER_CAP);
                prop_assert_eq!(GroupState::decode(&next.encode()).unwrap(), next.clone());
                // a protected capability is never in the ceiling of a member who is not a governance principal
                for mem in &next.membership {
                    if !next.governance.principals.contains(&mem.principal) {
                        let c = next.ceiling_of(&mem.principal);
                        prop_assert!(!c.contains("room.manage_governance") && !c.contains("room.manage_security_policy"));
                    }
                }
                s = next;
            }
        }
    }
}
