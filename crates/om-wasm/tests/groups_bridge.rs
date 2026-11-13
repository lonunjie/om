// SPDX-License-Identifier: MPL-2.0
//! Ordinary groups through the JSON bridge (`group_do`, `group_receive`, `group_query`): two bridges, each opened from
//! the stored records of a profile that is `Active` with the other (made natively over the loopback provider), then
//! the whole group flow as JSON commands, the way the TypeScript binding drives the wasm module. The group wire in
//! the `out` items is Core-internal JSON (groups plan Q2), not a frozen format. The engine is the test-only fake by
//! default and OpenMLS with `--features groups-openmls`.
use om_core::api::*;
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};
use om_wasm::bridge::Bridge;
use serde_json::{Value, json};

fn call(b: &mut Bridge<InsecureTestRng>, v: Value) -> Value {
    serde_json::from_slice(&b.handle(&serde_json::to_vec(&v).unwrap())).unwrap()
}

fn home() -> Value {
    json!({ "tier": "T1", "observers": ["host_sees_ciphertext"], "migrationAvailable": true, "clientOriginChoice": ["default_origin"] })
}

fn create_profile(rt: &mut Runtime, id: &str, label: &str) -> ProfileExecutionContext {
    let ctx = rt.open(id);
    rt.dispatch(
        &ctx,
        "create-1",
        IntentBody::CreateIdentityProfile {
            label: label.into(),
        },
    );
    rt.flush_net(&ctx);
    ctx
}

/// One side: the bridge Core, its context, and its relationship id with the other person.
struct Side {
    b: Bridge<InsecureTestRng>,
    ctx: Value,
    rel: String,
}

fn connected() -> [Side; 2] {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let mut bob = Runtime::new("bob", &net);
    let a = create_profile(&mut alice, "p-alice", "Alice");
    let b = create_profile(&mut bob, "p-bob", "Bob");
    alice.dispatch(&a, "inv-1", IntentBody::CreateInvite);
    alice.flush_net(&a);
    let invite: Option<InviteView> = alice.query(Query::GetInvite {
        ctx: a.clone(),
        operation_id: "inv-1".into(),
    });
    bob.dispatch(
        &b,
        "acc-1",
        IntentBody::AcceptInvitation {
            link: invite.unwrap().link,
        },
    );
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    let intros: Vec<IntroductionRequest> = alice.query(Query::GetIntroductions { ctx: a.clone() });
    alice.dispatch(
        &a,
        "intro-1",
        IntentBody::AcceptIntroduction {
            request_id: intros[0].id.clone(),
        },
    );
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    let al: Vec<ConversationSummary> = alice.query(Query::GetConversationList { ctx: a.clone() });
    let bl: Vec<ConversationSummary> = bob.query(Query::GetConversationList { ctx: b.clone() });
    let side = |rt: &Runtime, profile: &str, rel: &str, tag: u8| {
        let mut br = Bridge::new(
            move || InsecureTestRng::from_label(&[b'g', tag]),
            || 1_790_000_000_000,
        );
        call(&mut br, json!({ "cmd": "init", "home": home() }));
        let ctx = call(
            &mut br,
            json!({ "cmd": "open_profile", "profileId": profile,
                "records": serde_json::to_value(rt.store.records(profile)).unwrap() }),
        )["ok"]
            .clone();
        Side {
            b: br,
            ctx,
            rel: rel.to_string(),
        }
    };
    [
        side(&alice, "p-alice", &al[0].id, 1),
        side(&bob, "p-bob", &bl[0].id, 2),
    ]
}

impl Side {
    /// `group_do` / `group_receive`: the commit is made durable (the test host has no store to fail), the answer returned.
    fn step(&mut self, cmd: Value) -> Value {
        let r = call(&mut self.b, cmd);
        assert!(r.get("ok").is_some(), "{r}");
        let r = r["ok"].clone();
        if let Some(c) = r.get("commit").filter(|c| !c.is_null()) {
            let f = call(
                &mut self.b,
                json!({ "cmd": "finish_commit", "commitId": c["id"], "ok": true }),
            );
            assert!(f.get("ok").is_some(), "{f}");
        }
        r
    }
    fn intent(&mut self, intent: Value) -> Value {
        let cmd = json!({ "cmd": "group_do", "ctx": self.ctx, "intent": intent });
        self.step(cmd)
    }
    fn recv(&mut self, from: Value, wire: Value) -> Value {
        let cmd = json!({ "cmd": "group_receive", "ctx": self.ctx, "from": from, "wire": wire });
        self.step(cmd)
    }
    fn query(&mut self, q: Value) -> Value {
        call(
            &mut self.b,
            json!({ "cmd": "group_query", "ctx": self.ctx, "query": q }),
        )
    }
}

fn events(rep: &Value) -> Vec<Value> {
    rep["reply"]["events"].as_array().unwrap().clone()
}

fn has(ev: &[(usize, Value)], who: usize, kind: &str) -> bool {
    ev.iter().any(|(i, e)| *i == who && e["kind"] == kind)
}

