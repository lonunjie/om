// SPDX-License-Identifier: MPL-2.0
//! Direct transfer through the engine (group G): a file too big for the delivery service is refused for the spool before
//! anything is sent, then offered to go straight to the other person; the message is sent at once and only the file
//! waits; the offer survives a restart; the bytes cross a loopback channel and open only if they hash to the digest the
//! message committed to. No wire codec, no LAN or WAN transport: the channel here is a pair of vectors.

mod common;

use om_core::api::*;
use om_core::attach::{AttachError, Meta};
use om_core::direct::{DirectFrame, ErrorCode, FetchState};
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};

struct Pair {
    net: LoopbackNetwork,
    alice: Runtime,
    bob: Runtime,
    a: ProfileExecutionContext,
    b: ProfileExecutionContext,
    a_conv: String,
    b_conv: String,
}

fn active_pair() -> Pair {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let mut bob = Runtime::new("bob", &net);
    let a = common::create_profile(&mut alice, "p-a", "Alice");
    let b = common::create_profile(&mut bob, "p-b", "Bob");
    alice.dispatch(&a, "inv", IntentBody::CreateInvite);
    alice.flush_net(&a);
    let link = alice
        .query::<Option<InviteView>>(Query::GetInvite {
            ctx: a.clone(),
            operation_id: "inv".into(),
        })
        .unwrap()
        .link;
    bob.dispatch(&b, "acc", IntentBody::AcceptInvitation { link });
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    let intros: Vec<IntroductionRequest> = alice.query(Query::GetIntroductions { ctx: a.clone() });
    alice.dispatch(
        &a,
        "i",
        IntentBody::AcceptIntroduction {
            request_id: intros[0].id.clone(),
        },
    );
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    let conv = |rt: &Runtime, c: &ProfileExecutionContext| {
        let l: Vec<ConversationSummary> = rt.query(Query::GetConversationList { ctx: c.clone() });
        assert_eq!(l[0].relationship, RelationshipStateCode::Active);
        l[0].id.clone()
    };
    let (a_conv, b_conv) = (conv(&alice, &a), conv(&bob, &b));
    Pair {
        net,
        alice,
        bob,
        a,
        b,
        a_conv,
        b_conv,
    }
}

fn view(rt: &Runtime, ctx: &ProfileExecutionContext, conv: &str) -> Vec<TimelineItem> {
    rt.query::<ConversationView>(Query::GetConversationView {
        ctx: ctx.clone(),
        conversation_id: conv.into(),
    })
    .timeline
}

fn parts(it: &TimelineItem) -> (&str, &str, &[AttachmentView], &Option<PresentationView>) {
    match it {
        TimelineItem::Outgoing {
            message_id,
            text,
            attachments,
            presentation,
            ..
        }
        | TimelineItem::Incoming {
            message_id,
            text,
            attachments,
            presentation,
            ..
        } => (message_id, text, attachments, presentation),
    }
}

fn file(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 31 % 251) as u8).collect()
}

fn meta() -> Meta {
    Meta {
        media_type: "video/mp4".into(),
        filename: Some("holiday.mp4".into()),
        ..Default::default()
    }
}

/// 6 MiB: over the loopback store's 4 MiB, under the 16 MiB ceiling.
const BIG: usize = 6 * 1024 * 1024;

fn settle(p: &mut Pair) {
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
}

fn offer(p: &mut Pair, op: &str, text: &str, plain: &[u8]) -> String {
    let (out, step) = p
        .alice
        .core
        .prepare_direct_attachment(&p.a, plain, meta())
        .unwrap();
    assert!(p.alice.settle(&step));
    let prep = out.expect("prepared");
    assert!(prep.ciphertext.is_empty(), "Core keeps the sealed object");
    p.alice.dispatch(
        &p.a,
        op,
        IntentBody::SendAttachmentsDirect {
            conversation_id: p.a_conv.clone(),
            draft: Draft { text: text.into() },
            attachments: vec![AttachmentInput {
                blob_id: prep.blob_id.clone(),
                voice_note: None,
            }],
        },
    );
    prep.blob_id
}

fn mid(rt: &Runtime, ctx: &ProfileExecutionContext, conv: &str) -> String {
    let v = view(rt, ctx, conv);
    parts(v.last().expect("a message")).0.to_string()
}

fn state_of(rt: &Runtime, ctx: &ProfileExecutionContext, conv: &str) -> AttachmentState {
    let v = view(rt, ctx, conv);
    parts(v.last().unwrap()).2[0].state
}

