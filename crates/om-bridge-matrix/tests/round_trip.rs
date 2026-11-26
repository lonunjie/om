// SPDX-License-Identifier: MPL-2.0
//! The in-process homeserver exchanges only Matrix client-server bytes with the adapter.
use std::collections::{BTreeMap, BTreeSet};

use om_bridge_matrix::*;
use om_codec::{Value as Cbor, cbor::encode};
use om_core::{
    bridge_control::{BridgeChain, BridgeState},
    bridge_fidelity::{
        self as fidelity, FOREIGN_ACTOR_UNVERIFIED as ACTOR, ORIGIN_UNVERIFIED as ORIGIN,
    },
    bridge_provenance::{ImportMapping, RoomProvenance, origin_fingerprint},
    bridge_room::{ADD_BRIDGE, BOTH, BridgeRoom, KIND_BRIDGE, Mapping},
    service_control::{ACTIVE, Endpoint as BridgeEndpoint},
};
use serde_json::{Value, json};

const LEFT: &str = "!left:matrix.test";
const RIGHT: &str = "!right:matrix.test";
const OM_ROOM: [u8; 32] = [42; 32];
const SECRET: [u8; 32] = [43; 32];
const ACTOR_MXID: &str = "@alice:matrix.test";

fn event_id(n: u8) -> EventId {
    [n; 16]
}

fn endpoint() -> Endpoint {
    Endpoint::new("https://matrix.test").unwrap()
}

fn text(body: &str, rich: bool) -> Rendering {
    Rendering::Message {
        message: Message::Text {
            body: body.into(),
            rich,
        },
        relation: None,
    }
}

fn native(n: u8, rendering: Rendering) -> ForeignRendering {
    ForeignRendering {
        event_id: event_id(n),
        epoch: 1,
        author: RenderingAuthor::Native,
        rendering,
    }
}

fn message(body: &str) -> Value {
    json!({"type":"m.room.message", "sender":ACTOR_MXID,
        "content":{"msgtype":"m.text", "body":body}})
}

fn edit(target: &str) -> Value {
    json!({"type":"m.room.message","sender":ACTOR_MXID,
        "content":{"msgtype":"m.text","body":"* Corrected",
            "m.new_content":{"msgtype":"m.text","body":"Corrected"},
            "m.relates_to":{"rel_type":"m.replace","event_id":target}}})
}

fn reaction(target: &str) -> Value {
    json!({"type":"m.reaction","sender":ACTOR_MXID,
        "content":{"m.relates_to":{"rel_type":"m.annotation","event_id":target,"key":"👍"}}})
}

struct Homeserver {
    rooms: BTreeMap<&'static str, Vec<(usize, Value)>>,
    serial: usize,
    sent: BTreeMap<(usize, String), String>,
}

impl Homeserver {
    fn new() -> Self {
        Self {
            rooms: BTreeMap::from([(LEFT, Vec::new()), (RIGHT, Vec::new())]),
            serial: 0,
            sent: BTreeMap::new(),
        }
    }

    fn post(&mut self, room: &str, mut value: Value) -> String {
        self.serial += 1;
        let reference = format!("$event-{}", self.serial);
        value["event_id"] = json!(reference);
        self.rooms.get_mut(room).unwrap().push((self.serial, value));
        reference
    }

    fn event(&self, room: &str, reference: &str) -> &Value {
        self.rooms[room]
            .iter()
            .map(|(_, event)| event)
            .find(|event| event["event_id"] == reference)
            .unwrap()
    }

    fn relay(&mut self, from: &str, to: &str, reference: &str, preserve: bool) -> String {
        let mut copy = self.event(from, reference).clone();
        copy["sender"] = json!("@relay:matrix.test");
        if !preserve {
            copy["content"]
                .as_object_mut()
                .unwrap()
                .remove(INTEROP_FIELD);
        }
        self.post(to, copy)
    }

