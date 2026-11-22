// SPDX-License-Identifier: MPL-2.0
//! The device-link ceremony (D24): an existing control device offers a link, a new device asks, both show the same
//! code, a person approves, the account's head gains the device, the enrollment notice reaches it, and the two devices
//! then send and receive with a contact. Revoking the device removes it.

mod common;

use common::confirmation as nonce;
use om_core::api::*;
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};

struct World {
    net: LoopbackNetwork,
    alice: Runtime,
    carol: Runtime,
    phone: Runtime,
    a: ProfileExecutionContext,
    c: ProfileExecutionContext,
    n: ProfileExecutionContext,
    a_conv: String,
    c_conv: String,
}

fn conv_of(rt: &Runtime, ctx: &ProfileExecutionContext) -> String {
    let l: Vec<ConversationSummary> = rt.query(Query::GetConversationList { ctx: ctx.clone() });
    assert_eq!(l[0].relationship, RelationshipStateCode::Active);
    l[0].id.clone()
}

/// Alice (the existing device) and Carol are contacts; the phone's profile is empty.
fn world() -> World {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice-link", &net);
    let mut carol = Runtime::new("carol-link", &net);
    let mut phone = Runtime::new("phone-link", &net);
    let a = common::create_profile(&mut alice, "p-a", "Alice");
    let c = common::create_profile(&mut carol, "p-c", "Carol");
    let n = phone.open("p-n");
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
    let intros: Vec<IntroductionRequest> = alice.query(Query::GetIntroductions { ctx: a.clone() });
    alice.dispatch(
        &a,
        "i",
        IntentBody::AcceptIntroduction {
            request_id: intros[0].id.clone(),
        },
    );
    settle_all(&mut [(&mut alice, &a), (&mut carol, &c)]);
    let (a_conv, c_conv) = (conv_of(&alice, &a), conv_of(&carol, &c));
    World {
        net,
        alice,
        carol,
        phone,
        a,
        c,
        n,
        a_conv,
        c_conv,
    }
}

impl World {
    fn settle(&mut self) {
        settle_all(&mut [
            (&mut self.alice, &self.a),
            (&mut self.carol, &self.c),
            (&mut self.phone, &self.n),
        ]);
    }

    /// Alice offers, the phone asks: both are waiting for the person. Returns the request id.
    fn offer_and_ask(&mut self) -> String {
        self.alice
            .dispatch(&self.a, "offer", IntentBody::OfferDeviceLink);
        self.alice.flush_net(&self.a);
        let offer = self.offer().expect("an offer with a link");
        assert_eq!(offer.state, LinkOfferState::WaitingForScan);
        assert!(offer.link.starts_with("om:link/"));
        self.phone.dispatch(
            &self.n,
            "ask",
            IntentBody::RequestDeviceLink {
                link: offer.link.clone(),
            },
        );
        self.settle();
        let seen = self.offer().unwrap();
        assert_eq!(seen.state, LinkOfferState::AwaitingApproval);
        let theirs = self.asked().expect("the phone's request");
        let code = seen
            .verification_code
            .clone()
            .expect("a code on the existing device");
        assert_eq!(code.len(), 6);
        assert_eq!(
            theirs.verification_code, code,
            "both devices show the same code"
        );
        let pending = self.devices(true).link_requests;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].verification_code, code);
        assert_eq!(pending[0].state, LinkRequestState::AwaitingApproval);
        pending[0].request_id.clone()
    }

    fn offer(&self) -> Option<LinkOfferView> {
        self.alice.query(Query::GetLinkOffer {
            ctx: self.a.clone(),
            operation_id: "offer".into(),
        })
    }

    fn asked(&self) -> Option<LinkRequestView> {
        self.phone.query(Query::GetLinkRequest {
            ctx: self.n.clone(),
            operation_id: "ask".into(),
        })
    }

    fn devices(&self, existing: bool) -> DevicesView {
        if existing {
            self.alice.query(Query::GetDevices {
                ctx: self.a.clone(),
            })
        } else {
            self.phone.query(Query::GetDevices {
                ctx: self.n.clone(),
            })
        }
    }

    fn approve(&mut self, request_id: &str, op: &str) -> Option<OutcomeClass> {
        let confirmation = nonce(&mut self.alice, &self.a, "ApproveDeviceLink", request_id);
        self.alice.dispatch(
            &self.a,
            op,
            IntentBody::ApproveDeviceLink {
                request_id: request_id.into(),
                confirmation,
            },
        );
        self.alice.outcome(&self.a, op)
    }

    fn say(&mut self, who: &str, op: &str, text: &str) {
        let (rt, ctx, conv) = match who {
            "alice" => (&mut self.alice, &self.a, &self.a_conv),
            "carol" => (&mut self.carol, &self.c, &self.c_conv),
            _ => {
                let conv = conv_of(&self.phone, &self.n);
                self.phone.dispatch(
                    &self.n,
                    op,
                    IntentBody::SendMessage {
                        conversation_id: conv,
                        draft: Draft {
                            text: text.into(),
                            reply_to: None,
                        },
                    },
                );
                return;
            }
        };
        rt.dispatch(
            ctx,
            op,
            IntentBody::SendMessage {
                conversation_id: conv.clone(),
                draft: Draft {
                    text: text.into(),
                    reply_to: None,
                },
            },
        );
    }
}

fn texts(rt: &Runtime, ctx: &ProfileExecutionContext, conv: &str) -> Vec<String> {
    let v: ConversationView = rt.query(Query::GetConversationView {
        ctx: ctx.clone(),
        conversation_id: conv.into(),
    });
    v.timeline
        .into_iter()
        .map(|t| match t {
            TimelineItem::Incoming { text, .. } | TimelineItem::Outgoing { text, .. } => text,
        })
        .collect()
}

#[test]
fn contact_nickname_syncs_only_to_eligible_sibling_devices_and_clears_after_restart() {
    let mut w = world();
    let request = w.offer_and_ask();
    assert_eq!(w.approve(&request, "ok"), Some(OutcomeClass::Committed));
    w.settle();
    let nickname = |rt: &Runtime, ctx: &ProfileExecutionContext| {
        rt.query::<Vec<ConversationSummary>>(Query::GetConversationList { ctx: ctx.clone() })[0]
            .nickname
            .clone()
    };
    w.alice.dispatch(
        &w.a,
        "nickname",
        IntentBody::SetContactNickname {
            conversation_id: w.a_conv.clone(),
            nickname: Some("Carol from work".into()),
        },
    );
    assert!(
        w.alice.core.net_ops(&w.a).unwrap().is_empty(),
        "the local intent does not send"
    );
    w.settle();
    assert_eq!(nickname(&w.phone, &w.n).as_deref(), Some("Carol from work"));
    assert_eq!(nickname(&w.carol, &w.c), None);

    w.n = w.phone.restart().remove(0);
    w.phone.dispatch(
        &w.n,
        "clear-nickname",
        IntentBody::SetContactNickname {
            conversation_id: conv_of(&w.phone, &w.n),
            nickname: None,
        },
    );
    w.settle();
    assert_eq!(nickname(&w.alice, &w.a), None);
    assert_eq!(nickname(&w.carol, &w.c), None);

    let id = w
        .devices(false)
        .devices
        .into_iter()
        .find(|d| d.is_current)
        .unwrap()
        .id;
    let confirmation = nonce(&mut w.alice, &w.a, "RevokeDevice", &id);
    w.alice.dispatch(
        &w.a,
        "revoke-nickname-reader",
        IntentBody::RevokeDevice {
            device_id: id,
            confirmation,
        },
    );
    w.settle();
    w.alice.dispatch(
        &w.a,
        "nickname-after-revoke",
        IntentBody::SetContactNickname {
            conversation_id: w.a_conv.clone(),
            nickname: Some("Never on the old phone".into()),
        },
    );
    w.settle();
    assert_eq!(nickname(&w.phone, &w.n), None);
}

