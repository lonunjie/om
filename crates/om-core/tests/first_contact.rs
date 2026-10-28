// SPDX-License-Identifier: MPL-2.0
//! First contact as ADR-0181 builds it: a per-invite intro identity, a link that commits to its key digest, a request
//! key trusted only from a descriptor that identity signed, a Core-issued ticket, a requester whose account stays
//! hidden until acceptance, and `ACCEPTED_UNBOUND` until both `IntroIdentityBinding`s verify.

mod common;

use common::{BindOpts, FakeRequester, app_state, create_profile, invite};
use om_core::api::*;
use om_core::control;
use om_core::crypto::cbor::{self, V};
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::{hex, unhex};
use om_core::host::NetCall;
use om_core::intro;
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};

fn account_hex(rt: &Runtime, ctx: &ProfileExecutionContext) -> String {
    let s = app_state(rt, ctx);
    let list = |k: &str| -> Vec<Vec<u8>> {
        s[k].as_array()
            .unwrap()
            .iter()
            .map(|x| unhex(x.as_str().unwrap()).unwrap())
            .collect()
    };
    hex(
        &control::verify_chain(&list("acsChain"), &list("acsCredentials"))
            .unwrap()
            .account_id,
    )
}

fn rel_state(rt: &Runtime, ctx: &ProfileExecutionContext) -> String {
    app_state(rt, ctx)["relationships"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .map(|r| r["state"].as_str().unwrap().to_string())
        .unwrap_or_default()
}

struct Duo {
    net: LoopbackNetwork,
    alice: Runtime,
    bob: Runtime,
    a: ProfileExecutionContext,
    b: ProfileExecutionContext,
}

fn duo() -> Duo {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let mut bob = Runtime::new("bob", &net);
    let a = create_profile(&mut alice, "p-alice", "Alice");
    let b = create_profile(&mut bob, "p-bob", "Bob");
    Duo {
        net,
        alice,
        bob,
        a,
        b,
    }
}

fn link_of(rt: &mut Runtime, ctx: &ProfileExecutionContext, op: &str) -> String {
    rt.dispatch(ctx, op, IntentBody::CreateInvite);
    rt.flush_net(ctx);
    rt.query::<Option<InviteView>>(Query::GetInvite {
        ctx: ctx.clone(),
        operation_id: op.into(),
    })
    .unwrap()
    .link
}

/// Bob accepts Alice's link and Alice receives the request (not yet accepted).
fn requested(d: &mut Duo) -> String {
    let link = link_of(&mut d.alice, &d.a, "inv");
    d.bob.dispatch(
        &d.b,
        "acc",
        IntentBody::AcceptInvitation { link: link.clone() },
    );
    d.bob.flush_net(&d.b); // resolve the invite
    d.bob.flush_net(&d.b); // deposit the request
    d.alice.poll(&d.a);
    link
}

fn first_intro(d: &Duo) -> IntroductionRequest {
    d.alice
        .query::<Vec<IntroductionRequest>>(Query::GetIntroductions { ctx: d.a.clone() })
        .remove(0)
}

#[test]
fn the_inviter_sees_an_unverified_intro_key_and_never_the_requesters_account() {
    let mut d = duo();
    let bob_account = account_hex(&d.bob, &d.b);
    requested(&mut d);
    let req = first_intro(&d);
    assert_eq!(req.verification, VerificationCode::Unverified);
    assert_eq!(
        req.display_name, "Bob",
        "the label is shown, as a self-chosen name"
    );
    let before = serde_json::to_string(&app_state(&d.alice, &d.a)).unwrap();
    assert!(
        !before.contains(&bob_account),
        "the pending request holds no trace of the requester's AccountID"
    );
    let intro = &app_state(&d.alice, &d.a)["intros"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone();
    assert!(intro.get("peerAcsBody").is_none() && intro.get("peerCredentials").is_none());
    assert!(
        intro["peerIntroPk"].as_str().unwrap().len() > 3000,
        "an intro key is held instead"
    );

    // After acceptance but before the requester's Bind, the account is still unknown: ACCEPTED_UNBOUND.
    d.alice.dispatch(
        &d.a,
        "ok",
        IntentBody::AcceptIntroduction { request_id: req.id },
    );
    d.alice.flush_net(&d.a);
    assert_eq!(rel_state(&d.alice, &d.a), "accepted_unbound");
    let mid = serde_json::to_string(&app_state(&d.alice, &d.a)).unwrap();
    assert!(!mid.contains(&bob_account), "still hidden once accepted");
    let list: Vec<ConversationSummary> = d
        .alice
        .query(Query::GetConversationList { ctx: d.a.clone() });
    assert_eq!(list[0].relationship, RelationshipStateCode::AcceptedUnbound);
    assert!(
        list[0]
            .notices
            .iter()
            .any(|n| n.code == SecurityNoticeCode::UnverifiedContact),
        "an unbound contact carries the unverified notice"
    );

    // Only after both bindings verify does the account appear, and the relationship is ACTIVE.
    settle_all(&mut [(&mut d.alice, &d.a), (&mut d.bob, &d.b)]);
    let after = serde_json::to_string(&app_state(&d.alice, &d.a)).unwrap();
    assert!(after.contains(&bob_account));
    for (rt, ctx) in [(&d.alice, &d.a), (&d.bob, &d.b)] {
        let l: Vec<ConversationSummary> = rt.query(Query::GetConversationList { ctx: ctx.clone() });
        assert_eq!(l[0].relationship, RelationshipStateCode::Active);
        assert!(l[0].notices.is_empty());
    }
}

#[test]
fn hello_follows_both_bindings_and_each_side_activates_only_then() {
    let mut d = duo();
    requested(&mut d);
    let req = first_intro(&d);
    d.alice.dispatch(
        &d.a,
        "ok",
        IntentBody::AcceptIntroduction { request_id: req.id },
    );
    d.alice.flush_net(&d.a); // Accept deposited
    assert_eq!(rel_state(&d.bob, &d.b), "awaiting_acceptance");
    d.bob.poll(&d.b); // Accept verified: Bob has verified Alice's binding and sends his own
    assert_eq!(
        rel_state(&d.bob, &d.b),
        "accepted_unbound",
        "Bob cannot yet know Alice verified his"
    );
    d.bob.flush_net(&d.b); // Bind deposited
    assert_eq!(rel_state(&d.alice, &d.a), "accepted_unbound");
    d.alice.poll(&d.a); // Bind verified: both bindings are known to Alice
    assert_eq!(rel_state(&d.alice, &d.a), "establishing");
    // No ordinary message goes out yet on either side.
    for (rt, ctx, conv_of) in [(&mut d.bob, d.b.clone(), 0), (&mut d.alice, d.a.clone(), 1)] {
        let conv = rt
            .query::<Vec<ConversationSummary>>(Query::GetConversationList { ctx: ctx.clone() })[0]
            .id
            .clone();
        rt.dispatch(
            &ctx,
            &format!("m{conv_of}"),
            IntentBody::SendMessage {
                conversation_id: conv,
                draft: Draft { text: "hi".into() },
            },
        );
    }
    assert_eq!(
        d.bob.outcome(&d.b, "m0"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Unavailable
        }),
        "the requester cannot send while unbound"
    );
    assert_eq!(
        d.alice.outcome(&d.a, "m1"),
        Some(OutcomeClass::PendingNetwork),
        "the inviter's message queues behind the Hello while establishing"
    );
    settle_all(&mut [(&mut d.alice, &d.a), (&mut d.bob, &d.b)]);
    assert_eq!(rel_state(&d.alice, &d.a), "active");
    assert_eq!(rel_state(&d.bob, &d.b), "active");
    let conv = d
        .bob
        .query::<Vec<ConversationSummary>>(Query::GetConversationList { ctx: d.b.clone() })[0]
        .id
        .clone();
    let v: ConversationView = d.bob.query(Query::GetConversationView {
        ctx: d.b.clone(),
        conversation_id: conv,
    });
    assert_eq!(
        v.timeline.len(),
        1,
        "the queued message arrived after the Hello"
    );
}

#[test]
fn the_link_commits_to_the_intro_key_and_the_record_is_signed_by_it() {
    let mut d = duo();
    let (link, record) = invite(&mut d.alice, &d.a, "inv");
    assert_eq!(link.len(), 48, "invite id plus intro key digest");
    let rec = cbor::decode(&record).unwrap();
    let pk = rec.get(3).unwrap().b().unwrap();
    assert_eq!(intro::key_digest(pk).to_vec(), link[16..].to_vec());
    let (d_bytes, t_bytes) = (
        rec.get(1).unwrap().b().unwrap(),
        rec.get(2).unwrap().b().unwrap(),
    );
    // One intro identity (role 4) signs both the descriptor and the ticket.
    let desc = intro::open_intro(d_bytes, intro::LBL_DESCRIPTOR_SIG, pk).unwrap();
    let ticket = intro::open_intro(t_bytes, intro::LBL_TICKET_SIG, pk).unwrap();
    // Both are W0-valid objects (envelope included) and carry no account material.
    let reg = om_codec::Registry::load();
    for (name, bytes) in [
        ("ContactDescriptor", d_bytes),
        ("IntroductionTicket", t_bytes),
    ] {
        let r = reg.check(name, bytes, false);
        assert!(r.is_ok(), "{name}: {:?}", r.err());
    }
    let info = intro::read_descriptor(&desc.body).unwrap();
    assert!(info.request_key.is_some());
    let t = intro::read_ticket(&ticket.body).unwrap();
    assert_eq!(
        (t.max_uses, t.payload_limit),
        (1, intro::PAYLOAD_LIMIT_BYTES)
    );
    assert_eq!(
        t.request_key, info.request_key,
        "the ticket repeats the signed request key"
    );
    assert!(
        ticket.body.opt(10).is_none(),
        "admission_proof is empty until an IntroGate adapter exists"
    );
    assert_eq!(ticket.body.opt(7), Some(&V::Bool(false)), "no attachments");
    assert_eq!(ticket.body.opt(8), Some(&V::Bool(false)), "no calls");
    let alice_account = unhex(&account_hex(&d.alice, &d.a)).unwrap();
    assert!(!record.windows(32).any(|w| w == alice_account.as_slice()));
}

#[test]
fn intro_identities_are_per_invite_derived_and_distinct() {
    let mut d = duo();
    let (l1, r1) = invite(&mut d.alice, &d.a, "i1");
    let (l2, r2) = invite(&mut d.alice, &d.a, "i2");
    let key = |r: &[u8]| {
        cbor::decode(r)
            .unwrap()
            .get(3)
            .unwrap()
            .b()
            .unwrap()
            .to_vec()
    };
    assert_ne!(key(&r1), key(&r2), "no intro key is reused across invites");
    assert_ne!(l1[16..], l2[16..]);
    // Deterministic derivation from the profile secret and the invite id.
    let (root, id) = ([3u8; 32], [9u8; 16]);
    let same = |a: &[u8; 32], b: &[u8; 16]| intro::derive_signer(a, b).public();
    assert_eq!(same(&root, &id), same(&root, &id));
    assert_ne!(same(&root, &id), same(&root, &[8u8; 16]));
    assert_ne!(same(&root, &id), same(&[4u8; 32], &id));
    let st = app_state(&d.alice, &d.a);
    assert!(
        st["introRoot"].as_str().is_some(),
        "one profile secret, created on first use"
    );
    let keys: std::collections::BTreeSet<_> = st["invites"]
        .as_object()
        .unwrap()
        .values()
        .map(|i| i["introPk"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(keys.len(), 2);
}

fn put(net: &LoopbackNetwork, link: &[u8], record: Vec<u8>) {
    net.perform(&NetCall::PutInvite {
        invite_id: link[..16].to_vec(),
        descriptor: record,
        expires_at: 0,
    });
}

#[test]
fn a_request_key_is_trusted_only_from_a_descriptor_signed_by_the_linked_intro_identity() {
    let mut d = duo();
    let (link, record) = invite(&mut d.alice, &d.a, "inv");
    let part = |r: &[u8], k: u64| {
        cbor::decode(r)
            .unwrap()
            .get(k)
            .unwrap()
            .b()
            .unwrap()
            .to_vec()
    };
    let (desc, ticket, pk) = (part(&record, 1), part(&record, 2), part(&record, 3));
    // Mallory's complete, internally valid record (own intro key, own request key).
    let net2 = LoopbackNetwork::default();
    let mut mallory = Runtime::new("mallory", &net2);
    let m = create_profile(&mut mallory, "p-mallory", "Mallory");
    let (_, mr) = invite(&mut mallory, &m, "m");
    let (m_desc, m_ticket, m_pk) = (part(&mr, 1), part(&mr, 2), part(&mr, 3));
    let rec = |a: Option<&[u8]>, b: Option<&[u8]>, c: Option<&[u8]>| {
        let mut v = vec![];
        for (k, x) in [(1, a), (2, b), (3, c)] {
            if let Some(x) = x {
                v.push((k, V::B(x.to_vec())));
            }
        }
        cbor::encode(&V::M(v))
    };
    // A forger who copies every field (invite id, service binding = the link's digest) but signs with its own key and
    // names its own request key: only the link's key digest can catch it.
    let forger =
        om_core::crypto::sig::DeviceSigner::generate(&mut InsecureTestRng::from_label(b"forger"));
    let forger_pk = forger.public();
    let (_, request_pk) = om_core::crypto::hpke::derive_key_pair(&[9u8; 32]);
    let mut frng = InsecureTestRng::from_label(b"forger-sign");
    let invite_id: [u8; 16] = link[..16].try_into().unwrap();
    let digest: [u8; 32] = link[16..].try_into().unwrap();
    let forged_desc = intro::sign_intro(
        &forger,
        intro::LBL_DESCRIPTOR_SIG,
        intro::descriptor_body(
            &invite_id,
            "Alice",
            &intro::service_uri(&[1; 16]),
            2_000_000_000,
            Some(&request_pk),
        ),
        &mut frng,
    );
    let forged_ticket = intro::sign_intro(
        &forger,
        intro::LBL_TICKET_SIG,
        intro::ticket_body(&invite_id, &digest, 2_000_000_000, &[3; 16], &request_pk),
        &mut frng,
    );
    let cases: Vec<(&str, Vec<u8>)> = vec![
        (
            "a forger's self-consistent record signed with its own key",
            rec(Some(&forged_desc), Some(&forged_ticket), Some(&forger_pk)),
        ),
        ("a record signed by another intro identity", mr.clone()),
        (
            "the right objects under another identity's key",
            rec(Some(&desc), Some(&ticket), Some(&m_pk)),
        ),
        (
            "the right descriptor with another identity's ticket",
            rec(Some(&desc), Some(&m_ticket), Some(&pk)),
        ),
        (
            "another identity's descriptor with the right ticket",
            rec(Some(&m_desc), Some(&ticket), Some(&pk)),
        ),
        ("no ticket", rec(Some(&desc), None, Some(&pk))),
        ("no descriptor", rec(None, Some(&ticket), Some(&pk))),
        ("no key", rec(Some(&desc), Some(&ticket), None)),
        ("garbage", vec![1, 2, 3]),
    ];
    let uri = format!("om:invite/{}", om_core_b64url(&link));
    for (n, (name, bad)) in cases.into_iter().enumerate() {
        put(&d.net, &link, bad);
        let op = format!("acc-{n}");
        d.bob.dispatch(
            &d.b,
            &op,
            IntentBody::AcceptInvitation { link: uri.clone() },
        );
        d.bob.flush_net(&d.b);
        assert_eq!(
            d.bob.outcome(&d.b, &op),
            Some(OutcomeClass::Rejected {
                reason: RejectReason::IntegrityFailure
            }),
            "{name}"
        );
    }
    // Control: the genuine record resolves and the request is sent.
    put(&d.net, &link, record);
    d.bob
        .dispatch(&d.b, "ok", IntentBody::AcceptInvitation { link: uri });
    d.bob.flush_net(&d.b);
    assert_eq!(
        d.bob.outcome(&d.b, "ok"),
        Some(OutcomeClass::PendingNetwork)
    );
    assert_eq!(rel_state(&d.bob, &d.b), "awaiting_acceptance");
}

fn om_core_b64url(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..=c.len() {
            out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

#[test]
fn a_bind_for_the_wrong_intro_identity_or_request_is_refused() {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let a = create_profile(&mut alice, "p-alice", "Alice");
    let mut rng = InsecureTestRng::from_label(b"bind-wrong");
    let now_ms = alice.clock.get();
    let now_s = now_ms / 1000;
    let other_intro = intro::derive_signer(&[5; 32], &[5; 16]).public();
    let cases: Vec<(&str, BindOpts)> = vec![
        (
            "a binding for another intro identity",
            BindOpts {
                intro_identity: Some(other_intro),
                ..BindOpts::default()
            },
        ),
        (
            "a binding scoped to another request key",
            BindOpts {
                request_digest: Some([7; 32]),
                ..BindOpts::default()
            },
        ),
    ];
    for (n, (name, opts)) in cases.into_iter().enumerate() {
        let mailbox = [n as u8 + 10; 16];
        let (link, record) = invite(&mut alice, &a, &format!("inv-{n}"));
        let mut r = FakeRequester::new(&mut rng);
        r.mailbox = mailbox;
        r.request(&net, &mut rng, &link, &record, now_ms, 3600, "Eve");
        alice.poll(&a);
        let req = alice
            .query::<Vec<IntroductionRequest>>(Query::GetIntroductions { ctx: a.clone() })
            .remove(0);
        alice.dispatch(
            &a,
            &format!("ok-{n}"),
            IntentBody::AcceptIntroduction { request_id: req.id },
        );
        alice.flush_net(&a);
        let acc = r.accept(&net, now_s).expect("accept");
        let find = |alice: &Runtime| {
            app_state(alice, &a)["relationships"]
                .as_object()
                .unwrap()
                .values()
                .find(|x| x["peerMailbox"] == hex(&mailbox))
                .unwrap()
                .clone()
        };
        let bind = r.bind_payload(&mut rng, now_s, opts);
        r.deposit_to_inviter(&net, &mut rng, &acc, &bind, now_s);
        alice.poll(&a);
        let rel = find(&alice);
        assert_eq!(rel["state"], "accepted_unbound", "{name}");
        assert_eq!(rel["peerAccountId"], "", "{name}: no account pinned");
        assert_eq!(rel["integrityFailures"], 1, "{name}");
        // Control: the same requester's honest Bind then binds, so the test does reach the verifier.
        let honest = r.bind_payload(&mut rng, now_s, BindOpts::default());
        r.deposit_to_inviter(&net, &mut rng, &acc, &honest, now_s);
        alice.poll(&a);
        assert_eq!(find(&alice)["state"], "establishing", "{name}: control");
    }
}

#[test]
fn an_invite_serves_one_request_and_the_second_is_dropped() {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let a = create_profile(&mut alice, "p-alice", "Alice");
    let mut rng = InsecureTestRng::from_label(b"single-use");
    let now_ms = alice.clock.get();
    let (link, record) = invite(&mut alice, &a, "inv");
    let mut one = FakeRequester::new(&mut rng);
    one.request(&net, &mut rng, &link, &record, now_ms, 3600, "One");
    alice.poll(&a);
    let mut two = FakeRequester::new(&mut rng);
    two.request(&net, &mut rng, &link, &record, now_ms, 3600, "Two");
    alice.poll(&a);
    let list: Vec<IntroductionRequest> = alice.query(Query::GetIntroductions { ctx: a.clone() });
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].display_name, "One");
}

#[test]
fn an_unfinished_first_contact_expires_with_the_bootstrap_window() {
    let mut d = duo();
    requested(&mut d);
    let req = first_intro(&d);
    d.alice.dispatch(
        &d.a,
        "ok",
        IntentBody::AcceptIntroduction { request_id: req.id },
    );
    d.alice.flush_net(&d.a);
    assert_eq!(rel_state(&d.alice, &d.a), "accepted_unbound");
    // Bob never answers. After the 7-day bootstrap window both unfinished sides are dropped on the next activity.
    d.alice.tick(8 * 24 * 3600 * 1000);
    d.bob.tick(8 * 24 * 3600 * 1000);
    d.alice.poll(&d.a);
    d.bob.poll(&d.b);
    d.alice
        .dispatch(&d.a, "noop", IntentBody::ClearAdaptiveNavigation);
    d.bob
        .dispatch(&d.b, "noop", IntentBody::ClearAdaptiveNavigation);
    assert_eq!(
        rel_state(&d.alice, &d.a),
        "",
        "the inviter's unbound contact is gone"
    );
    assert_eq!(
        rel_state(&d.bob, &d.b),
        "",
        "the requester's unanswered request is gone"
    );
}

fn long_chain_account(
    net: &LoopbackNetwork,
    label: &str,
    extra_links: usize,
) -> (Runtime, ProfileExecutionContext) {
    let mut rt = Runtime::new(label, net);
    let ctx = create_profile(&mut rt, &format!("p-{label}"), label);
    for _ in 0..extra_links {
        let (_, step) = rt.core.debug_link_messaging_device(&ctx).unwrap();
        rt.settle(&step);
    }
    (rt, ctx)
}

#[test]
fn the_requester_refuses_an_inviters_chain_above_the_first_contact_cap_and_accepts_one_at_it() {
    let cap = om_core::FIRST_CONTACT_MAX_CHAIN;
    for (links, binds) in [(cap - 1, true), (cap, false)] {
        let net = LoopbackNetwork::default();
        let (mut alice, a) = long_chain_account(&net, "alice", links);
        let mut bob = Runtime::new("bob", &net);
        let b = create_profile(&mut bob, "p-bob", "Bob");
        let link = link_of(&mut alice, &a, "inv");
        bob.dispatch(&b, "acc", IntentBody::AcceptInvitation { link });
        settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
        let req = alice
            .query::<Vec<IntroductionRequest>>(Query::GetIntroductions { ctx: a.clone() })
            .remove(0);
        alice.dispatch(
            &a,
            "ok",
            IntentBody::AcceptIntroduction { request_id: req.id },
        );
        settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
        let want = if binds {
            "active"
        } else {
            "awaiting_acceptance"
        };
        assert_eq!(rel_state(&bob, &b), want, "{} chain entries", links + 1);
        if !binds {
            let st = app_state(&bob, &b);
            let rel = st["relationships"]
                .as_object()
                .unwrap()
                .values()
                .next()
                .unwrap()
                .clone();
            assert_eq!(rel["peerAccountId"], "", "nothing was pinned");
            assert_eq!(rel["integrityFailures"], 1);
        }
    }
}

#[test]
fn a_v2_profile_loads_and_drops_what_cannot_finish_under_the_new_flow() {
    let mut d = duo();
    let (_, _) = invite(&mut d.alice, &d.a, "stale");
    requested(&mut d);
    // Make the stored state look like version 2: keep the pending introduction and unused invite.
    let recs = d.alice.store.records("p-alice");
    let rec = recs.iter().find(|r| r.key == "state").unwrap();
    let v = cbor::decode(&rec.value).unwrap();
    let mut app: serde_json::Value =
        serde_json::from_slice(v.get(1).unwrap().b().unwrap()).unwrap();
    assert!(!app["intros"].as_object().unwrap().is_empty());
    app["version"] = serde_json::json!(2);
    let old = cbor::encode(&V::M(vec![
        (0, v.get(0).unwrap().clone()),
        (1, V::B(serde_json::to_vec(&app).unwrap())),
    ]));
    let mut core = om_core::Core::new(
        InsecureTestRng::from_label(b"migrate"),
        || 1_790_000_000_000u64,
        om_core::runtime::home(),
    );
    let ctx = core
        .open_profile(
            "p-alice",
            &[om_core::host::Record {
                key: "state".into(),
                value: old,
            }],
        )
        .expect("a version 2 profile still opens");
    let intros: serde_json::Value = core
        .query(&Query::GetIntroductions { ctx: ctx.clone() })
        .unwrap();
    assert_eq!(
        intros.as_array().unwrap().len(),
        0,
        "pending introductions are dropped"
    );
    let st = core.debug_app_state(&ctx).unwrap();
    assert_eq!(st["version"], 3);
    assert!(
        st["invites"]
            .as_object()
            .unwrap()
            .values()
            .all(|i| i["revoked"] == true || i["used"] == true)
    );
}