    fn send(&mut self, device: usize, request: &Request) -> Vec<u8> {
        assert_eq!(request.method, Method::Put);
        let cache_key = (device, request.url.clone());
        if let Some(reference) = self.sent.get(&cache_key) {
            return serde_json::to_vec(&json!({"event_id":reference})).unwrap();
        }
        let tail = request
            .url
            .strip_prefix("https://matrix.test/_matrix/client/v3/rooms/")
            .unwrap();
        let parts: Vec<_> = tail.split('/').collect();
        assert_eq!(parts.len(), 4);
        let room = match parts[0] {
            "%21left%3Amatrix.test" => LEFT,
            "%21right%3Amatrix.test" => RIGHT,
            _ => panic!("unknown test room"),
        };
        let mut content: Value = serde_json::from_slice(&request.body).unwrap();
        let event_type = match parts[1] {
            "send" => parts[2],
            "redact" => {
                let target = parts[2].strip_prefix("%24").unwrap();
                content["redacts"] = json!(format!("${target}"));
                "m.room.redaction"
            }
            _ => panic!("unknown test path"),
        };
        assert!(content.get(INTEROP_FIELD).is_some());
        let reference = self.post(room, json!({
            "type":event_type, "sender":format!("@bridge{device}:matrix.test"), "content":content,
        }));
        self.sent.insert(cache_key, reference.clone());
        serde_json::to_vec(&json!({"event_id":reference})).unwrap()
    }

    fn sync(&self, request: &Request) -> Vec<u8> {
        assert_eq!(request.method, Method::Get);
        let tail = request
            .url
            .strip_prefix("https://matrix.test/_matrix/client/v3/sync")
            .unwrap();
        let since: usize = if tail.is_empty() {
            0
        } else {
            tail.strip_prefix("?since=").unwrap().parse().unwrap()
        };
        let mut joined = serde_json::Map::new();
        for (&room, timeline) in &self.rooms {
            let events: Vec<_> = timeline
                .iter()
                .filter(|(n, _)| *n > since)
                .map(|(_, v)| v)
                .collect();
            joined.insert(
                room.into(),
                json!({"timeline":{"events":events,"limited":false}}),
            );
        }
        serde_json::to_vec(&json!({"next_batch":self.serial.to_string(),"rooms":{"join":joined}}))
            .unwrap()
    }
}

struct Peer {
    adapter: MatrixAdapter,
    binding: Binding,
    mapping: ImportMapping,
    chain: BridgeChain,
    cursor: Option<String>,
    path_preserves_metadata: bool,
}

impl Peer {
    fn admit(room: &mut BridgeRoom, id: u8, foreign: &str) -> Self {
        let bridge = [id; 32];
        let endpoint_id = vec![id; 16];
        let policy = Binding {
            incarnation: [id; 16],
            room: OM_ROOM,
            matrix_room: foreign.into(),
            bridge,
            endpoint_id: endpoint_id.clone(),
            admitted_epoch: 1,
        };
        let ep = BridgeEndpoint {
            id: endpoint_id.clone(),
            state: ACTIVE,
            profile: 1,
            key: vec![id; 40],
            mode: 0,
            host_ref: None,
        };
        let controller = (1, vec![99; 32]);
        let head = BridgeState {
            service_id: bridge,
            version: 1,
            prev: None,
            controllers: [controller.clone()].into(),
            threshold: 1,
            endpoints: [(endpoint_id, ep)].into(),
            hash: [id; 32],
        };
        let mut chain = BridgeChain::new(bridge);
        chain.accept(head.clone(), &[controller]).unwrap();
        room.admit(
            &caps(),
            &head,
            Mapping {
                incarnation: policy.incarnation,
                namespace: NAMESPACE.into(),
                allowed_classes: [1, 2, 3, 4, 8].into(),
                direction: BOTH,
                active: true,
            },
            &[],
            1,
        )
        .unwrap();
        Self {
            adapter: MatrixAdapter::new(endpoint(), policy.clone()).unwrap(),
            binding: policy,
            chain,
            cursor: None,
            path_preserves_metadata: true,
            mapping: ImportMapping {
                bridge,
                room: OM_ROOM.to_vec(),
                namespace: NAMESPACE.into(),
                foreign_scope: foreign.into(),
                direction: BOTH,
                active: true,
            },
        }
    }
}