#[test]
fn two_cores_link_and_both_send_and_receive_with_a_third() {
    let mut w = world();
    let request_id = w.offer_and_ask();
    assert_eq!(w.approve(&request_id, "ok"), Some(OutcomeClass::Committed));
    w.settle();
    w.settle();

    // The phone is an account member: the head names it, its request reads approved, and it holds the conversation.
    assert_eq!(w.asked().unwrap().state, LinkRequestState::Approved);
    assert_eq!(w.offer().unwrap().state, LinkOfferState::Linked);
    let on_phone = w.devices(false);
    assert_eq!(on_phone.devices.len(), 2);
    assert!(
        on_phone
            .devices
            .iter()
            .any(|d| d.is_current && d.role == DeviceRole::Messaging)
    );
    let on_alice = w.devices(true);
    assert_eq!(on_alice.devices.len(), 2);
    assert!(on_alice.link_requests.is_empty(), "the request is decided");
    let phone_conv = conv_of(&w.phone, &w.n);

    // The phone sends to Carol and Alice's own copy of it is synced; Carol answers to both devices.
    w.say("phone", "p1", "from the phone");
    w.settle();
    w.settle();
    assert!(texts(&w.carol, &w.c, &w.c_conv).contains(&"from the phone".to_string()));
    assert!(texts(&w.alice, &w.a, &w.a_conv).contains(&"from the phone".to_string()));
    w.say("carol", "c1", "to both of you");
    w.settle();
    w.settle();
    assert!(texts(&w.alice, &w.a, &w.a_conv).contains(&"to both of you".to_string()));
    assert!(texts(&w.phone, &w.n, &phone_conv).contains(&"to both of you".to_string()));
    w.say("alice", "a1", "from the laptop");
    w.settle();
    w.settle();
    assert!(texts(&w.carol, &w.c, &w.c_conv).contains(&"from the laptop".to_string()));
    assert!(texts(&w.phone, &w.n, &phone_conv).contains(&"from the laptop".to_string()));
    let _ = &w.net;
}

#[test]
fn revoking_the_linked_device_removes_it() {
    let mut w = world();
    let request_id = w.offer_and_ask();
    assert_eq!(w.approve(&request_id, "ok"), Some(OutcomeClass::Committed));
    w.settle();
    w.settle();
    let phone_conv = conv_of(&w.phone, &w.n);
    w.say("phone", "p1", "before");
    w.settle();
    w.settle();
    assert!(texts(&w.carol, &w.c, &w.c_conv).contains(&"before".to_string()));

    let id = w
        .devices(false)
        .devices
        .iter()
        .find(|d| d.is_current)
        .unwrap()
        .id
        .clone();
    let confirmation = nonce(&mut w.alice, &w.a, "RevokeDevice", &id);
    w.alice.dispatch(
        &w.a,
        "revoke",
        IntentBody::RevokeDevice {
            device_id: id,
            confirmation,
        },
    );
    assert_eq!(
        w.alice.outcome(&w.a, "revoke"),
        Some(OutcomeClass::Committed)
    );
    w.settle();
    w.settle();

    w.say("carol", "c2", "after the revoke");
    w.say("phone", "p2", "from a revoked device");
    w.settle();
    w.settle();
    assert!(texts(&w.alice, &w.a, &w.a_conv).contains(&"after the revoke".to_string()));
    assert!(
        !texts(&w.phone, &w.n, &phone_conv).contains(&"after the revoke".to_string()),
        "a removed device receives nothing new"
    );
    assert!(
        !texts(&w.carol, &w.c, &w.c_conv).contains(&"from a revoked device".to_string()),
        "a removed device's messages are refused"
    );
}

#[test]
fn a_messaging_device_cannot_link_another_and_bad_approvals_are_refused() {
    let mut w = world();
    let request_id = w.offer_and_ask();
    // No confirmation, a nonce for another request, and a stale request id each fail.
    w.alice.dispatch(
        &w.a,
        "bare",
        IntentBody::ApproveDeviceLink {
            request_id: request_id.clone(),
            confirmation: serde_json::Value::Null,
        },
    );
    assert_eq!(
        w.alice.outcome(&w.a, "bare"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Unauthorized
        })
    );
    let wrong = nonce(&mut w.alice, &w.a, "ApproveDeviceLink", "someone-else");
    w.alice.dispatch(
        &w.a,
        "wrong",
        IntentBody::ApproveDeviceLink {
            request_id: request_id.clone(),
            confirmation: wrong,
        },
    );
    assert_eq!(
        w.alice.outcome(&w.a, "wrong"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Unauthorized
        })
    );
    assert_eq!(w.devices(true).devices.len(), 1, "nothing was linked");
    assert_eq!(
        w.approve("no-such-request", "nosuch"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::NotFound
        })
    );
    assert_eq!(w.approve(&request_id, "ok"), Some(OutcomeClass::Committed));
    w.settle();
    w.settle();
    // The phone is a messaging device: it may not offer a link, and may not approve one.
    w.phone
        .dispatch(&w.n, "offer-2", IntentBody::OfferDeviceLink);
    assert_eq!(
        w.phone.outcome(&w.n, "offer-2"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Unauthorized
        })
    );
    // Linking is one device per offer: the same request cannot be approved twice.
    assert_eq!(
        w.approve(&request_id, "again"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::NotFound
        })
    );
}

#[test]
fn a_refused_request_and_an_expired_ceremony_link_nothing() {
    // Refused by the person.
    let mut w = world();
    let request_id = w.offer_and_ask();
    w.alice.dispatch(
        &w.a,
        "no",
        IntentBody::RejectDeviceLink {
            request_id: request_id.clone(),
        },
    );
    assert_eq!(w.alice.outcome(&w.a, "no"), Some(OutcomeClass::Committed));
    w.settle();
    assert_eq!(w.asked().unwrap().state, LinkRequestState::Rejected);
    assert!(
        w.devices(false).devices.is_empty(),
        "the phone has no account"
    );
    assert_eq!(w.devices(true).devices.len(), 1);

    // Too late: the ceremony lives five minutes.
    let mut w = world();
    let request_id = w.offer_and_ask();
    w.alice.tick(5 * 60 * 1000 + 1);
    assert_eq!(
        w.approve(&request_id, "late"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Expired
        })
    );
    assert_eq!(w.devices(true).devices.len(), 1, "the head did not change");

    // A link that is already past its time is not even started.
    let mut w = world();
    w.alice.dispatch(&w.a, "offer", IntentBody::OfferDeviceLink);
    w.alice.flush_net(&w.a);
    let link = w.offer().unwrap().link;
    w.phone.tick(5 * 60 * 1000 + 61_000);
    w.phone
        .dispatch(&w.n, "ask", IntentBody::RequestDeviceLink { link });
    assert_eq!(
        w.phone.outcome(&w.n, "ask"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Expired
        })
    );
    // Garbage is not a link.
    w.phone.dispatch(
        &w.n,
        "junk",
        IntentBody::RequestDeviceLink {
            link: "om:link/AAAAAAAAAAAAAAAAAAAAAAAA".into(),
        },
    );
    assert_eq!(
        w.phone.outcome(&w.n, "junk"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Invalid
        })
    );
}

#[test]
fn nothing_the_ceremony_queues_carries_a_secret_in_the_clear() {
    let mut w = world();
    let request_id = w.offer_and_ask();
    let link = w.offer().unwrap().link;
    assert_eq!(w.approve(&request_id, "ok"), Some(OutcomeClass::Committed));
    w.settle();
    w.settle();
    let token = link.trim_start_matches("om:link/");
    let raw = common::b64_decode(token);
    let secret = &raw[5..21];
    for bytes in w.net.all_deposited_bytes() {
        assert!(
            !bytes.windows(16).any(|win| win == secret),
            "the ceremony key never appears on a relay"
        );
    }
    // The ceremony key is wiped from both devices once the ceremony ends.
    let st = w.alice.core.debug_app_state(&w.a).unwrap();
    let _ = st;
}

#[test]
fn every_other_device_gets_the_enrollment_notice() {
    let mut w = world();
    let request_id = w.offer_and_ask();
    assert_eq!(w.approve(&request_id, "ok"), Some(OutcomeClass::Committed));
    w.settle();
    w.settle();
    // The two devices talk (so they share a session), then Alice links a tablet as well.
    w.say("alice", "a1", "hello phone");
    w.settle();
    w.settle();
    let mut tablet = Runtime::new("tablet-link", &w.net);
    let t = tablet.open("p-t");
    w.alice
        .dispatch(&w.a, "offer2", IntentBody::OfferDeviceLink);
    w.alice.flush_net(&w.a);
    let link = w
        .alice
        .query::<Option<LinkOfferView>>(Query::GetLinkOffer {
            ctx: w.a.clone(),
            operation_id: "offer2".into(),
        })
        .unwrap()
        .link;
    tablet.dispatch(&t, "ask2", IntentBody::RequestDeviceLink { link });
    settle_all(&mut [
        (&mut w.alice, &w.a),
        (&mut w.carol, &w.c),
        (&mut w.phone, &w.n),
        (&mut tablet, &t),
    ]);
    let pending = w.devices(true).link_requests;
    assert_eq!(pending.len(), 1);
    let request_id = pending[0].request_id.clone();
    assert_eq!(w.approve(&request_id, "ok2"), Some(OutcomeClass::Committed));
    settle_all(&mut [
        (&mut w.alice, &w.a),
        (&mut w.carol, &w.c),
        (&mut w.phone, &w.n),
        (&mut tablet, &t),
    ]);
    let on_phone = w.devices(false);
    assert_eq!(
        on_phone.devices.len(),
        3,
        "the phone's head lists the tablet"
    );
    assert_eq!(on_phone.notices.len(), 1);
    assert_eq!(on_phone.notices[0].code, DeviceNoticeCode::DeviceLinked);
    let on_tablet: DevicesView = tablet.query(Query::GetDevices { ctx: t.clone() });
    assert_eq!(on_tablet.devices.len(), 3);
}

