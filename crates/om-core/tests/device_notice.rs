// SPDX-License-Identifier: MPL-2.0
//! Owed device-enrollment notices (ADR-0193): a link is approved with a durable notice owed to every other device,
//! delivered whenever a path exists, acknowledged by the device, and shown on the approving device as "not told yet"
//! until then. The strict option keeps the older refusal.

mod common;

use common::confirmation as nonce;
use om_core::api::*;
use om_core::crypto::cbor::{self, V};
use om_core::host::Step;
use om_core::runtime::{Fault, LoopbackNetwork, Runtime, settle_all};

const MIN: u64 = 60_000;

/// One account (Alice's devices, index 0 is the first) and Carol, a contact. Devices can be switched off.
struct Fleet {
    net: LoopbackNetwork,
    rts: Vec<Runtime>,
    ctxs: Vec<ProfileExecutionContext>,
    on: Vec<bool>,
    carol: Runtime,
    c: ProfileExecutionContext,
}

impl Fleet {
    fn new(tag: &str) -> Fleet {
        Fleet::build(tag, true)
    }

    /// An account with no conversation at all: no device has any path to another.
    fn bare(tag: &str) -> Fleet {
        Fleet::build(tag, false)
    }

    fn build(tag: &str, contact: bool) -> Fleet {
        let net = LoopbackNetwork::default();
        let mut alice = Runtime::new(&format!("alice-{tag}"), &net);
        let mut carol = Runtime::new(&format!("carol-{tag}"), &net);
        let a = common::create_profile(&mut alice, "p-a", "Alice");
        let c = common::create_profile(&mut carol, "p-c", "Carol");
        if !contact {
            return Fleet {
                net,
                rts: vec![alice],
                ctxs: vec![a],
                on: vec![true],
                carol,
                c,
            };
        }
        alice.dispatch(&a, "inv", IntentBody::CreateInvite);
        alice.flush_net(&a);
        let link = alice
            .query::<Option<InviteView>>(Query::GetInvite {
                ctx: a.clone(),
                operation_id: "inv".into(),
            })
            .unwrap()
            .link;
        carol.dispatch(&c, "acc", IntentBody::AcceptInvitation { link });
        settle_all(&mut [(&mut alice, &a), (&mut carol, &c)]);
        let intros: Vec<IntroductionRequest> =
            alice.query(Query::GetIntroductions { ctx: a.clone() });
        alice.dispatch(
            &a,
            "i",
            IntentBody::AcceptIntroduction {
                request_id: intros[0].id.clone(),
            },
        );
        settle_all(&mut [(&mut alice, &a), (&mut carol, &c)]);
        Fleet {
            net,
            rts: vec![alice],
            ctxs: vec![a],
            on: vec![true],
            carol,
            c,
        }
    }

    fn settle(&mut self) {
        let mut parties: Vec<(&mut Runtime, &ProfileExecutionContext)> = self
            .rts
            .iter_mut()
            .zip(self.ctxs.iter())
            .zip(self.on.iter())
            .filter(|(_, on)| **on)
            .map(|(p, _)| p)
            .collect();
        parties.push((&mut self.carol, &self.c));
        settle_all(&mut parties);
    }

    fn advance(&mut self, ms: u64) {
        for r in &self.rts {
            r.tick(ms);
        }
        self.carol.tick(ms);
    }

    fn devices(&self, i: usize) -> DevicesView {
        self.rts[i].query(Query::GetDevices {
            ctx: self.ctxs[i].clone(),
        })
    }

    /// Device `by` offers, a new device asks, `by` approves. Returns the new index and the approval's outcome.
    fn link(&mut self, by: usize, name: &str) -> (usize, Option<OutcomeClass>) {
        self.link_with(by, name, None)
    }