fn caps() -> BTreeSet<String> {
    [ADD_BRIDGE.to_string()].into()
}

fn access<'a>(
    room: &'a BridgeRoom,
    mapping: &'a ImportMapping,
    epoch: u64,
    preserves: bool,
) -> RoomAccess<'a> {
    RoomAccess {
        room,
        mapping,
        epoch,
        now: 1,
        observed_at: 1,
        matrix_plaintext_allowed: true,
        edits_allowed: true,
        relay_bridged: false,
        path_preserves_metadata: preserves,
    }
}

struct Materialized {
    id: EventId,
    foreign_ref: String,
    actor: String,
    author: BridgeId,
    operation: ImportedAction,
    trace: InteropTrace,
    flags: Vec<u8>,
}

struct Harness {
    room: BridgeRoom,
    provenance: RoomProvenance,
    peers: [Peer; 2],
    server: Homeserver,
    imported: Vec<Materialized>,
    diagnostics: Vec<ImportDrop>,
    epoch: u64,
}

impl Harness {
    fn new(second_scope: &str) -> Self {
        let mut room = BridgeRoom::new(None, 20, 60);
        let peers = [
            Peer::admit(&mut room, 11, LEFT),
            Peer::admit(&mut room, 12, second_scope),
        ];
        Self {
            room,
            provenance: RoomProvenance::new(SECRET),
            peers,
            server: Homeserver::new(),
            imported: Vec::new(),
            diagnostics: Vec::new(),
            epoch: 1,
        }
    }

    fn prepare(&self, i: usize, source: &ForeignRendering) -> Result<PreparedEmission, ExportDrop> {
        let peer = &self.peers[i];
        let access = access(
            &self.room,
            &peer.mapping,
            self.epoch,
            peer.path_preserves_metadata,
        );
        peer.adapter.emit(source, &access, &SECRET)
    }

    fn publish(&mut self, i: usize, prepared: &PreparedEmission) -> (String, Vec<u8>) {
        let peer = &mut self.peers[i];
        let access = access(
            &self.room,
            &peer.mapping,
            self.epoch,
            peer.path_preserves_metadata,
        );
        let request = peer.adapter.request(prepared, &access).unwrap();
        let response = self.server.send(i, request);
        let reference = peer.adapter.confirm_emit(prepared, 200, &response).unwrap();
        (reference, response)
    }

    fn collect(&mut self, i: usize) -> usize {
        let peer = &mut self.peers[i];
        let request = endpoint().sync_request(peer.cursor.as_deref()).unwrap();
        let response = self.server.sync(&request);
        let batch = parse_sync_response(200, &response, &peer.binding.matrix_room).unwrap();
        assert!(!batch.limited);
        peer.cursor = Some(batch.next_batch);
        let event_count = batch.events.len();
        let access = access(
            &self.room,
            &peer.mapping,
            self.epoch,
            peer.path_preserves_metadata,
        );
        for event in batch.events {
            let event = event.unwrap();
            let id = event_id(100 + u8::try_from(self.imported.len()).unwrap());
            let candidate = match peer.adapter.prepare_import(
                &event,
                id,
                &access,
                &self.provenance,
                &SECRET,
                None,
            ) {
                Ok(candidate) => candidate,
                Err(drop) => {
                    self.diagnostics.push(drop);
                    continue;
                }
            };
            let body = event_body(&peer.binding, id, &candidate);
            let checked = peer
                .adapter
                .check_import(&candidate, &body, &access, &peer.chain, &self.provenance)
                .unwrap();
            self.imported.push(Materialized {
                id,
                author: candidate.author(),
                foreign_ref: event.event_ref,
                actor: event.actor_ref,
                operation: candidate.operation().clone(),
                trace: candidate.trace().clone(),
                flags: candidate.flags().to_vec(),
            });
            peer.adapter
                .commit_import(checked, &mut self.provenance)
                .unwrap();
        }
        event_count
    }