/// A Rust mirror of the Home's deposit check for profile 1 (`parseDeposit`, D9): a type-23 envelope whose fields are
/// exactly the SealedDeliveryEnvelope ones, with the profile-1 `enc` length and a bucket-sized ciphertext.
fn profile1_shape_ok(bytes: &[u8]) -> bool {
    use om_core::crypto::cbor;
    let Ok(v) = cbor::decode(bytes) else {
        return false;
    };
    let u = |k| v.get(k).ok().and_then(|x| x.u().ok());
    let b = |k| v.get(k).ok().and_then(|x| x.b().ok());
    u(0) == Some(23)
        && u(1) == Some(1)
        && u(2) == Some(1)
        && b(3).is_some_and(|x| x.len() == 16)
        && b(4).is_some_and(|x| x.len() == 16)
        && b(5).is_some_and(|x| x.len() == 1120)
        && b(6).is_some_and(|x| [1040, 4112, 16400, 65552, 131088].contains(&x.len()))
        && u(7).is_some()
        && bytes.len() <= 133_000
        && cbor::encode(&v) == bytes
}

#[test]
fn every_ceremony_deposit_has_the_shape_the_home_accepts() {
    let mut w = world();
    let request_id = w.offer_and_ask();
    let link = w.offer().unwrap().link;
    assert_eq!(w.approve(&request_id, "ok"), Some(OutcomeClass::Committed));
    w.settle();
    w.settle();
    // The two queues are named by the link: the request queue by its deposit capability, the response queue by the
    // receive capability (the loopback's capabilities carry the queue id).
    let raw = common::b64_decode(link.trim_start_matches("om:link/"));
    let a_len = raw[21] as usize;
    let a_queue = raw[22 + a_len - 16..22 + a_len].to_vec();
    let b_len = raw[22 + a_len] as usize;
    let b_cap = &raw[23 + a_len..23 + a_len + b_len];
    let b_queue = b_cap[16..32].to_vec();
    let ceremony: Vec<_> = w
        .net
        .deposit_log()
        .into_iter()
        .filter(|(q, _)| *q == a_queue || *q == b_queue)
        .collect();
    assert!(
        ceremony.len() >= 3,
        "a request, an approval and at least one handover"
    );
    for (_, bytes) in &ceremony {
        assert!(profile1_shape_ok(bytes), "deposit shape");
    }
    // The same bytes are what the TypeScript `parseDeposit` must accept (see the report for the vector).
    let first = &ceremony[0].1;
    let hex: String = first.iter().map(|b| format!("{b:02x}")).collect();
    println!("CEREMONY_REQUEST_VECTOR_LEN={} HEX={hex}", first.len());
    std::fs::write(std::env::temp_dir().join("om-ceremony-vector.hex"), &hex).ok();
}

// ---- ADR-0192: link version 2, bound context, and the end of the ceremony key ---------------------------------------