    /// [`Fleet::link`], with `fault` armed for the first network call after the approval is decided (the approval's
    /// own deposit).
    fn link_with(
        &mut self,
        by: usize,
        name: &str,
        fault: Option<Fault>,
    ) -> (usize, Option<OutcomeClass>) {
        let mut rt = Runtime::new(&format!("{name}-dn"), &self.net);
        let ctx = rt.open(&format!("p-{name}"));
        let offer_op = format!("offer-{name}");
        let (r, c) = (&mut self.rts[by], &self.ctxs[by]);
        r.dispatch(c, &offer_op, IntentBody::OfferDeviceLink);
        r.flush_net(c);
        let link = r
            .query::<Option<LinkOfferView>>(Query::GetLinkOffer {
                ctx: c.clone(),
                operation_id: offer_op,
            })
            .unwrap()
            .link;
        rt.dispatch(&ctx, "ask", IntentBody::RequestDeviceLink { link });
        self.rts.push(rt);
        self.ctxs.push(ctx);
        self.on.push(true);
        let new = self.rts.len() - 1;
        self.settle();
        let pending = self.devices(by).link_requests;
        assert_eq!(pending.len(), 1, "one request waits for {name}");
        let request_id = pending[0].request_id.clone();
        let confirmation = nonce(
            &mut self.rts[by],
            &self.ctxs[by],
            "ApproveDeviceLink",
            &request_id,
        );
        let op = format!("approve-{name}");
        if let Some(f) = fault {
            self.net.inject(f);
        }
        self.rts[by].dispatch(
            &self.ctxs[by],
            &op,
            IntentBody::ApproveDeviceLink {
                request_id,
                confirmation,
            },
        );
        let out = self.rts[by].outcome(&self.ctxs[by], &op);
        self.settle();
        (new, out)
    }

    fn device_id(&self, i: usize) -> String {
        self.devices(i)
            .devices
            .iter()
            .find(|d| d.is_current)
            .unwrap()
            .id
            .clone()
    }

    /// What device `viewer` shows for device `of`.
    fn state_of(&self, viewer: usize, of: usize) -> (DeviceNoticeState, Option<u64>) {
        let id = self.device_id(of);
        let d = self.devices(viewer);
        let s = d.devices.iter().find(|d| d.id == id).unwrap();
        (s.notice_state, s.last_attempt_at)
    }

    /// The first device says hello in the conversation with Carol, so every device pair has a session.
    fn talk(&mut self) {
        let l: Vec<ConversationSummary> = self.rts[0].query(Query::GetConversationList {
            ctx: self.ctxs[0].clone(),
        });
        let conversation_id = l[0].id.clone();
        self.rts[0].dispatch(
            &self.ctxs[0],
            "hi",
            IntentBody::SendMessage {
                conversation_id,
                draft: Draft {
                    text: "hi".into(),
                    reply_to: None,
                },
            },
        );
        self.settle();
        self.settle();
    }

    fn owed(&self, i: usize) -> Vec<(String, String, u64)> {
        let st = self.rts[i].core.debug_app_state(&self.ctxs[i]).unwrap();
        st["owedNotices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| {
                (
                    o["siblingDeviceId"].as_str().unwrap()[..4].to_string(),
                    o["state"].as_str().unwrap().to_string(),
                    o["attempts"].as_u64().unwrap(),
                )
            })
            .collect()
    }
}