/// Move frames both ways until the channel is quiet; `drop_after` chunks delivered cuts the channel there.
fn run_channel(p: &mut Pair, bob_msg: &str, first: DirectFrame, drop_after: Option<usize>) {
    let (mut to_a, mut to_b, mut chunks) = (vec![first], vec![], 0usize);
    loop {
        for f in to_a.drain(..) {
            let (out, step) = p.alice.core.direct_serve(&p.a, &f).unwrap();
            assert!(p.alice.settle(&step));
            to_b.extend(out);
        }
        to_b.extend(p.alice.core.direct_pump(&p.a, 0).unwrap());
        if to_b.is_empty() {
            return;
        }
        for f in to_b.drain(..) {
            if matches!(f, DirectFrame::Chunk { .. }) {
                chunks += 1;
                if drop_after.is_some_and(|n| chunks > n) {
                    return;
                }
            }
            to_a.extend(
                p.bob
                    .core
                    .direct_fetch_handle(&p.b, bob_msg, 0, &f)
                    .unwrap(),
            );
        }
    }
}

#[test]
fn too_big_for_the_service_is_refused_before_sending_and_offered_direct() {
    let mut p = active_pair();
    let (puts, deposits) = (p.net.blob_put_count(), p.net.deposit_count());
    let err = p
        .alice
        .prepare_attachment(&p.a, &file(BIG), meta())
        .unwrap_err();
    assert_eq!(err.reject_reason(), RejectReason::TooLarge);
    assert_eq!(
        (p.net.blob_put_count(), p.net.deposit_count()),
        (puts, deposits)
    );
    let plan = |p: &Pair, size| -> AttachmentPlanView {
        p.alice.query(Query::PlanAttachment {
            ctx: p.a.clone(),
            size,
        })
    };
    assert_eq!(plan(&p, 1024), AttachmentPlanView::Spool);
    assert_eq!(
        plan(&p, BIG as u64),
        AttachmentPlanView::OfferDirect {
            limit: 4 * 1024 * 1024 - 16
        }
    );
    assert!(matches!(
        plan(&p, 17 << 20),
        AttachmentPlanView::TooLarge { .. }
    ));
    assert_eq!(plan(&p, 0), AttachmentPlanView::Empty);
    // A file that fits the spool is not a direct candidate; nothing was sealed for it.
    let (out, _) = p
        .alice
        .core
        .prepare_direct_attachment(&p.a, &file(1024), meta())
        .unwrap();
    assert_eq!(out.unwrap_err(), AttachError::Invalid);
}

#[test]
fn the_message_goes_at_once_and_the_file_crosses_a_direct_channel() {
    let mut p = active_pair();
    let plain = file(BIG);
    let blob = offer(&mut p, "d1", "the whole trip", &plain);
    settle(&mut p);
    assert_eq!(p.net.blob_put_count(), 0, "nothing was stored for later");
    assert!(
        !p.net.blob_ciphertexts().iter().any(|c| c.len() == BIG + 16),
        "the sealed object never reached the service"
    );
    assert_eq!(
        state_of(&p.alice, &p.a, &p.a_conv),
        AttachmentState::WaitingForRecipient
    );
    assert_eq!(
        state_of(&p.bob, &p.b, &p.b_conv),
        AttachmentState::RemoteDirect
    );
    let bm = mid(&p.bob, &p.b, &p.b_conv);
    // The ordinary fetch has nothing to fetch; only the direct one applies.
    assert!(p.bob.core.attachment_fetch(&p.b, &bm, 0).is_err());

    // The channel drops after 5 chunks; the second attempt resumes from the held prefix.
    let first = p.bob.core.direct_fetch_start(&p.b, &bm, 0).unwrap();
    run_channel(&mut p, &bm, first, Some(5));
    let (st, held) = p.bob.core.direct_fetch_progress(&p.b, &bm, 0).unwrap();
    assert_eq!(st, FetchState::Receiving);
    assert_eq!(held, 5 * 16_384);
    assert_eq!(
        state_of(&p.alice, &p.a, &p.a_conv),
        AttachmentState::Transferring
    );
    let again = p.bob.core.direct_fetch_start(&p.b, &bm, 0).unwrap();
    run_channel(&mut p, &bm, again, None);
    assert_eq!(
        p.bob.core.direct_fetch_progress(&p.b, &bm, 0).unwrap().0,
        FetchState::Complete
    );
    let opened = p.bob.core.direct_fetch_open(&p.b, &bm, 0).unwrap();
    assert_eq!(opened.plaintext, plain);
    assert_eq!(opened.filename.as_deref(), Some("holiday.mp4"));
    // Local receipt only, said as such, and it survives a restart.
    assert_eq!(
        state_of(&p.alice, &p.a, &p.a_conv),
        AttachmentState::DeliveredLocally
    );
    let ctxs = p.alice.restart();
    assert_eq!(
        state_of(&p.alice, &ctxs[0], &p.a_conv),
        AttachmentState::DeliveredLocally
    );
    let offers: Vec<DirectOfferView> = p.alice.query(Query::GetDirectOffers {
        ctx: ctxs[0].clone(),
    });
    assert_eq!(
        (
            offers.len(),
            offers[0].blob_id.as_str(),
            offers[0].needs_file
        ),
        (1, blob.as_str(), false)
    );
}