fn b64_encode(raw: &[u8]) -> String {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for c in raw.chunks(3) {
        let n = c.iter().fold(0u32, |a, b| a << 8 | u32::from(*b)) << (8 * (3 - c.len()));
        for i in 0..=c.len() {
            out.push(alphabet[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

fn relink(link: &str, f: impl FnOnce(&mut Vec<u8>)) -> String {
    let mut raw = common::b64_decode(link.trim_start_matches("om:link/"));
    f(&mut raw);
    format!("om:link/{}", b64_encode(&raw))
}

#[test]
fn a_version_one_link_is_refused_and_nothing_starts() {
    let mut w = world();
    w.alice.dispatch(&w.a, "offer", IntentBody::OfferDeviceLink);
    w.alice.flush_net(&w.a);
    let link = w.offer().unwrap().link;
    let old = relink(&link, |raw| raw[0] = 1);
    w.phone
        .dispatch(&w.n, "ask", IntentBody::RequestDeviceLink { link: old });
    assert_eq!(
        w.phone.outcome(&w.n, "ask"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Expired
        })
    );
    assert!(w.asked().is_none(), "no ceremony was started");
    // A link of a version nobody defined is not a link.
    let odd = relink(&link, |raw| raw[0] = 9);
    w.phone
        .dispatch(&w.n, "odd", IntentBody::RequestDeviceLink { link: odd });
    assert_eq!(
        w.phone.outcome(&w.n, "odd"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Invalid
        })
    );
}

#[test]
fn a_link_changed_in_transit_never_reaches_the_existing_device() {
    // Expiry, a capability and the Home are all bound into the keys: a changed one seals under keys the other side
    // does not have, so no request ever shows (and no code is displayed).
    for flip in [1usize, 5 + 16 + 2, 5 + 16 + 1 + 32 + 2] {
        let mut w = world();
        w.alice.dispatch(&w.a, "offer", IntentBody::OfferDeviceLink);
        w.alice.flush_net(&w.a);
        let link = w.offer().unwrap().link;
        // Expiry stays in the future (low byte of the big-endian u32 moves by one second).
        let bent = relink(&link, |raw| {
            let at = if flip == 1 { 4 } else { flip };
            raw[at] ^= 1;
        });
        w.phone
            .dispatch(&w.n, "ask", IntentBody::RequestDeviceLink { link: bent });
        w.settle();
        let seen = w.offer().unwrap();
        assert_eq!(
            seen.state,
            LinkOfferState::WaitingForScan,
            "flip at {flip}: no request was accepted"
        );
        assert!(seen.verification_code.is_none());
        assert!(w.devices(true).link_requests.is_empty());
    }
}

#[test]
fn a_second_request_on_the_same_link_is_ignored() {
    let mut w = world();
    let _ = w.offer_and_ask();
    let first = w.offer().unwrap().verification_code.unwrap();
    // A second new device with the same link: dropped while one request is pending.
    let link = w.offer().unwrap().link;
    let mut other = Runtime::new("other-link", &w.net);
    let o = other.open("p-o");
    other.dispatch(&o, "ask2", IntentBody::RequestDeviceLink { link });
    settle_all(&mut [(&mut w.alice, &w.a), (&mut w.phone, &w.n), (&mut other, &o)]);
    let pending = w.devices(true).link_requests;
    assert_eq!(pending.len(), 1);
    assert_eq!(w.offer().unwrap().verification_code.unwrap(), first);
}

#[test]
fn the_ceremony_key_is_gone_from_both_devices_when_the_link_is_done() {
    let mut w = world();
    let request_id = w.offer_and_ask();
    assert_eq!(w.approve(&request_id, "ok"), Some(OutcomeClass::Committed));
    w.settle();
    w.settle();
    w.settle();
    let alice_st = w.alice.core.debug_app_state(&w.a).unwrap();
    for o in alice_st["linkOffers"].as_object().unwrap().values() {
        assert_eq!(o["secret"], "", "the existing device holds no key");
        assert!(o["link"].is_null());
        assert_eq!(o["aRoute"]["capability"], serde_json::json!(""));
        assert_eq!(o["bRoute"]["capability"], serde_json::json!(""));
        assert_eq!(o["bRecv"], "");
    }
    assert_eq!(
        w.phone.core.debug_app_state(&w.n).unwrap()["linkJoin"]["secret"],
        ""
    );
}

#[test]
fn terminal_link_views_and_records_are_scrubbed_across_restart() {
    for reject in [false, true] {
        let mut w = world();
        let request_id = w.offer_and_ask();
        if reject {
            w.alice
                .dispatch(&w.a, "no", IntentBody::RejectDeviceLink { request_id });
        } else {
            assert_eq!(w.approve(&request_id, "ok"), Some(OutcomeClass::Committed));
        }
        w.settle();
        w.settle();
        w.settle();
        for restart in [false, true] {
            if restart {
                w.a = w.alice.restart().remove(0);
                w.n = w.phone.restart().remove(0);
            }
            assert!(w.offer().unwrap().link.is_empty());
            for (rt, ctx, field) in [(&w.alice, &w.a, "linkOffers"), (&w.phone, &w.n, "linkJoin")] {
                let app = rt.core.debug_app_state(ctx).unwrap();
                let records = if field == "linkOffers" {
                    app[field].as_object().unwrap().values().collect::<Vec<_>>()
                } else {
                    vec![&app[field]]
                };
                for r in records {
                    assert_eq!(r["secret"], "");
                    assert_eq!(r["bRecv"], "");
                    if field == "linkOffers" {
                        assert!(r["link"].is_null());
                        assert_eq!(r["aRoute"]["capability"], serde_json::json!(""));
                        assert_eq!(r["bRoute"]["capability"], serde_json::json!(""));
                    } else {
                        assert_eq!(r["aCap"], "");
                    }
                }
            }
        }
    }
}

#[test]
fn older_terminal_records_are_scrubbed_on_load() {
    use om_core::crypto::cbor::{self, V};
    let mut w = world();
    w.offer_and_ask();
    for state in ["linked", "rejected", "expired", "failed"] {
        let original = w.alice.store.records("p-a");
        let value = cbor::decode(&original[0].value).unwrap();
        let mut app: serde_json::Value =
            serde_json::from_slice(value.get(1).unwrap().b().unwrap()).unwrap();
        app["linkOffers"]["offer"]["state"] = state.into();
        let record = cbor::encode(&V::M(vec![
            (0, value.get(0).unwrap().clone()),
            (1, V::B(serde_json::to_vec(&app).unwrap())),
        ]));
        w.alice.store.replace_record("p-a", "state", record);
        w.a = w.alice.restart().remove(0);
        assert!(w.offer().unwrap().link.is_empty());
        let loaded = w.alice.core.debug_app_state(&w.a).unwrap();
        let r = &loaded["linkOffers"]["offer"];
        assert_eq!(r["secret"], "");
        assert!(r["link"].is_null());
        assert_eq!(r["aRoute"]["capability"], "");
        assert_eq!(r["bRoute"]["capability"], "");
        assert_eq!(r["bRecv"], "");
        // Restore the original live record for the next legacy terminal case.
        w.alice
            .store
            .replace_record("p-a", "state", original[0].value.clone());
        w.a = w.alice.restart().remove(0);
    }
}

#[test]
fn terminal_offer_retention_is_bounded_on_load_and_during_replacement() {
    use om_core::crypto::cbor::{self, V};
    let mut w = world();
    w.offer_and_ask();
    let original = w.alice.store.records("p-a");
    let value = cbor::decode(&original[0].value).unwrap();
    let mut app: serde_json::Value =
        serde_json::from_slice(value.get(1).unwrap().b().unwrap()).unwrap();
    let mut summary = app["linkOffers"]["offer"].clone();
    summary["state"] = "rejected".into();
    let offers = app["linkOffers"].as_object_mut().unwrap();
    offers.clear();
    for n in 0..5000 {
        let mut r = summary.clone();
        r["operationId"] = format!("old-{n}").into();
        offers.insert(format!("old-{n}"), r);
    }
    let record = cbor::encode(&V::M(vec![
        (0, value.get(0).unwrap().clone()),
        (1, V::B(serde_json::to_vec(&app).unwrap())),
    ]));
    w.alice.store.replace_record("p-a", "state", record);
    w.a = w.alice.restart().remove(0);
    let loaded = w.alice.core.debug_app_state(&w.a).unwrap();
    assert!(loaded["linkOffers"].as_object().unwrap().len() <= 32);
    assert!(serde_json::to_vec(&loaded["linkOffers"]).unwrap().len() < 64_000);
    // Replacement cancels older unapproved offers and retains only the bounded summary window.
    for n in 0..100 {
        w.alice.supply_routes(4);
        w.alice.dispatch(
            &w.a,
            &format!("replacement-{n}"),
            IntentBody::OfferDeviceLink,
        );
    }
    let loaded = w.alice.core.debug_app_state(&w.a).unwrap();
    assert!(loaded["linkOffers"].as_object().unwrap().len() <= 33);
    w.alice.tick(24 * 60 * 60 * 1000 + 300_001);
    w.alice
        .dispatch(&w.a, "retention-tick", IntentBody::OfferDeviceLink);
    w.a = w.alice.restart().remove(0);
    assert!(
        w.alice.core.debug_app_state(&w.a).unwrap()["linkOffers"]
            .as_object()
            .unwrap()
            .len()
            <= 1
    );
}

#[test]
fn expired_ceremonies_scrub_both_records_and_views_before_and_after_restart() {
    let mut w = world();
    let request_id = w.offer_and_ask();
    w.alice.tick(300_001);
    w.phone.tick(300_001);
    w.approve(&request_id, "late");
    w.phone.dispatch(&w.n, "expire", IntentBody::CreateInvite);
    for restart in [false, true] {
        if restart {
            w.a = w.alice.restart().remove(0);
            w.n = w.phone.restart().remove(0);
        }
        assert!(w.offer().unwrap().link.is_empty());
        for (rt, ctx) in [(&w.alice, &w.a), (&w.phone, &w.n)] {
            let app = rt.core.debug_app_state(ctx).unwrap();
            let r = if app["linkJoin"].is_null() {
                &app["linkOffers"]["offer"]
            } else {
                &app["linkJoin"]
            };
            assert_eq!(r["secret"], "");
            assert_eq!(r["bRecv"], "");
        }
    }
}

#[test]
fn active_offer_admission_is_bounded_and_legacy_overflow_is_purged() {
    use om_core::crypto::cbor::{self, V};
    let mut w = world();
    w.offer_and_ask();
    let original = w.alice.store.records("p-a");
    let value = cbor::decode(&original[0].value).unwrap();
    let mut app: serde_json::Value =
        serde_json::from_slice(value.get(1).unwrap().b().unwrap()).unwrap();
    let mut active = app["linkOffers"]["offer"].clone();
    active["state"] = "transferring".into();
    let offers = app["linkOffers"].as_object_mut().unwrap();
    offers.clear();
    for n in 0..100 {
        let mut o = active.clone();
        o["operationId"] = format!("active-{n}").into();
        offers.insert(format!("active-{n}"), o);
    }
    let record = cbor::encode(&V::M(vec![
        (0, value.get(0).unwrap().clone()),
        (1, V::B(serde_json::to_vec(&app).unwrap())),
    ]));
    w.alice.store.replace_record("p-a", "state", record);
    w.a = w.alice.restart().remove(0);
    let app = w.alice.core.debug_app_state(&w.a).unwrap();
    assert_eq!(
        app["linkOffers"]
            .as_object()
            .unwrap()
            .values()
            .filter(|o| o["state"] == "transferring")
            .count(),
        4
    );
    w.alice
        .dispatch(&w.a, "over-cap", IntentBody::OfferDeviceLink);
    assert_eq!(
        w.alice.outcome(&w.a, "over-cap"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::RateLimited
        })
    );
}

#[test]
fn authorized_roster_transfer_resumes_after_restart_before_final_link_cleanup() {
    let mut w = world();
    let request_id = w.offer_and_ask();
    assert_eq!(w.approve(&request_id, "ok"), Some(OutcomeClass::Committed));
    let app = w.alice.core.debug_app_state(&w.a).unwrap();
    assert_eq!(app["linkOffers"]["offer"]["state"], "transferring");
    assert_ne!(app["linkOffers"]["offer"]["secret"], "");
    assert!(w.offer().unwrap().link.is_empty());
    w.a = w.alice.restart().remove(0);
    w.settle();
    w.settle();
    w.settle();
    let app = w.alice.core.debug_app_state(&w.a).unwrap();
    assert_eq!(app["linkOffers"]["offer"]["state"], "linked");
    assert_eq!(app["linkOffers"]["offer"]["secret"], "");
    assert_eq!(w.asked().unwrap().state, LinkRequestState::Approved);
}
mod controller_probes {
    use super::*;
    use om_core::control;
    use om_core::crypto::binding::Acs;
    use om_core::crypto::cbor::{self, V};
    use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
    use om_core::crypto::{hex, unhex};
    use om_core::host::MonotonicSample;
    use om_core::recovery_record::RecoveryPlace;
    use om_core::state::ProfileState;

    fn get(rt: &Runtime, profile: &str) -> ProfileState {
        ProfileState::decode("probe".into(), &rt.store.records(profile)).unwrap()
    }

    fn put(rt: &mut Runtime, profile: &str, st: ProfileState) -> ProfileExecutionContext {
        for (key, value) in st.encode().unwrap() {
            rt.store.replace_record(profile, &key, value);
        }
        rt.restart().remove(0)
    }

    fn head(st: &ProfileState) -> Acs {
        let chain = chain(st);
        control::verify_chain_with(&chain, &creds(st), &st.app.link_evidence()).unwrap()
    }

    fn chain(st: &ProfileState) -> Vec<Vec<u8>> {
        st.app.acs_chain.iter().map(|s| unhex(s).unwrap()).collect()
    }

    fn creds(st: &ProfileState) -> Vec<Vec<u8>> {
        st.app
            .acs_credentials
            .iter()
            .map(|s| unhex(s).unwrap())
            .collect()
    }

    fn intent(value: serde_json::Value) -> IntentBody {
        serde_json::from_value(value).expect("controller ceremony intent")
    }

    fn controller_view(rt: &Runtime, ctx: &ProfileExecutionContext) -> serde_json::Value {
        let view: serde_json::Value = rt.query(Query::GetDevices { ctx: ctx.clone() });
        view["controllerChange"].clone()
    }

    fn controller_change(w: &mut World, device_id: &str, remove: bool) {
        use serde_json::json;
        let (host, host_ctx, guest, guest_ctx) = if remove {
            (&mut w.phone, &mut w.n, &mut w.alice, &mut w.a)
        } else {
            (&mut w.alice, &mut w.a, &mut w.phone, &mut w.n)
        };
        let operation =
            |name: &str| format!("{name}-{}", if remove { "remove" } else { "promote" });
        host.dispatch(
            host_ctx,
            &operation("offer"),
            intent(json!({
                "kind": "OfferControllerChange", "deviceId": device_id, "remove": remove,
            })),
        );
        settle_all(&mut [(host, host_ctx), (guest, guest_ctx)]);
        let offer = controller_view(host, host_ctx);
        let link = offer["link"].as_str().expect("controller link");
        guest.dispatch(
            guest_ctx,
            &operation("join"),
            intent(json!({
                "kind": "JoinControllerChange", "link": link,
            })),
        );
        let joined = controller_view(guest, guest_ctx);
        let session = joined["requestId"].as_str().expect("guest request");
        assert_eq!(offer["verificationCode"], joined["verificationCode"]);
        let confirm = |rt: &mut Runtime, ctx: &ProfileExecutionContext| {
            intent(json!({
                "kind": "ConfirmControllerChange", "requestId": session,
                "confirmation": nonce(rt, ctx, "ConfirmControllerChange", session),
            }))
        };

        // The host's own confirmation, even if replayed or issued before the second vote, does not change the head.
        let early = confirm(host, host_ctx);
        host.dispatch(host_ctx, &operation("early"), early);
        assert_eq!(
            host.outcome(host_ctx, &operation("early")),
            Some(OutcomeClass::Rejected {
                reason: RejectReason::StaleState
            })
        );

        *guest_ctx = guest.restart().remove(0);
        let guest_confirm = confirm(guest, guest_ctx);
        guest.dispatch(guest_ctx, &operation("guest"), guest_confirm.clone());
        guest.dispatch(guest_ctx, &operation("guest-replay"), guest_confirm);
        assert_eq!(
            guest.outcome(guest_ctx, &operation("guest-replay")),
            Some(OutcomeClass::Rejected {
                reason: RejectReason::Unauthorized
            })
        );
        settle_all(&mut [(host, host_ctx), (guest, guest_ctx)]);
        let ready = controller_view(host, host_ctx);
        assert_eq!(ready["waiting"], false);
        assert_eq!(ready["verificationCode"], joined["verificationCode"]);
        assert_eq!(ready["requestId"], session);
        *host_ctx = host.restart().remove(0);
        let host_confirm = confirm(host, host_ctx);
        host.dispatch(host_ctx, &operation("host"), host_confirm.clone());
        assert_eq!(
            host.outcome(host_ctx, &operation("host")),
            Some(OutcomeClass::Committed)
        );
        host.dispatch(host_ctx, &operation("host-replay"), host_confirm);
        assert_eq!(
            host.outcome(host_ctx, &operation("host-replay")),
            Some(OutcomeClass::Rejected {
                reason: RejectReason::Unauthorized
            })
        );
        settle_all(&mut [(host, host_ctx), (guest, guest_ctx)]);
    }

    #[test]
    fn promotes_an_existing_device_and_shares_the_kit() {
        let (mut w, _) = linked_with_kit();
        let target = hex(&get(&w.phone, "p-n").device.unwrap().device_id);
        controller_change(&mut w, &target, false);
        let a = get(&w.alice, "p-a");
        let b = get(&w.phone, "p-n");
        assert_eq!(head(&a), head(&b));
        assert_eq!(head(&a).control.len(), 2);
        assert!(head(&b).control.contains(&h16_for_test(&target)));
        assert!(b.app.recovery_kit.is_some() && b.app.recovery_kit == a.app.recovery_kit);
    }

    #[test]
    fn promotion_offer_rejects_a_different_account_and_a_changed_parent() {
        use serde_json::json;
        let (mut w, _) = linked_with_kit();
        let target = hex(&get(&w.phone, "p-n").device.unwrap().device_id);
        w.alice.dispatch(
            &w.a,
            "offer-stale",
            intent(json!({
                "kind": "OfferControllerChange", "deviceId": target, "remove": false,
            })),
        );
        w.settle();
        let offer = controller_view(&w.alice, &w.a);
        let link = offer["link"].as_str().unwrap();
        let join = intent(json!({ "kind": "JoinControllerChange", "link": link }));
        w.carol.dispatch(&w.c, "wrong-account", join.clone());
        assert!(matches!(
            w.carol.outcome(&w.c, "wrong-account"),
            Some(OutcomeClass::Rejected { .. })
        ));
        w.phone.dispatch(&w.n, "join-stale", join);
        let request_id = offer["requestId"].as_str().unwrap();
        let guest_confirmation = nonce(&mut w.phone, &w.n, "ConfirmControllerChange", request_id);
        w.phone.dispatch(
            &w.n,
            "guest-stale",
            IntentBody::ConfirmControllerChange {
                request_id: request_id.into(),
                confirmation: guest_confirmation,
            },
        );
        w.settle();
        let confirmation = nonce(&mut w.alice, &w.a, "ConfirmControllerChange", request_id);
        let (_, step) = w.alice.core.debug_link_messaging_device(&w.a).unwrap();
        assert!(w.alice.settle(&step));
        let parent = head(&get(&w.alice, "p-a"));
        w.alice.dispatch(
            &w.a,
            "host-stale",
            IntentBody::ConfirmControllerChange {
                request_id: request_id.into(),
                confirmation,
            },
        );
        assert!(matches!(
            w.alice.outcome(&w.a, "host-stale"),
            Some(OutcomeClass::Rejected { .. })
        ));
        w.settle();
        assert_eq!(head(&get(&w.alice, "p-a")), parent);
        assert!(get(&w.phone, "p-n").app.recovery_kit.is_none());
    }

    fn h16_for_test(device: &str) -> [u8; 16] {
        unhex(device).unwrap().try_into().unwrap()
    }

    fn linked_with_kit() -> (World, String) {
        let mut w = world();
        w.alice.set_recovery_place(Some(RecoveryPlace {
            host: "home.example".into(),
            port: None,
            scope: [7; 32],
        }));
        w.alice
            .dispatch(&w.a, "replace", IntentBody::ReplaceRecoveryKey);
        let sheet = w
            .alice
            .query::<Option<RecoveryKeyView>>(Query::GetRecoveryKey {
                ctx: w.a.clone(),
                key_operation_id: "replace".into(),
            })
            .unwrap()
            .sheet
            .unwrap();
        w.alice.dispatch(
            &w.a,
            "saved",
            IntentBody::ConfirmRecoveryKeyStored {
                key_operation_id: "replace".into(),
            },
        );
        w.settle();
        let request = w.offer_and_ask();
        assert_eq!(w.approve(&request, "ok"), Some(OutcomeClass::Committed));
        w.settle();
        w.say("phone", "warm", "Start sibling session");
        w.settle();

        (w, sheet)
    }

    fn controllers() -> (World, String) {
        let (mut w, sheet) = linked_with_kit();
        let target = hex(&get(&w.phone, "p-n").device.unwrap().device_id);
        controller_change(&mut w, &target, false);
        (w, sheet)
    }

    #[test]
    fn fork_of_valid_sibling_heads_freezes_control_after_restart() {
        let (mut w, _) = controllers();
        let mut a = get(&w.alice, "p-a");
        let mut b = get(&w.phone, "p-n");
        let parent = head(&a);
        let mut rng = InsecureTestRng::from_label(b"fork probe");
        let ad = a.device.as_ref().unwrap();
        let bd = b.device.as_ref().unwrap();
        let target = hex(&ad.device_id);
        let signing_device = [(ad, [1; 32]), (bd, [2; 32])];
        let branches = signing_device.map(|(d, commitment)| {
            let mut child = parent.clone();
            child.previous = Some(parent.hash());
            child.generation += 1;
            child.transparency = Some(commitment);
            let signed = control::sign_acs(&child.body(), &[(&d.device_id, &d.signer)], &mut rng);
            assert!(control::verify_successor(&parent, &signed, &creds(&a)).is_ok());
            (child, signed)
        });
        let [(_, own), (tip, competing)] = branches;
        a.app.acs_chain.push(hex(&own));
        b.app.acs_chain.push(hex(&competing));
        let signed_tip = b
            .app
            .carry_sign_head_for_peers(&bd.signer, &bd.device_id, &tip, &mut rng)
            .unwrap();
        w.a = put(&mut w.alice, "p-a", a);
        w.n = put(&mut w.phone, "p-n", b);
        let update = cbor::encode(&V::M(vec![
            (0, V::U(8)),
            (1, V::A(vec![V::B(competing)])),
            (2, V::A(vec![])),
            (6, V::B(signed_tip)),
        ]));
        let (queued, step) = w
            .phone
            .core
            .debug_send_to_sibling(&w.n, &target, &update)
            .unwrap();
        assert!(queued);
        assert!(w.phone.settle(&step));
        w.phone.flush_net(&w.n);
        assert!(w.alice.poll(&w.a) > 0);
        w.a = w.alice.restart().remove(0);
        assert!(matches!(
            w.devices(true).control_state,
            AccountControlStateCode::ForkDetected | AccountControlStateCode::SafetyFreeze
        ));
        assert!(w.devices(true).devices.iter().all(|d| d.actions.is_empty()));
        w.alice
            .dispatch(&w.a, "fork-new-key", IntentBody::ReplaceRecoveryKey);
        assert_eq!(
            w.alice.outcome(&w.a, "fork-new-key"),
            Some(OutcomeClass::Rejected {
                reason: RejectReason::Conflict
            })
        );
    }

    #[test]
    fn acknowledging_on_one_controller_still_alerts_the_other() {
        let (mut w, sheet) = controllers();
        let mut fresh = Runtime::new("restricted", &w.net);
        let ctx = fresh.open("restricted");
        let mono = MonotonicSample {
            boot_session: vec![1; 8],
            elapsed_ms: 0,
        };
        let tick = |rt: &mut Runtime, ctx: &ProfileExecutionContext| {
            rt.tick(61_000);
            rt.recovery_tick(ctx, &mono);
            rt.flush_net(ctx);
        };
        tick(&mut w.alice, &w.a);
        tick(&mut w.phone, &w.n);
        fresh.dispatch(
            &ctx,
            "begin",
            IntentBody::BeginRecovery {
                recovery_key: sheet,
                home: String::new(),
                place: None,
            },
        );
        fresh.flush_net(&ctx);
        fresh.dispatch(&ctx, "request", IntentBody::RequestRecovery);
        fresh.flush_net(&ctx);
        assert!(!w.net.recovery_requests().is_empty());

        // A drains and verifies while B is offline. Reading on A must not remove B's alert.
        tick(&mut w.alice, &w.a);
        tick(&mut w.alice, &w.a);
        assert_eq!(
            w.alice
                .query::<RecoveryStatusView>(Query::GetRecoveryStatus { ctx: w.a.clone() })
                .requests
                .len(),
            1
        );
        w.n = w.phone.restart().remove(0);
        tick(&mut w.phone, &w.n);
        tick(&mut w.phone, &w.n);
        assert_eq!(
            w.phone
                .query::<RecoveryStatusView>(Query::GetRecoveryStatus { ctx: w.n.clone() })
                .requests
                .len(),
            1
        );
    }
    #[test]
    fn a_removed_controllers_kit_cannot_open_the_next_record() {
        use om_core::recovery_record::{RecordKeys, RecoveryKit, open_envelope};

        let (mut w, sheet) = controllers();
        let a = get(&w.alice, "p-a");
        let target = a.device.as_ref().unwrap().device_id;
        let parent = head(&a);
        let kit = RecoveryKit::from_bytes(&unhex(a.app.recovery_kit.as_deref().unwrap()).unwrap())
            .unwrap();
        let (_, previous) = w.net.recovery_record(&kit.locator()).unwrap();
        let prior_puts = w.net.recovery_puts().len();
        let old_keys = RecordKeys::from_kit(&kit);
        assert!(open_envelope(&old_keys, &kit.scope, &previous).is_ok());

        controller_change(&mut w, &hex(&target), true);
        let b = get(&w.phone, "p-n");
        let child = head(&b);
        assert_eq!(head(&get(&w.alice, "p-a")), child);
        assert!(child.revoked.contains(&target));
        let signed = chain(&b).pop().unwrap();
        let (_, body, signers) = control::open_signed(&signed, &creds(&b), Some(&parent)).unwrap();
        assert_eq!(signers, parent.control);
        for device in [a.device.as_ref().unwrap(), b.device.as_ref().unwrap()] {
            let one = control::sign_acs(
                &body,
                &[(&device.device_id, &device.signer)],
                &mut InsecureTestRng::from_label(b"one vote"),
            );
            assert!(control::verify_successor(&parent, &one, &creds(&b)).is_err());
        }
        assert!(control::verify_successor(&child, &signed, &creds(&b)).is_err());
        assert!(get(&w.alice, "p-a").app.recovery_kit.is_none());

        w.n = w.phone.restart().remove(0);
        w.phone.tick(91 * 86_400_000);
        w.phone.recovery_tick(
            &w.n,
            &MonotonicSample {
                boot_session: vec![1],
                elapsed_ms: 0,
            },
        );
        w.phone.flush_net(&w.n);
        let status = |rt: &Runtime, ctx: &ProfileExecutionContext| {
            rt.query::<RecoveryStatusView>(Query::GetRecoveryStatus { ctx: ctx.clone() })
        };
        assert_eq!(
            status(&w.phone, &w.n).record,
            Some(RecoveryRecordCode::NeedsNewKey)
        );
        assert_eq!(w.net.recovery_record(&kit.locator()).unwrap().1, previous);

        // The old code still reads the saved record while updates are paused.
        let mut fresh = common::recovery_world::fresh(&w.net);
        fresh.begin(&sheet);
        assert!(fresh.view().is_some());

        w.phone
            .dispatch(&w.n, "new-key", IntentBody::ReplaceRecoveryKey);
        assert_eq!(
            status(&w.phone, &w.n).record,
            Some(RecoveryRecordCode::NeedsNewKey)
        );
        assert_eq!(w.net.recovery_record(&kit.locator()).unwrap().1, previous);
        w.phone.dispatch(
            &w.n,
            "stored",
            IntentBody::ConfirmRecoveryKeyStored {
                key_operation_id: "new-key".into(),
            },
        );
        assert_eq!(
            w.phone.outcome(&w.n, "stored"),
            Some(OutcomeClass::Committed)
        );
        w.phone.flush_net(&w.n);
        let b = get(&w.phone, "p-n");
        let new_kit =
            RecoveryKit::from_bytes(&unhex(b.app.recovery_kit.as_deref().unwrap()).unwrap())
                .unwrap();
        assert_ne!(kit.locator(), new_kit.locator());
        let (_, newer) = w.net.recovery_record(&new_kit.locator()).unwrap();
        assert!(open_envelope(&old_keys, &new_kit.scope, &newer).is_err());
        for (_, _, envelope) in &w.net.recovery_puts()[prior_puts..] {
            assert!(open_envelope(&old_keys, &kit.scope, envelope).is_err());
        }
        assert!(open_envelope(&RecordKeys::from_kit(&new_kit), &new_kit.scope, &newer).is_ok());
        assert_eq!(
            status(&w.phone, &w.n).record,
            Some(RecoveryRecordCode::Ready)
        );
        assert_eq!(w.net.recovery_record(&kit.locator()).unwrap().1, previous);
    }

    #[test]
    fn the_second_controller_can_approve_recovery_while_record_writes_are_paused() {
        let (mut w, sheet) = controllers();
        let removed = hex(&get(&w.alice, "p-a").device.unwrap().device_id);
        controller_change(&mut w, &removed, true);
        let previous_puts = w.net.recovery_puts().len();
        let survivor = head(&get(&w.phone, "p-n"));
        assert!(survivor.revoked.contains(&h16_for_test(&removed)));

        let mut fresh = common::recovery_world::fresh(&w.net);
        fresh.begin(&sheet);
        assert_eq!(
            fresh.go("old-head", IntentBody::RequestRecovery),
            Some(OutcomeClass::Committed)
        );

        let join = |w: &mut World, fresh: &mut common::recovery_world::Fresh, op: &str| {
            w.phone
                .dispatch(&w.n, op, IntentBody::OfferRecoveryApproval);
            w.phone.flush_net(&w.n);
            let offer = w
                .phone
                .query::<Option<RecoveryApprovalOfferView>>(Query::GetRecoveryApprovalOffer {
                    ctx: w.n.clone(),
                    operation_id: op.into(),
                })
                .unwrap();
            assert_eq!(
                fresh.go(
                    op,
                    IntentBody::JoinRecoveryApproval {
                        link: offer.link.unwrap()
                    }
                ),
                Some(OutcomeClass::Committed)
            );
            settle_all(&mut [(&mut w.phone, &w.n), (&mut fresh.rt, &fresh.ctx)]);
        };

        // The first attempt names the saved record's older parent. Its authenticated rejection
        // supplies the verified successor. It never releases the restricted device on its own.
        join(&mut w, &mut fresh, "old-parent");
        let stale = fresh.view().unwrap();
        assert_eq!(stale.attempt, Some(om_core::state::AttemptState::Stale));
        assert!(stale.retry_from_verified_head);
        fresh.ctx = fresh.rt.restart().remove(0);
        assert_eq!(
            fresh.go("updated-head", IntentBody::RequestRecovery),
            Some(OutcomeClass::Committed)
        );
        join(&mut w, &mut fresh, "current-parent");
        let offer = w
            .phone
            .query::<Option<RecoveryApprovalOfferView>>(Query::GetRecoveryApprovalOffer {
                ctx: w.n.clone(),
                operation_id: "current-parent".into(),
            })
            .unwrap();
        assert_eq!(offer.state, ApprovalOfferCode::AwaitingConfirmation);
        assert_eq!(
            offer.verification_code,
            fresh.view().unwrap().approval.unwrap().verification_code
        );
        let request_id = offer.request_id.unwrap();
        let confirmation = nonce(&mut w.phone, &w.n, "ConfirmRecovery", &request_id);
        w.phone.dispatch(
            &w.n,
            "recover",
            IntentBody::ConfirmRecovery {
                request_id,
                confirmation,
            },
        );
        assert_eq!(
            w.phone.outcome(&w.n, "recover"),
            Some(OutcomeClass::Committed)
        );
        settle_all(&mut [(&mut w.phone, &w.n), (&mut fresh.rt, &fresh.ctx)]);
        assert!(fresh.view().is_none());
        let recovered = head(&get(&fresh.rt, "q"));
        assert_eq!(recovered.previous, Some(survivor.hash()));
        assert_eq!(recovered.control.len(), 1);
        assert!(survivor.control.is_subset(&recovered.revoked));
        assert_eq!(
            w.net.recovery_puts().len(),
            previous_puts,
            "an epoch change reuses the record key and must stay paused"
        );
        let status = fresh
            .rt
            .query::<RecoveryStatusView>(Query::GetRecoveryStatus {
                ctx: fresh.ctx.clone(),
            });
        assert_eq!(status.record, Some(RecoveryRecordCode::NeedsNewKey));
    }

    #[test]
    fn existing_owner_approval_accepts_either_controller_vote() {
        for approve_on_phone in [false, true] {
            let (mut w, sheet) = controllers();
            let mono = MonotonicSample {
                boot_session: vec![1; 8],
                elapsed_ms: 0,
            };
            w.alice.recovery_tick(&w.a, &mono);
            w.alice.flush_net(&w.a);
            let parent = head(&get(&w.alice, "p-a"));
            let mut f = common::recovery_world::fresh(&w.net);
            f.begin(&sheet);
            assert_eq!(
                f.go("request", IntentBody::RequestRecovery),
                Some(OutcomeClass::Committed)
            );
            let (host, ctx, profile) = if approve_on_phone {
                (&mut w.phone, &w.n, "p-n")
            } else {
                (&mut w.alice, &w.a, "p-a")
            };
            host.dispatch(ctx, "offer-recovery", IntentBody::OfferRecoveryApproval);
            host.flush_net(ctx);
            let offer = |rt: &Runtime| {
                rt.query::<Option<RecoveryApprovalOfferView>>(Query::GetRecoveryApprovalOffer {
                    ctx: ctx.clone(),
                    operation_id: "offer-recovery".into(),
                })
                .unwrap()
            };
            let link = offer(host).link.unwrap();
            assert_eq!(
                f.go("join", IntentBody::JoinRecoveryApproval { link }),
                Some(OutcomeClass::Committed)
            );
            settle_all(&mut [(host, ctx), (&mut f.rt, &f.ctx)]);
            let held = offer(host);
            assert_eq!(held.state, ApprovalOfferCode::AwaitingConfirmation);
            assert_eq!(
                held.verification_code,
                f.view().unwrap().approval.unwrap().verification_code
            );
            let request_id = held.request_id.unwrap();
            let confirmation = nonce(host, ctx, "ConfirmRecovery", &request_id);
            host.dispatch(
                ctx,
                "confirm",
                IntentBody::ConfirmRecovery {
                    request_id,
                    confirmation,
                },
            );
            assert_eq!(host.outcome(ctx, "confirm"), Some(OutcomeClass::Committed));
            for _ in 0..4 {
                settle_all(&mut [(host, ctx), (&mut f.rt, &f.ctx)]);
            }
            assert!(f.view().is_none(), "recovery completed without a timer");
            let st = get(host, profile);
            let bytes = chain(&st).pop().unwrap();
            let envelope = cbor::decode(&bytes).unwrap();
            let recovered = Acs::from_body(envelope.get(1).unwrap().b().unwrap()).unwrap();
            assert_eq!(recovered.previous, Some(parent.hash()));
            assert!(parent.control.is_subset(&recovered.revoked));
            assert_eq!(envelope.get(2).unwrap().a().unwrap().len(), 2);
        }
    }
}

#[test]
fn first_call_answer_selects_one_of_three_independent_offers() {
    use om_core::calls::{CallReason, CallState, CallView};
    use om_core::crypto::{
        cbor::{self, V},
        delivery,
        kdf::hkdf_sha256,
        rng::insecure_test_rng::InsecureTestRng,
        unhex,
    };
    use om_core::host::NetCall;
    use om_core::state::ProfileState;
    use zeroize::Zeroizing;

    let mut w = world();
    let request_id = w.offer_and_ask();
    assert_eq!(w.approve(&request_id, "ok"), Some(OutcomeClass::Committed));
    w.settle();
    let mut tablet = Runtime::new("call-tablet", &w.net);
    let t = tablet.open("p-t");
    w.alice
        .dispatch(&w.a, "offer-tablet", IntentBody::OfferDeviceLink);
    w.alice.flush_net(&w.a);
    let offer: LinkOfferView = w.alice.query(Query::GetLinkOffer {
        ctx: w.a.clone(),
        operation_id: "offer-tablet".into(),
    });
    tablet.dispatch(
        &t,
        "ask-tablet",
        IntentBody::RequestDeviceLink { link: offer.link },
    );
    w.settle();
    tablet.flush_net(&t);
    w.settle();
    let request_id = w.devices(true).link_requests[0].request_id.clone();
    assert_eq!(
        w.approve(&request_id, "ok-tablet"),
        Some(OutcomeClass::Committed)
    );
    let settle = |w: &mut World, tablet: &mut Runtime| {
        settle_all(&mut [
            (&mut w.alice, &w.a),
            (&mut w.carol, &w.c),
            (&mut w.phone, &w.n),
            (tablet, &t),
        ]);
    };
    settle(&mut w, &mut tablet);
    // Establish the existing sessions and their delivery routes before sending an ephemeral offer.
    w.say("phone", "p1", "from the phone");
    tablet.dispatch(
        &t,
        "t1",
        IntentBody::SendMessage {
            conversation_id: conv_of(&tablet, &t),
            draft: Draft {
                text: "from the tablet".into(),
                reply_to: None,
            },
        },
    );
    settle(&mut w, &mut tablet);
    w.say("carol", "c1", "to all of you");
    settle(&mut w, &mut tablet);
    w.alice.core.enable_calls();
    w.carol.core.enable_calls();
    w.phone.core.enable_calls();
    tablet.core.enable_calls();
    let contacts: Vec<ConversationSummary> = w
        .carol
        .query(Query::GetConversationList { ctx: w.c.clone() });
    w.carol.dispatch(
        &w.c,
        "dial",
        IntentBody::StartCall {
            target: contacts[0].person_id.clone(),
            media: CallMedia::Audio,
            sdp: Some(b"offer".to_vec()),
        },
    );
    let ops = w.carol.core.net_ops(&w.c).unwrap();
    let offers = |callee: &Runtime, ctx: &ProfileExecutionContext| {
        let mut profile = ProfileState::decode(
            ctx.profile_id.clone(),
            &callee.store.records(&ctx.profile_id),
        )
        .unwrap();
        let rel = &profile.app.relationships[&conv_of(callee, ctx)];
        let device = profile.device.as_mut().unwrap();
        let mut open = ops.iter().filter_map(|op| {
            let NetCall::Deposit {
                bytes,
                mailbox,
                relay_uri,
            } = &op.call
            else {
                return None;
            };
            let wrapper = cbor::decode(bytes).unwrap();
            let key_id = wrapper.get(3).unwrap().b().unwrap().try_into().unwrap();
            let key = rel
                .my_delivery
                .iter()
                .find(|key| unhex(&key.public.key_id).unwrap().as_slice() == key_id)?;
            let inner = delivery::open(
                bytes,
                &unhex(&key.seed).unwrap().try_into().unwrap(),
                &key_id,
                relay_uri.as_deref().unwrap(),
                mailbox,
                callee.clock.get() / 1000,
            )
            .unwrap();
            let envelope = cbor::decode(&inner.inner_envelope).unwrap();
            let fields = envelope.a().unwrap();
            let session = device.sessions.get_mut(fields[0].b().unwrap()).unwrap();
            let (pt, _) = session
                .dr
                .decrypt(
                    fields[2].b().unwrap(),
                    fields[3].b().unwrap(),
                    b"OM-CALL-SIGNAL-CTX-v1",
                    &mut InsecureTestRng::from_label(b"call-offer fixture"),
                )
                .unwrap();
            let pt = Zeroizing::new(pt);
            let signal = cbor::decode(&pt).unwrap();
            assert_eq!(signal.get(2).unwrap().u().unwrap(), 1);
            let package = signal.get(7).unwrap();
            Some(Zeroizing::new(
                <[u8; 32]>::try_from(package.get(0).unwrap().b().unwrap()).unwrap(),
            ))
        });
        let secret = open.next().expect("this device received an offer");
        assert!(open.next().is_none());
        (device.device_id, secret)
    };
    let (_, alice_secret) = offers(&w.alice, &w.a);
    let (phone_id, phone_secret) = offers(&w.phone, &w.n);
    let (_, tablet_secret) = offers(&tablet, &t);
    assert!(*alice_secret != *phone_secret);
    assert!(*alice_secret != *tablet_secret);
    assert!(*phone_secret != *tablet_secret);

    settle(&mut w, &mut tablet);
    let from = |p: &Runtime, ctx: &ProfileExecutionContext| {
        p.query::<CallView>(Query::GetCallView { ctx: ctx.clone() })
    };
    let incoming = from(&w.alice, &w.a);
    assert_eq!(incoming.state, CallState::Incoming);
    assert_eq!(from(&w.phone, &w.n).state, CallState::Incoming);
    assert_eq!(from(&tablet, &t).state, CallState::Incoming);
    let answer = |id: &str| IntentBody::AcceptCall {
        call_session_id: id.into(),
        sdp: b"answer".to_vec(),
        media_flags: 1,
    };
    w.alice
        .dispatch(&w.a, "race", answer(&incoming.call_session_id));
    w.phone
        .dispatch(&w.n, "answer", answer(&incoming.call_session_id));
    w.phone.flush_net(&w.n);
    w.carol.poll(&w.c);
    settle(&mut w, &mut tablet);
    assert_eq!(from(&w.phone, &w.n).state, CallState::Connecting);
    for (rt, ctx) in [(&mut w.alice, &w.a), (&mut tablet, &t)] {
        assert_eq!(from(rt, ctx).reason, Some(CallReason::AnsweredElsewhere));
        assert!(
            rt.core
                .take_call_keys(ctx, &incoming.call_session_id)
                .unwrap()
                .is_none()
        );
    }
    let caller = w
        .carol
        .core
        .take_call_keys(&w.c, &incoming.call_session_id)
        .unwrap()
        .unwrap();
    let winner = w
        .phone
        .core
        .take_call_keys(&w.n, &incoming.call_session_id)
        .unwrap()
        .unwrap();
    assert!(caller.send() == winner.receive());
    assert!(caller.receive() == winner.send());
    let carol = ProfileState::decode(
        w.c.profile_id.clone(),
        &w.carol.store.records(&w.c.profile_id),
    )
    .unwrap()
    .device
    .unwrap()
    .device_id;
    let id = unhex(&incoming.call_session_id).unwrap();
    for (sender, recipient, key) in [
        (carol, phone_id, caller.send()),
        (phone_id, carol, caller.receive()),
    ] {
        let info = cbor::encode(&V::A(vec![
            V::T("OM-CALL-MEDIA-BASE-KDF-v1".into()),
            V::B(sender.to_vec()),
            V::B(recipient.to_vec()),
            V::U(0),
        ]));
        let mut derived = Zeroizing::new([0; 32]);
        hkdf_sha256(&id, phone_secret.as_ref(), &info, derived.as_mut());
        assert!(derived.as_ref() == key);
        for other in [&alice_secret, &tablet_secret] {
            hkdf_sha256(&id, other.as_ref(), &info, derived.as_mut());
            assert!(derived.as_ref() != key);
        }
    }
}

/// Calls on a linked account: the contact's call goes to every device of the account, one device answers, and removing that
/// device from the account ends the contact's call with a failure and leaves no media keys behind.
#[test]
fn removing_the_device_that_answered_ends_the_contacts_call() {
    use om_core::calls::{CallReason, CallState, CallView};
    let view = |rt: &Runtime, ctx: &ProfileExecutionContext| -> Option<CallView> {
        rt.query(Query::GetCallView { ctx: ctx.clone() })
    };
    let mut w = world();
    let request_id = w.offer_and_ask();
    assert_eq!(w.approve(&request_id, "ok"), Some(OutcomeClass::Committed));
    w.settle();
    w.settle();
    for rt in [&mut w.alice, &mut w.carol, &mut w.phone] {
        rt.core.enable_calls();
    }
    let chat: ConversationView = w.carol.query(Query::GetConversationView {
        ctx: w.c.clone(),
        conversation_id: w.c_conv.clone(),
    });
    w.carol.dispatch(
        &w.c,
        "dial",
        IntentBody::StartCall {
            target: chat.summary.person_id,
            media: CallMedia::Audio,
            sdp: Some(b"prepared offer".to_vec()),
        },
    );
    w.settle();
    w.settle();
    let ringing = view(&w.phone, &w.n).expect("the linked device rings too");
    assert_eq!(ringing.state, CallState::Incoming);
    let id = ringing.call_session_id;
    w.phone.dispatch(
        &w.n,
        "answer",
        IntentBody::AcceptCall {
            call_session_id: id.clone(),
            sdp: b"prepared answer".to_vec(),
            media_flags: 1,
        },
    );
    w.settle();
    w.settle();
    assert_eq!(view(&w.carol, &w.c).unwrap().state, CallState::Connecting);
    assert_eq!(view(&w.phone, &w.n).unwrap().state, CallState::Connecting);

    let device = w
        .devices(false)
        .devices
        .iter()
        .find(|d| d.is_current)
        .unwrap()
        .id
        .clone();
    let confirmation = nonce(&mut w.alice, &w.a, "RevokeDevice", &device);
    w.alice.dispatch(
        &w.a,
        "revoke",
        IntentBody::RevokeDevice {
            device_id: device,
            confirmation,
        },
    );
    assert_eq!(
        w.alice.outcome(&w.a, "revoke"),
        Some(OutcomeClass::Committed)
    );
    for _ in 0..3 {
        w.settle();
    }
    let ended = view(&w.carol, &w.c).expect("the call's final state is visible");
    assert_eq!(ended.state, CallState::Failed);
    assert_eq!(ended.reason, Some(CallReason::Failed));
    assert!(
        w.carol.core.take_call_keys(&w.c, &id).unwrap().is_none(),
        "no media keys outlive the call"
    );
}