#[test]
fn a_sibling_that_cannot_be_reached_is_owed_the_notice_then_told_and_clears() {
    let mut f = Fleet::new("a");
    let (phone, out) = f.link(0, "phone");
    assert_eq!(out, Some(OutcomeClass::Committed));
    // Switch the phone off before the tablet is linked: no session carries anything to it yet.
    f.on[phone] = false;
    let (tablet, out) = f.link(0, "tablet");
    assert_eq!(
        out,
        Some(OutcomeClass::Committed),
        "the approval does not wait for a path"
    );
    assert_eq!(
        f.devices(0).devices.len(),
        3,
        "the tablet is listed at once"
    );
    let (state, _) = f.state_of(0, phone);
    assert_eq!(state, DeviceNoticeState::Owed, "the phone is not told yet");
    assert_eq!(f.state_of(0, tablet).0, DeviceNoticeState::Told);
    assert!(
        f.devices(0)
            .devices
            .iter()
            .find(|d| d.id == f.device_id(phone))
            .unwrap()
            .actions
            .contains(&DeviceActionCode::RetryNotice)
    );
    // The phone comes back and the two devices speak: the notice reaches it and it answers.
    f.on[phone] = true;
    f.advance(2 * MIN);
    f.settle();
    f.settle();
    let on_phone = f.devices(phone);
    assert_eq!(
        on_phone.devices.len(),
        3,
        "the phone's head lists the tablet"
    );
    assert_eq!(on_phone.notices.len(), 1);
    assert_eq!(
        f.state_of(0, phone).0,
        DeviceNoticeState::Told,
        "acknowledged"
    );
    let st = f.rts[0].core.debug_app_state(&f.ctxs[0]).unwrap();
    assert!(st["owedNotices"].as_array().unwrap().is_empty());
}

#[test]
fn an_offline_sibling_is_re_sent_on_the_schedule_and_never_cleared_without_an_answer() {
    let mut f = Fleet::new("b");
    let (phone, _) = f.link(0, "phone");
    f.on[phone] = false;
    let (_tablet, out) = f.link(0, "tablet");
    assert_eq!(out, Some(OutcomeClass::Committed));
    assert_eq!(f.state_of(0, phone).0, DeviceNoticeState::Owed);
    let owed_at = |f: &Fleet| -> (u64, String, u64) {
        let st = f.rts[0].core.debug_app_state(&f.ctxs[0]).unwrap();
        let o = &st["owedNotices"][0];
        (
            o["attempts"].as_u64().unwrap(),
            o["state"].as_str().unwrap().to_string(),
            o["lastAttemptAt"].as_u64().unwrap(),
        )
    };
    let (a0, s0, t0) = owed_at(&f);
    // Not yet due: nothing is sent on a pump.
    f.advance(5 * MIN);
    f.settle();
    assert_eq!(owed_at(&f).0, a0);
    // 15 minutes after the first send: sent again.
    f.advance(11 * MIN);
    f.settle();
    let (a1, s1, t1) = owed_at(&f);
    assert!(a1 > a0 || s0 != s1, "a second attempt was made");
    assert!(t1 > t0);
    // 1 hour is the next gap, then 6, then 24: nothing between.
    f.advance(30 * MIN);
    f.settle();
    assert_eq!(owed_at(&f).0, a1, "not due inside the hour");
    f.advance(31 * MIN);
    f.settle();
    let (a2, _, _) = owed_at(&f);
    assert!(a2 > a1, "due after an hour");
    // Still listed after a week without an answer: nothing is dropped for age.
    f.advance(7 * 24 * 60 * MIN);
    f.settle();
    assert_eq!(f.state_of(0, phone).0, DeviceNoticeState::Owed);
    // Told on a device that never answered is not a thing: the person may remove it.
    let phone_id = f.device_id(phone);
    let conf = nonce(&mut f.rts[0], &f.ctxs[0], "RevokeDevice", &phone_id);
    f.rts[0].dispatch(
        &f.ctxs[0],
        "rm",
        IntentBody::RevokeDevice {
            device_id: phone_id.clone(),
            confirmation: conf,
        },
    );
    f.settle();
    f.rts[0].dispatch(
        &f.ctxs[0],
        "again",
        IntentBody::RetryDeviceNotice {
            device_id: phone_id,
        },
    );
    assert!(matches!(
        f.rts[0].outcome(&f.ctxs[0], "again"),
        Some(OutcomeClass::Rejected { .. })
    ));
    let st = f.rts[0].core.debug_app_state(&f.ctxs[0]).unwrap();
    assert!(
        st["owedNotices"].as_array().unwrap().is_empty(),
        "removing the device drops what was owed to it"
    );
}