    fn drain(&mut self) {
        for _ in 0..24 {
            let before = self.imported.len();
            let seen = self.collect(0) + self.collect(1);
            for imported in &self.imported[before..] {
                let rendering = ForeignRendering {
                    event_id: imported.id,
                    epoch: self.epoch,
                    author: RenderingAuthor::Bridged {
                        bridge: imported.author,
                        trace: imported.trace.clone(),
                        origin_verified: !imported.flags.contains(&ORIGIN),
                    },
                    rendering: text("Room retained the import", false),
                };
                for i in 0..2 {
                    assert!(matches!(
                        self.prepare(i, &rendering),
                        Err(ExportDrop::OwnImport | ExportDrop::RelayDenied)
                    ));
                }
            }
            if seen == 0 {
                assert!(self.server.serial + self.imported.len() <= 24);
                return;
            }
        }
        panic!("bridge loop did not reach quiescence");
    }

    fn import(&self, reference: &str) -> &Materialized {
        self.imported
            .iter()
            .find(|event| event.foreign_ref == reference)
            .unwrap()
    }
}

fn map(fields: impl IntoIterator<Item = (u64, Cbor)>) -> Cbor {
    Cbor::Map(
        fields
            .into_iter()
            .map(|(k, v)| (Cbor::Uint(k), v))
            .collect(),
    )
}

fn rich(body: &str) -> Cbor {
    let inline = map([(0, Cbor::Uint(1)), (1, Cbor::Text(body.into()))]);
    let block = map([(0, Cbor::Uint(1)), (1, Cbor::Array(vec![inline]))]);
    map([(0, Cbor::Uint(1)), (1, Cbor::Array(vec![block]))])
}

fn message_body(body: &str, relation: Option<Relation<EventId>>) -> Cbor {
    let mut fields = vec![(0, rich(body)), (1, Cbor::Array(vec![]))];
    match relation {
        Some(Relation::Reply(id)) => fields.push((2, Cbor::Bytes(id.to_vec()))),
        Some(Relation::Thread(id)) => fields.push((5, Cbor::Bytes(id.to_vec()))),
        None => {}
    }
    map(fields)
}

fn event_body(binding: &Binding, id: EventId, import: &PreparedImport) -> Vec<u8> {
    let (kind, body) = match import.operation() {
        ImportedAction::Message {
            message: Message::Text { body, .. },
            relation,
        } => (1, message_body(body, *relation)),
        ImportedAction::Edit { target, body } => {
            (2, map([(0, Cbor::Bytes(target.to_vec())), (1, rich(body))]))
        }
        ImportedAction::Reaction { target, key } => (
            4,
            map([
                (0, Cbor::Bytes(target.to_vec())),
                (1, Cbor::Text(key.clone())),
                (2, Cbor::Bool(true)),
            ]),
        ),
        ImportedAction::UnavailableTarget { .. } => (1, message_body("Target unavailable", None)),
        ImportedAction::RedactionRequest { .. } => (1, message_body("Redaction requested", None)),
        ImportedAction::Message {
            message: Message::Attachment(_),
            ..
        } => panic!("this model has no media store"),
    };
    encode(&map([
        (0, Cbor::Uint(10)),
        (1, Cbor::Uint(1)),
        (2, Cbor::Uint(1)),
        (3, Cbor::Bytes(id.to_vec())),
        (4, Cbor::Bytes(OM_ROOM.to_vec())),
        (
            5,
            map([
                (0, Cbor::Uint(KIND_BRIDGE)),
                (1, Cbor::Bytes(import.author().to_vec())),
            ]),
        ),
        (6, Cbor::Bytes(binding.endpoint_id.clone())),
        (
            7,
            map([
                (0, Cbor::Uint(1000)),
                (1, Cbor::Uint(0)),
                (2, Cbor::Bytes(binding.endpoint_id.clone())),
            ]),
        ),
        (8, Cbor::Uint(kind)),
        (9, Cbor::Uint(1)),
        (11, body),
        (14, import.provenance().clone()),
    ]))
}