#[test]
fn an_offer_survives_a_restart_and_is_served_again_only_from_the_same_file() {
    let mut p = active_pair();
    let plain = file(BIG);
    let blob = offer(&mut p, "d1", "", &plain);
    settle(&mut p);
    let bm = mid(&p.bob, &p.b, &p.b_conv);
    p.a = p.alice.restart().remove(0);
    let offers: Vec<DirectOfferView> = p.alice.query(Query::GetDirectOffers { ctx: p.a.clone() });
    assert_eq!(offers.len(), 1);
    assert_eq!(offers[0].state, AttachmentState::WaitingForRecipient);
    assert!(
        offers[0].needs_file,
        "nothing is served until the file is handed back"
    );
    assert_eq!(offers[0].size, BIG as u64);

    // Asked before it is armed: the one uniform refusal.
    let first = p.bob.core.direct_fetch_start(&p.b, &bm, 0).unwrap();
    let (out, _) = p.alice.core.direct_serve(&p.a, &first).unwrap();
    assert!(matches!(
        out[..],
        [DirectFrame::Error {
            code: ErrorCode::Unavailable,
            ..
        }]
    ));

    // Another file, or the same size with other bytes, is refused; the right one arms it.
    let mut other = plain.clone();
    other[100] ^= 1;
    assert_eq!(
        p.alice.core.arm_direct_offer(&p.a, &blob, &other),
        Err(AttachError::IntegrityFailure)
    );
    assert_eq!(
        p.alice.core.arm_direct_offer(&p.a, &blob, &plain[1..]),
        Err(AttachError::IntegrityFailure)
    );
    p.alice.core.arm_direct_offer(&p.a, &blob, &plain).unwrap();
    let first = p.bob.core.direct_fetch_start(&p.b, &bm, 0).unwrap();
    run_channel(&mut p, &bm, first, None);
    assert_eq!(
        p.bob
            .core
            .direct_fetch_open(&p.b, &bm, 0)
            .unwrap()
            .plaintext,
        plain
    );
}

#[test]
fn withdrawing_or_lapsing_stops_serving_and_says_so() {
    let mut p = active_pair();
    let plain = file(BIG);
    let blob = offer(&mut p, "d1", "", &plain);
    settle(&mut p);
    let bm = mid(&p.bob, &p.b, &p.b_conv);
    let am = mid(&p.alice, &p.a, &p.a_conv);
    p.alice.dispatch(
        &p.a,
        "w1",
        IntentBody::WithdrawDirectOffer { message_id: am },
    );
    assert_eq!(p.alice.outcome(&p.a, "w1"), Some(OutcomeClass::Committed));
    assert_eq!(
        state_of(&p.alice, &p.a, &p.a_conv),
        AttachmentState::NotOffered
    );
    let first = p.bob.core.direct_fetch_start(&p.b, &bm, 0).unwrap();
    let (out, _) = p.alice.core.direct_serve(&p.a, &first).unwrap();
    assert!(matches!(
        out[..],
        [DirectFrame::Error {
            code: ErrorCode::Unavailable,
            ..
        }]
    ));
    assert_eq!(
        p.alice.core.arm_direct_offer(&p.a, &blob, &plain),
        Err(AttachError::NotFound)
    );

    // A second offer lapses after a week without a fetch.
    offer(&mut p, "d2", "", &file(BIG + 1));
    p.alice.tick(8 * 24 * 3600 * 1000);
    p.alice.dispatch(
        &p.a,
        "t",
        IntentBody::SendMessage {
            conversation_id: p.a_conv.clone(),
            draft: Draft { text: "hi".into() },
        },
    );
    let v = view(&p.alice, &p.a, &p.a_conv);
    let last_file = v.iter().rev().find(|i| !parts(i).2.is_empty()).unwrap();
    assert_eq!(parts(last_file).2[0].state, AttachmentState::NotOffered);
}

#[test]
fn a_state_from_before_direct_offers_still_loads() {
    let p = active_pair();
    let mut st = p.alice.core.debug_app_state(&p.a).unwrap();
    assert!(st.as_object_mut().unwrap().remove("directOffers").is_some());
    let app: om_core::state::AppState = serde_json::from_value(st).unwrap();
    assert!(app.direct_offers.is_empty());
}