#[test]
fn two_siblings_one_answers_one_does_not() {
    let mut f = Fleet::new("c");
    let (phone, _) = f.link(0, "phone");
    let (tablet, _) = f.link(0, "tablet");
    f.talk();
    f.on[tablet] = false;
    let (laptop, out) = f.link(0, "laptop");
    assert_eq!(out, Some(OutcomeClass::Committed));
    f.advance(2 * MIN);
    f.settle();
    f.settle();
    assert_eq!(
        f.state_of(0, phone).0,
        DeviceNoticeState::Told,
        "the phone answered"
    );
    assert_eq!(
        f.state_of(0, tablet).0,
        DeviceNoticeState::Owed,
        "the tablet did not"
    );
    assert_eq!(f.state_of(0, laptop).0, DeviceNoticeState::Told);
    // It survives a restart of the approving device and still finishes afterwards.
    f.rts[0].restart();
    f.ctxs[0] = f.rts[0].core.context_for("p-a").unwrap();
    assert_eq!(f.state_of(0, tablet).0, DeviceNoticeState::Owed);
    f.on[tablet] = true;
    f.advance(2 * MIN);
    f.settle();
    f.settle();
    f.advance(2 * MIN);
    f.settle();
    f.settle();
    assert_eq!(
        f.devices(tablet).devices.len(),
        4,
        "the tablet learned of the laptop"
    );
    assert_eq!(f.devices(tablet).notices.len(), 1);
}

#[test]
fn a_new_device_carries_the_notice_when_the_approving_device_is_gone() {
    let mut f = Fleet::new("d");
    let (phone, _) = f.link(0, "phone");
    f.talk();
    f.on[phone] = false;
    let (laptop, out) = f.link(0, "laptop");
    assert_eq!(out, Some(OutcomeClass::Committed));
    // The approving device is lost; only the laptop and the phone remain.
    f.on[0] = false;
    f.on[phone] = true;
    f.advance(2 * MIN);
    for _ in 0..3 {
        f.advance(2 * MIN);
        f.settle();
    }
    let on_phone = f.devices(phone);
    assert_eq!(
        on_phone.devices.len(),
        3,
        "the phone learned of the laptop from the laptop"
    );
    assert_eq!(on_phone.notices.len(), 1);
    // The laptop and the phone have no return route to each other yet (the sibling mesh is not built, D24), so the
    // laptop keeps listing the phone as not told rather than assuming it.
    assert_eq!(f.state_of(laptop, phone).0, DeviceNoticeState::Owed);
}

#[test]
fn a_sibling_many_enrollments_behind_catches_up_in_order() {
    let mut f = Fleet::new("e");
    let (phone, _) = f.link(0, "phone");
    f.talk();
    f.on[phone] = false;
    for n in 0..6 {
        let (_, out) = f.link(0, &format!("extra{n}"));
        assert_eq!(out, Some(OutcomeClass::Committed));
        // keep the extra devices from answering for the phone; they are not under test
    }
    assert_eq!(f.state_of(0, phone).0, DeviceNoticeState::Owed);
    f.on[phone] = true;
    for _ in 0..6 {
        f.advance(2 * MIN);
        f.settle();
    }
    let on_phone = f.devices(phone);
    assert_eq!(
        on_phone.devices.len(),
        8,
        "six enrollments behind, all applied"
    );
    assert_eq!(on_phone.notices.len(), 6);
    assert_eq!(f.state_of(0, phone).0, DeviceNoticeState::Told);
}

#[test]
fn more_than_the_bound_owed_refuses_with_quota_and_changes_nothing() {
    let mut f = Fleet::new("f");
    let mut refused = None;
    for n in 0..12 {
        // Each new device completes its own link and then is switched off, so nothing it is owed is ever answered.
        let before = f.devices(0).devices.len();
        let (new, out) = f.link(0, &format!("d{n}"));
        if out != Some(OutcomeClass::Committed) {
            refused = Some((out, before));
            break;
        }
        f.on[new] = false;
    }
    let (out, before) = refused.expect("the bound is reached");
    assert_eq!(
        out,
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Quota
        })
    );
    assert_eq!(f.devices(0).devices.len(), before, "nothing was changed");
}