#[test]
fn round_trip_retries_once_and_stripped_own_events_are_echoes() {
    let mut h = Harness::new(RIGHT);
    let om = native(1, text("Hello", true));
    let a = h.prepare(0, &om).unwrap();
    let b = h.prepare(1, &om).unwrap();
    assert_eq!(a.flags(), &[fidelity::LOSSY_FORMATTING]);
    assert_ne!(a.transaction_id(), b.transaction_id());
    assert_eq!(
        a.transaction_id(),
        h.prepare(0, &om).unwrap().transaction_id()
    );

    // Lost HTTP response. Restore the exact request and retry on the same device.
    let peer = &h.peers[0];
    let scope = access(&h.room, &peer.mapping, h.epoch, true);
    let first_request = peer.adapter.request(&a, &scope).unwrap();
    let lost_ack = h.server.send(0, first_request);
    let retry = serde_json::from_slice(&serde_json::to_vec(&a).unwrap()).unwrap();
    let (left, retry_ack) = h.publish(0, &retry);
    assert_eq!(lost_ack, retry_ack);
    let (right, _) = h.publish(1, &b);
    assert_eq!(h.server.serial, 2);
    let encoded = &h.server.event(LEFT, &left)["content"];
    assert_eq!(encoded["body"], "Hello");
    assert!(encoded.get("formatted_body").is_none());
    assert_eq!(encoded[INTEROP_FIELD]["trace"]["hop_count"], 1);

    // Restart the adapter with private target/echo state before /sync.
    let peer = &mut h.peers[0];
    let records = serde_json::to_vec(&peer.adapter.records().cloned().collect::<Vec<_>>()).unwrap();
    let mut restored = MatrixAdapter::new(endpoint(), peer.binding.clone()).unwrap();
    restored
        .restore_records(&peer.binding, serde_json::from_slice(&records).unwrap())
        .unwrap();
    peer.adapter = restored;
    h.server.rooms.get_mut(LEFT).unwrap()[0].1["content"]
        .as_object_mut()
        .unwrap()
        .remove(INTEROP_FIELD);
    let reply = h.server.post(LEFT, json!({
        "type":"m.room.message","sender":ACTOR_MXID,
        "content":{"msgtype":"m.text","body":"Reply", "m.relates_to":{"m.in_reply_to":{"event_id":left}}}
    }));
    h.drain();
    assert_eq!(h.diagnostics, [ImportDrop::Echo, ImportDrop::Echo]);
    assert_eq!(h.server.event(RIGHT, &right)["content"]["body"], "Hello");
    assert_eq!(h.imported.len(), 1);
    let got = h.import(&reply);
    assert_eq!(got.author, h.peers[0].binding.bridge);
    assert_eq!(got.actor, ACTOR_MXID);
    assert_eq!(got.flags, [ACTOR]);
    assert!(matches!(got.operation, ImportedAction::Message {
        relation: Some(Relation::Reply(id)), ..
    } if id == event_id(1)));
}

#[test]
fn reflection_with_trace_is_dropped_even_with_a_fresh_foreign_id() {
    let mut h = Harness::new(RIGHT);
    let emission = h.prepare(0, &native(1, text("Origin", false))).unwrap();
    let (id, _) = h.publish(0, &emission);
    let fresh = h.server.relay(LEFT, LEFT, &id, true);
    assert_ne!(fresh, id);
    h.drain();
    assert_eq!(h.diagnostics, [ImportDrop::Echo, ImportDrop::Reflection]);
    assert!(h.imported.is_empty());
}

#[test]
fn a_metadata_stripping_relay_materializes_only_a_flagged_observed_leg() {
    let mut h = Harness::new(RIGHT);
    h.peers[1].path_preserves_metadata = false;
    let emission = h.prepare(0, &native(1, text("Origin", false))).unwrap();
    let (left, _) = h.publish(0, &emission);
    let right = h.server.relay(LEFT, RIGHT, &left, false);
    h.drain();
    assert_eq!(h.imported.len(), 1);
    let got = h.import(&right);
    assert_eq!(got.flags, [ACTOR, ORIGIN]);
    let observed_fp = origin_fingerprint(&SECRET, "matrix", &right);
    assert_eq!(got.trace.origin_fingerprint, observed_fp);
    assert!(!h.provenance.is_materialized(&observed_fp));
}