/// Deliver `rep`'s `out` from side `from` to the other side, recursively. `devs` is `[alice device, bob device]` once
/// known (a device-targeted wire needs the sender's device id, which the transport authenticates).
fn pump(s: &mut [Side; 2], devs: &[String; 2], from: usize, rep: &Value) -> Vec<(usize, Value)> {
    let mut ev: Vec<(usize, Value)> = events(rep).into_iter().map(|e| (from, e)).collect();
    let to = 1 - from;
    for o in rep["reply"]["out"].as_array().unwrap() {
        let sender = if o["to"].get("relationship").is_some() {
            json!({ "relationship": s[to].rel })
        } else {
            json!({ "device": devs[from] })
        };
        let r = s[to].recv(sender, o["wire"].clone());
        ev.extend(pump(s, devs, to, &r));
    }
    ev
}

#[test]
fn group_flow_runs_as_json_commands_between_two_cores() {
    let mut s = connected();
    // Alice creates the group and invites Bob over their Active relationship
    let rel = s[0].rel.clone();
    let rep =
        s[0].intent(json!({ "kind": "create", "name": "Book club", "relationshipIds": [rel] }));
    let none = [String::new(), String::new()];
    assert!(events(&rep).iter().any(|e| e["kind"] == "created"), "{rep}");
    assert_eq!(rep["reply"]["out"][0]["wire"]["t"], "invite");
    let ev = pump(&mut s, &none, 0, &rep);
    assert!(has(&ev, 1, "invite_received"), "{ev:?}");
    let groups = s[0].query(json!({ "kind": "groups" }))["ok"].clone();
    let conv = groups["groups"][0]["conversationId"]
        .as_str()
        .unwrap()
        .to_string();
    let invites = s[1].query(json!({ "kind": "invites" }))["ok"].clone();
    let inv = invites["invites"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(invites["invites"][0]["groupName"], "Book club");
    // Bob accepts; his KeyPackage release names his device
    let rep = s[1].intent(json!({ "kind": "accept", "inviteId": inv }));
    let bob_dev = rep["reply"]["out"][0]["wire"]["releases"][0]["device"]
        .as_str()
        .unwrap()
        .to_string();
    let ev = pump(&mut s, &none, 1, &rep);
    assert!(has(&ev, 0, "commit_prepared"), "{ev:?}");
    assert_eq!(
        s[0].query(json!({ "kind": "group", "conversationId": conv }))["ok"]["group"]["canSend"],
        false,
        "nothing is sent under the unmerged candidate"
    );
    let intent_id = ev
        .iter()
        .find(|(_, e)| e["kind"] == "commit_prepared")
        .unwrap()
        .1["intentId"]
        .as_str()
        .unwrap()
        .to_string();
    s[0].intent(json!({ "kind": "commit_published", "intentId": intent_id }));
    let rep = s[0].intent(json!({ "kind": "commit_accepted", "intentId": intent_id }));
    assert_eq!(rep["reply"]["out"][0]["wire"]["t"], "welcome");
    let dir = rep["reply"]["out"][0]["wire"]["directory"]
        .as_array()
        .unwrap()
        .clone();
    let alice_dev = dir
        .iter()
        .map(|d| d[0].as_str().unwrap().to_string())
        .find(|d| *d != bob_dev)
        .unwrap();
    let devs = [alice_dev, bob_dev];
    let ev = pump(&mut s, &devs, 0, &rep);
    assert!(has(&ev, 1, "joined"), "{ev:?}");
    // text both ways: one wire per other device, never over the relationship
    let rep = s[0].intent(json!({ "kind": "send_text", "conversationId": conv, "text": "hello" }));
    assert_eq!(rep["reply"]["out"][0]["wire"]["t"], "app");
    assert!(rep["reply"]["out"][0]["to"].get("device").is_some());
    let ev = pump(&mut s, &devs, 0, &rep);
    assert!(has(&ev, 1, "message_received"), "{ev:?}");
    let rep = s[1].intent(json!({ "kind": "send_text", "conversationId": conv, "text": "hi A" }));
    pump(&mut s, &devs, 1, &rep);
    for side in s.iter_mut() {
        let m = side.query(json!({ "kind": "messages", "conversationId": conv }));
        let texts: Vec<&str> = m["ok"]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["text"].as_str().unwrap())
            .collect();
        assert_eq!(texts, ["hello", "hi A"]);
        let g = side.query(json!({ "kind": "group", "conversationId": conv }));
        assert_eq!(g["ok"]["group"]["summary"]["members"], 2);
        assert_eq!(g["ok"]["group"]["canSend"], true);
    }
    // a typed refusal and malformed input are answers, not panics
    let r = call(
        &mut s[0].b,
        json!({ "cmd": "group_do", "ctx": s[0].ctx, "intent": { "kind": "decline", "inviteId": "nope" } }),
    );
    assert!(r["ok"]["refused"].is_string(), "{r}");
    let r = call(
        &mut s[0].b,
        json!({ "cmd": "group_do", "ctx": s[0].ctx, "intent": { "kind": "leave", "conversationId": "zz" } }),
    );
    assert!(r.get("err").is_some(), "{r}");
    let r = call(
        &mut s[0].b,
        json!({ "cmd": "group_receive", "ctx": s[0].ctx, "from": { "device": "00" }, "wire": { "t": "nope" } }),
    );
    assert!(r.get("err").is_some(), "{r}");
}