#[test]
fn an_account_too_large_for_one_message_links_through_continuation_chunks() {
    let mut f = Fleet::bare("big");
    // A long chain of real signed states (one device per state), well past what one 100 kB message carries.
    for _ in 0..30 {
        let (_, step) = f.rts[0]
            .core
            .debug_link_messaging_device(&f.ctxs[0])
            .unwrap();
        f.rts[0].settle(&step);
    }
    let (new, out) = f.link(0, "late");
    assert_eq!(
        out,
        Some(OutcomeClass::Committed),
        "the approval is sent as a manifest and chunks (ADR-0195)"
    );
    f.settle();
    let st = f.rts[new].core.debug_app_state(&f.ctxs[new]).unwrap();
    assert_eq!(
        st["acsChain"].as_array().unwrap().len(),
        32,
        "the new device holds the whole chain it was sent in pieces"
    );
    assert_eq!(f.devices(new).devices.len(), f.devices(0).devices.len());
    assert!(
        f.net
            .all_deposited_bytes()
            .iter()
            .filter(|b| b.len() > 60_000)
            .count()
            >= 3,
        "the account travelled in several sealed messages"
    );
}

#[test]
fn an_approval_the_home_refuses_for_good_leaves_a_device_shown_as_not_set_up_that_can_be_removed() {
    let mut f = Fleet::bare("undelivered");
    let known: Vec<String> = f.devices(0).devices.iter().map(|d| d.id.clone()).collect();
    let (_, out) = f.link_with(0, "phone", Some(Fault::Refused));
    assert_eq!(
        out,
        Some(OutcomeClass::Committed),
        "the approval itself committed"
    );
    f.settle();
    let view = f.devices(0);
    let d = view
        .devices
        .iter()
        .find(|d| !known.contains(&d.id))
        .expect("listed on the approving device");
    assert_eq!(
        d.notice_state,
        DeviceNoticeState::NotSetUp,
        "never received what it needs"
    );
    assert!(
        d.actions.contains(&DeviceActionCode::Revoke),
        "it can be removed"
    );
    // A device whose approval was delivered is not marked.
    let (ok, _) = f.link(0, "tablet");
    f.settle();
    assert_ne!(f.state_of(0, ok).0, DeviceNoticeState::NotSetUp);
}

#[test]
fn a_sibling_with_no_path_is_owed_by_default_and_refused_when_strict() {
    // Default: the approval does not wait. The phone has no conversation to be reached through, so it is owed.
    let mut f = Fleet::bare("g1");
    let (phone, _) = f.link(0, "phone");
    let (_tablet, out) = f.link(0, "tablet");
    assert_eq!(out, Some(OutcomeClass::Committed));
    assert_eq!(f.state_of(0, phone).0, DeviceNoticeState::Owed);
    assert_eq!(f.owed(0)[0].1, "waiting");

    // Strict: the same situation is refused and nothing changes.
    let mut f = Fleet::bare("g2");
    let (_phone, _) = f.link(0, "phone");
    f.rts[0].dispatch(
        &f.ctxs[0],
        "strict",
        IntentBody::SetStrictDeviceNotice { enabled: true },
    );
    assert!(f.devices(0).strict_notice);
    let (_tablet, out) = f.link(0, "tablet");
    assert_eq!(
        out,
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Unavailable
        })
    );
    assert_eq!(f.devices(0).devices.len(), 2, "nothing changed");
    assert!(
        f.owed(0).is_empty(),
        "no notice is owed for a link that did not happen"
    );
    // Turning it off again restores the default.
    f.rts[0].dispatch(
        &f.ctxs[0],
        "lax",
        IntentBody::SetStrictDeviceNotice { enabled: false },
    );
    assert!(!f.devices(0).strict_notice);
}

fn ack_payload(digest: &[u8]) -> Vec<u8> {
    cbor::encode(&V::M(vec![(0, V::U(13)), (1, V::B(digest.to_vec()))]))
}