#[test]
fn two_bridges_deduplicate_only_a_recomputable_fingerprint() {
    let mut h = Harness::new(LEFT);
    let source = h.server.post(LEFT, message("Same text"));
    let other = h.server.post(LEFT, message("Same text"));
    h.drain();
    assert_eq!(h.imported.len(), 2);
    assert_eq!(
        h.diagnostics,
        [ImportDrop::Duplicate, ImportDrop::Duplicate]
    );
    for id in [source, other] {
        let got = h.import(&id);
        assert_eq!(got.flags, [ACTOR]);
        assert!(h.provenance.is_materialized(&got.trace.origin_fingerprint));
    }
}

#[test]
fn fresh_relay_id_cannot_borrow_a_verified_key_and_r1_refuses_last_hop() {
    let mut h = Harness::new(RIGHT);
    let source = h.server.post(LEFT, message("Origin"));
    h.drain();
    let original = h.import(&source).trace.clone();
    // Model a relay preserving lineage while assigning a fresh Matrix event ID.
    let metadata = InteropMetadata {
        trace: original.clone(),
        fidelity_flags: vec![fidelity::EXACT],
    };
    let mut relay = message("Origin");
    relay["content"][INTEROP_FIELD] = serde_json::to_value(metadata).unwrap();
    let relayed_id = h.server.post(RIGHT, relay.clone());
    let mut spent = original;
    spent.traversed_bridge_ids.push([77; 32]);
    spent.hop_count = 2;
    relay["content"][INTEROP_FIELD]["trace"] = serde_json::to_value(spent).unwrap();
    let exhausted_id = h.server.post(RIGHT, relay);
    h.drain();
    assert_eq!(h.imported.len(), 2);
    let got = h.import(&relayed_id);
    assert_eq!(got.flags, [ACTOR, ORIGIN]);
    assert_eq!(got.trace.hop_count, 2);
    assert_eq!(
        got.trace.origin_fingerprint,
        origin_fingerprint(&SECRET, NAMESPACE, &relayed_id)
    );
    assert!(!h.provenance.is_materialized(&got.trace.origin_fingerprint));
    assert!(
        h.imported
            .iter()
            .all(|event| event.foreign_ref != exhausted_id)
    );
    assert_eq!(h.diagnostics, [ImportDrop::HopLimit]);
}

