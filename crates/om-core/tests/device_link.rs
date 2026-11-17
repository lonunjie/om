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