#[test]
fn an_acknowledgement_for_a_digest_that_is_not_owed_to_the_sender_is_ignored() {
    let mut f = Fleet::new("e");
    let (phone, _) = f.link(0, "phone");
    let (tablet, _) = f.link(0, "tablet");
    f.talk();
    // The tablet goes dark, then a laptop is linked: the notice for the laptop is owed to the tablet, and the phone (which
    // has a session with the approving device) is a different sender.
    f.on[tablet] = false;
    let (_laptop, out) = f.link(0, "laptop");
    assert_eq!(out, Some(OutcomeClass::Committed));
    assert_eq!(f.state_of(0, tablet).0, DeviceNoticeState::Owed);
    assert_eq!(f.state_of(0, phone).0, DeviceNoticeState::Told);
    let before = f.owed(0);
    assert_eq!(before.len(), 1, "one notice is owed, to the tablet");
    let st = f.rts[0].core.debug_app_state(&f.ctxs[0]).unwrap();
    let owed_digest = st["owedNotices"][0]["noticeDigest"]
        .as_str()
        .unwrap()
        .to_string();
    let first = f.device_id(0);
    // The phone answers with a digest nobody owes it, then with the digest that is owed to the tablet: neither may clear
    // anything, and neither is an error.
    let owed_to_tablet: Vec<u8> = (0..owed_digest.len() / 2)
        .map(|i| u8::from_str_radix(&owed_digest[2 * i..2 * i + 2], 16).unwrap())
        .collect();
    for (n, payload) in [ack_payload(&[7u8; 32]), ack_payload(&owed_to_tablet)]
        .iter()
        .enumerate()
    {
        let (sent, step) = f.rts[phone]
            .core
            .debug_send_to_sibling(&f.ctxs[phone], &first, payload)
            .unwrap();
        f.rts[phone].settle(&step);
        assert!(sent, "the phone's acknowledgement {n} went out");
        f.settle();
        f.settle();
        assert_eq!(f.owed(0), before, "acknowledgement {n} changed nothing");
        assert_eq!(f.state_of(0, tablet).0, DeviceNoticeState::Owed);
    }
}

/// Hand device `i` only the mailbox items `pick` selects (by position among those waiting, or by `all` for every one
/// from sequence 0), so a test can deliver traffic in an order the relay would not.
fn deliver_only(f: &mut Fleet, i: usize, from_start: bool, pick: &dyn Fn(usize) -> bool) -> usize {
    let c = f.ctxs[i].clone();
    let mut n = 0;
    for t in f.rts[i].core.poll_targets(&c).unwrap() {
        assert!(
            t.capability.is_empty(),
            "this helper reads the device's own mailbox only"
        );
        let after = if from_start { 0 } else { t.after };
        let items: Vec<_> = f
            .net
            .poll_at(t.relay_uri.as_deref(), &t.mailbox, after)
            .into_iter()
            .enumerate()
            .filter(|(k, _)| pick(*k))
            .map(|(_, x)| x)
            .collect();
        if items.is_empty() {
            continue;
        }
        n += items.len();
        let step = f.rts[i].core.inbound(&c, &t.mailbox, &items).unwrap();
        f.rts[i].settle(&step);
    }
    n
}