#[test]
fn edit_reaction_and_redaction_only_resolve_their_exact_targets() {
    let mut h = Harness::new(RIGHT);
    let source = h.server.post(LEFT, message("Original"));
    let mapped_edit = h.server.post(LEFT, edit(&source));
    let mapped_reaction = h.server.post(LEFT, reaction(&source));
    let missing_edit = h.server.post(LEFT, edit("$unknown"));
    let missing_reaction = h.server.post(LEFT, reaction("$unknown"));
    let mut spoof = edit(&source);
    spoof["sender"] = json!("@another:matrix.test");
    let spoofed = h.server.post(LEFT, spoof);
    h.drain();

    let target = h.import(&source).id;
    let edited = h.import(&mapped_edit);
    assert_eq!(
        edited.operation,
        ImportedAction::Edit {
            target,
            body: "Corrected".into()
        }
    );
    assert_eq!(edited.flags, [ACTOR]);
    let reacted = h.import(&mapped_reaction);
    assert_eq!(
        reacted.operation,
        ImportedAction::Reaction {
            target,
            key: "👍".into()
        }
    );
    assert_eq!(reacted.flags, [ACTOR]);
    let fallback = h.import(&missing_edit);
    assert_eq!(
        fallback.flags,
        [
            fidelity::REPLY_TARGET_UNAVAILABLE,
            fidelity::EDIT_AS_NEW_MESSAGE,
            ACTOR
        ]
    );
    assert!(
        matches!(&fallback.operation, ImportedAction::Message { message: Message::Text { body, .. }, relation: None }
        if body == "Edited version: Corrected")
    );
    assert_eq!(
        h.import(&missing_reaction).flags,
        [fidelity::REPLY_TARGET_UNAVAILABLE, ACTOR]
    );
    assert_eq!(
        h.import(&missing_reaction).operation,
        ImportedAction::UnavailableTarget {
            kind: EventKind::Reaction
        }
    );
    assert_eq!(
        h.import(&spoofed).flags,
        [fidelity::EDIT_AS_NEW_MESSAGE, ACTOR]
    );

    let (base, _) = h.publish(
        1,
        &h.prepare(1, &native(1, text("OM source", false))).unwrap(),
    );
    let actions = [
        (
            Rendering::Edit {
                target: event_id(1),
                body: "Updated".into(),
                rich: false,
            },
            "m.room.message",
            "m.replace",
        ),
        (
            Rendering::Reaction {
                target: event_id(1),
                key: "👍".into(),
            },
            "m.reaction",
            "m.annotation",
        ),
        (
            Rendering::Message {
                message: Message::Text {
                    body: "Thread".into(),
                    rich: false,
                },
                relation: Some(Relation::Thread(event_id(1))),
            },
            "m.room.message",
            "m.thread",
        ),
    ];
    for (i, (rendering, event_type, relation)) in actions.into_iter().enumerate() {
        let prepared = h.prepare(1, &native(2 + i as u8, rendering)).unwrap();
        assert_eq!(prepared.flags(), &[fidelity::EXACT]);
        let (id, _) = h.publish(1, &prepared);
        let event = h.server.event(RIGHT, &id);
        assert_eq!(event["type"], event_type);
        assert_eq!(event["content"]["m.relates_to"]["rel_type"], relation);
        assert_eq!(event["content"]["m.relates_to"]["event_id"], base);
    }
    for missing in [
        Rendering::Edit {
            target: event_id(9),
            body: "Missing".into(),
            rich: false,
        },
        Rendering::Reaction {
            target: event_id(9),
            key: "👍".into(),
        },
    ] {
        assert!(
            matches!(h.prepare(1, &native(5, missing)), Err(ExportDrop::Fidelity(flags))
            if flags == [fidelity::REPLY_TARGET_UNAVAILABLE])
        );
    }
    let redact = h
        .prepare(
            1,
            &native(
                6,
                Rendering::Redact {
                    target: event_id(1),
                    reason: None,
                },
            ),
        )
        .unwrap();
    assert_eq!(redact.flags(), &[fidelity::EXACT]);
    let (redaction, _) = h.publish(1, &redact);
    assert_eq!(
        h.server.event(RIGHT, &redaction)["content"]["redacts"],
        base
    );
    assert_eq!(h.server.event(RIGHT, &base)["content"]["body"], "OM source");
    let requested = h.server.post(
        LEFT,
        json!({"type":"m.room.redaction","sender":ACTOR_MXID,
        "content":{"redacts":source,"reason":"Requested"}}),
    );
    h.drain();
    assert_eq!(
        h.import(&requested).operation,
        ImportedAction::RedactionRequest {
            target,
            reason: Some("Requested".into()),
        }
    );
    assert_eq!(h.import(&requested).flags, [ACTOR]);
}

#[test]
fn removal_refuses_pending_retries_future_events_and_inbound_sync() {
    let mut h = Harness::new(RIGHT);
    let om = native(1, text("Prepared prior to removal", false));
    let pending = h.prepare(1, &om).unwrap();
    let historical = ForeignRendering {
        epoch: 0,
        ..native(2, text("Before admission", false))
    };
    assert!(matches!(
        h.prepare(1, &historical),
        Err(ExportDrop::OutsideMembership)
    ));
    let bridge = h.peers[1].binding.bridge;
    h.room.remove(&caps(), &bridge).unwrap();
    h.epoch += 1;
    let peer = &h.peers[1];
    let context = access(&h.room, &peer.mapping, h.epoch, true);
    assert!(matches!(
        peer.adapter.request(&pending, &context),
        Err(ExportDrop::Inactive)
    ));
    let future = ForeignRendering {
        epoch: h.epoch,
        ..native(3, text("After removal", false))
    };
    assert!(matches!(h.prepare(1, &future), Err(ExportDrop::Inactive)));
    h.server.post(RIGHT, message("Arrives after removal"));
    h.drain();
    assert_eq!(h.diagnostics, [ImportDrop::Inactive]);
    assert!(h.imported.is_empty());
    assert_eq!(h.server.serial, 1);
}