#[test]
fn a_sibling_given_the_new_devices_traffic_before_the_notice_holds_it_and_does_not_trust_the_device()
 {
    // ADR-0193 decision 7 (a sibling without the verified notice does not treat the new device as authorized), with
    // ADR-0120 K5 and K12: an envelope from a device the held head does not list is held until the head is refreshed;
    // nothing in it is applied and the sibling's head and notices do not change.
    let mut f = Fleet::new("k");
    let (tablet, _) = f.link(0, "tablet");
    f.on[tablet] = false;
    let (laptop, out) = f.link(0, "laptop");
    assert_eq!(out, Some(OutcomeClass::Committed));
    assert_eq!(
        f.state_of(0, tablet).0,
        DeviceNoticeState::Owed,
        "the tablet has not been told"
    );
    let tablet_id = f.device_id(tablet);
    // The laptop speaks to the tablet first: a payload that a trusted sibling would act on (an acknowledgement).
    let ack = ack_payload(&[9u8; 32]);
    let (sent, step) = f.rts[laptop]
        .core
        .debug_send_to_sibling(&f.ctxs[laptop], &tablet_id, &ack)
        .unwrap();
    f.rts[laptop].settle(&step);
    assert!(sent, "the laptop's traffic reaches the tablet's mailbox");
    f.on[tablet] = true;
    f.rts[laptop].flush_net(&f.ctxs[laptop].clone());
    let before = f.devices(tablet);
    assert_eq!(
        before.devices.len(),
        2,
        "the tablet's head does not list the laptop"
    );
    assert!(before.notices.is_empty());
    // Only the laptop's item is handed over (the second item waiting), so no notice can have arrived first.
    assert_eq!(deliver_only(&mut f, tablet, false, &|k| k == 1), 1);
    let after = f.devices(tablet);
    assert_eq!(
        after.devices.len(),
        2,
        "still not listed: nothing from the laptop made it authorized"
    );
    assert!(after.notices.is_empty(), "and no notice appeared");
    let st = f.rts[tablet].core.debug_app_state(&f.ctxs[tablet]).unwrap();
    let held: usize = st["relationships"]
        .as_object()
        .unwrap()
        .values()
        .map(|r| r["held"].as_array().map_or(0, Vec::len))
        .sum();
    assert_eq!(
        held, 1,
        "the envelope is held until the head is refreshed, not applied"
    );
}

/// A restore brings history and nothing that acts (ADR-0184, ADR-0193): an owed notice, a device list and a path to any
/// sibling are not in a backup, so a restored profile owes nobody anything and cannot send on the old account's behalf.
#[test]
fn a_restored_backup_owes_no_notice_and_acts_on_nothing_pending() {
    let mut f = Fleet::new("r");
    let (phone, _) = f.link(0, "phone");
    f.talk();
    f.on[phone] = false;
    let (_tablet, out) = f.link(0, "tablet");
    assert_eq!(out, Some(OutcomeClass::Committed));
    assert_eq!(f.owed(0).len(), 1, "a notice is owed to the phone");
    let plan = f.rts[0].core.backup_plan();
    let key = [0x42u8; 32];
    let file = f.rts[0]
        .core
        .export_history(&f.ctxs[0], &plan, &key)
        .unwrap();
    let mut fresh = Runtime::new("restored-dn", &f.net);
    let ctx = common::create_profile(&mut fresh, "p-restored", "Alice again");
    let (_, commit) = fresh.core.import_history(&ctx, &file, &key).unwrap();
    assert!(fresh.settle(&Step {
        commit,
        handle: None
    }));
    let st = fresh.core.debug_app_state(&ctx).unwrap();
    assert!(
        st["owedNotices"].as_array().unwrap().is_empty(),
        "nothing is owed after a restore"
    );
    assert!(st["owedAcks"].as_array().is_none_or(|a| a.is_empty()));
    assert!(st["relationships"].as_object().unwrap().is_empty());
    let d: DevicesView = fresh.query(Query::GetDevices { ctx: ctx.clone() });
    assert_eq!(
        d.devices.len(),
        1,
        "only this device; the old account's devices did not come back"
    );
    assert!(
        d.devices
            .iter()
            .all(|x| x.notice_state == DeviceNoticeState::Told)
    );
    // Time passes and the retry driver runs: nothing is sent to anyone.
    for _ in 0..3 {
        fresh.tick(2 * 24 * 60 * MIN);
        let tick = fresh.core.notice_tick(&ctx).unwrap();
        fresh.settle(&tick);
    }
    assert!(
        fresh
            .core
            .net_ops(&ctx)
            .unwrap()
            .iter()
            .all(|o| !format!("{:?}", o.call).contains("Deposit"))
    );
    assert!(fresh.core.poll_targets(&ctx).unwrap().is_empty());
}