#[test]
fn opt_in_relay_needs_preserving_path_and_carries_full_lineage() {
    let mut h = Harness::new(RIGHT);
    let reference = h.server.post(LEFT, message("Foreign origin"));
    h.drain();
    let imported = h.import(&reference);
    let (from_id, from_author, from_trace) = (imported.id, imported.author, imported.trace.clone());
    let input = ForeignRendering {
        event_id: from_id,
        epoch: h.epoch,
        author: RenderingAuthor::Bridged {
            bridge: from_author,
            trace: from_trace.clone(),
            origin_verified: true,
        },
        rendering: text("Foreign origin", false),
    };
    let peer = &h.peers[1];
    let mut context = access(&h.room, &peer.mapping, h.epoch, false);
    context.relay_bridged = true;
    assert!(matches!(
        peer.adapter.emit(&input, &context, &SECRET),
        Err(ExportDrop::RelayDenied)
    ));
    context.path_preserves_metadata = true;
    let prepared = peer.adapter.emit(&input, &context, &SECRET).unwrap();
    let request = peer.adapter.request(&prepared, &context).unwrap();
    let bytes = h.server.send(1, request);
    let foreign_id = parse_event_response(200, &bytes).unwrap();
    let content = &h.server.event(RIGHT, &foreign_id)["content"];
    let meta: InteropMetadata = serde_json::from_value(content[INTEROP_FIELD].clone()).unwrap();
    assert_eq!(meta.trace.origin_fingerprint, from_trace.origin_fingerprint);
    assert_eq!(
        meta.trace.traversed_bridge_ids,
        [from_author, peer.binding.bridge]
    );
    assert_eq!(meta.trace.hop_count, 2);
    assert_eq!(meta.trace.hop_limit, 3);
    assert_eq!(meta.fidelity_flags, [fidelity::EXACT]);
    h.peers[1]
        .adapter
        .confirm_emit(&prepared, 200, &bytes)
        .unwrap();
    h.server.relay(RIGHT, LEFT, &foreign_id, true);
    h.drain();
    assert_eq!(h.imported.len(), 1);
    assert_eq!(h.diagnostics, [ImportDrop::Reflection, ImportDrop::Echo]);
}

#[test]
fn attachment_and_poll_summary_send_as_inert_content() {
    let mut h = Harness::new(RIGHT);
    let file = h
        .prepare(
            0,
            &native(
                1,
                Rendering::Message {
                    message: Message::Attachment(Attachment {
                        kind: AttachmentKind::File,
                        name: "notes.txt".into(),
                        caption: Some("Meeting notes".into()),
                        mxc: "mxc://matrix.test/opaque".into(),
                        bytes: Some(23),
                    }),
                    relation: None,
                },
            ),
        )
        .unwrap();
    assert_eq!(file.flags(), &[fidelity::EXACT]);
    let (file_id, _) = h.publish(0, &file);
    let content = &h.server.event(LEFT, &file_id)["content"];
    assert_eq!(content["filename"], "notes.txt");
    assert_eq!(content["body"], "Meeting notes");
    assert_eq!(content["url"], "mxc://matrix.test/opaque");

    let poll = h
        .prepare(
            0,
            &native(
                2,
                Rendering::PollSummary {
                    body: "Choose a date".into(),
                },
            ),
        )
        .unwrap();
    assert_eq!(poll.flags(), &[fidelity::LOSSY_FORMATTING]);
    let (poll_id, _) = h.publish(0, &poll);
    let event = h.server.event(LEFT, &poll_id);
    assert_eq!(event["type"], "m.room.message");
    assert_eq!(
        event["content"]["body"],
        "Poll summary (read-only): Choose a date"
    );
    h.drain();
    assert!(h.imported.is_empty());
}
