// SPDX-License-Identifier: MPL-2.0
//! `bridges` slice 2: provenance and loop engine against the ADR-0264 vectors (V-013 scenarios on product code):
//! per-Room dedup only on a matching verified fingerprint, mapping binding, unverified origins never becoming dedup keys,
//! and the Room secret as a real keyed input.
use om_codec::cbor::{encode, unhex};
use om_codec::{Limits, Value, decode};
use om_core::bridge_provenance::*;

const BRIDGE: [u8; 32] = [0x33; 32];
const SECRET: [u8; 32] = [0xa0; 32];

fn vector(id: &str) -> Vec<u8> {
    let d: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../verification/prototypes/bridges-wire/vectors.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let v = d["positive"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .unwrap_or_else(|| panic!("no vector {id}"));
    unhex(v["wire_hex"].as_str().unwrap())
}
fn mapping() -> ImportMapping {
    ImportMapping {
        bridge: BRIDGE,
        room: vec![0x44; 32],
        namespace: "matrix".into(),
        foreign_scope: "!room:server".into(),
        direction: 3,
        active: true,
    }
}
fn event() -> Vec<u8> {
    vector("ok-import-with-head-and-mapping")
}
/// Replaces `path` (a chain of map keys) in an event body and re-encodes it.
fn edit(wire: &[u8], path: &[u64], new: Value) -> Vec<u8> {
    fn go(v: &mut Value, path: &[u64], new: Value) {
        let Value::Map(m) = v else {
            panic!("not a map")
        };
        let slot = m
            .iter_mut()
            .find(|(k, _)| *k == Value::Uint(path[0]))
            .unwrap();
        if path.len() == 1 {
            slot.1 = new;
        } else {
            go(&mut slot.1, &path[1..], new);
        }
    }
    let mut v = decode(wire, Limits::OM, wire.len()).unwrap();
    go(&mut v, path, new);
    encode(&v)
}

#[test]
fn the_fingerprint_is_the_keyed_hmac_a_second_codec_produced() {
    // the vector's fingerprint came from the Python prototype with this Room secret
    let fp = origin_fingerprint(&SECRET, "matrix", "ev-0001");
    let e = RoomProvenance::new(SECRET);
    assert_eq!(
        e.check_import(&event(), &mapping()).unwrap().fingerprint,
        fp
    );
    // another Room's secret, event or namespace gives another fingerprint (no cross-Room linkability)
    assert_ne!(fp, origin_fingerprint(&[0xa1; 32], "matrix", "ev-0001"));
    assert_ne!(fp, origin_fingerprint(&SECRET, "matrix", "ev-0002"));
    assert_ne!(fp, origin_fingerprint(&SECRET, "telegram", "ev-0001"));
}

#[test]
fn a_second_bridge_importing_the_same_origin_event_is_deduplicated_per_room() {
    let mut room = RoomProvenance::new(SECRET);
    let v = room.check_import(&event(), &mapping()).unwrap();
    assert!(v.verified);
    room.commit(&v);
    assert!(room.is_materialized(&v.fingerprint));
    // the same origin event through the same or another bridge in the Room is refused
    assert_eq!(
        room.check_import(&event(), &mapping()),
        Err(ImportRefusal::Duplicate)
    );
}

#[test]
fn dedup_needs_a_matching_fingerprint_never_text_or_name() {
    let mut room = RoomProvenance::new(SECRET);
    let v = room.check_import(&event(), &mapping()).unwrap();
    room.commit(&v);
    // identical text, name and time, different foreign event ref: a different (recomputed) fingerprint is required, so a
    // copied fingerprint with another ref is a mismatch rather than a silent merge
    let other_ref = edit(&event(), &[14, 2], Value::Text("ev-0002".into()));
    assert_eq!(
        room.check_import(&other_ref, &mapping()),
        Err(ImportRefusal::FingerprintMismatch)
    );
    // a different Room's secret does not recognise the event at all
    let foreign = RoomProvenance::new([0xa1; 32]);
    assert_eq!(
        foreign.check_import(&event(), &mapping()),
        Err(ImportRefusal::FingerprintMismatch)
    );
}

#[test]
fn the_mapping_binds_author_room_namespace_scope_and_direction() {
    let room = RoomProvenance::new(SECRET);
    let mut m = mapping();
    m.bridge = [0x34; 32];
    assert_eq!(
        room.check_import(&event(), &m),
        Err(ImportRefusal::WrongAuthor)
    );
    for tweak in [
        |m: &mut ImportMapping| m.active = false,
        |m: &mut ImportMapping| m.direction = 1,
        |m: &mut ImportMapping| m.room = vec![0x45; 32],
        |m: &mut ImportMapping| m.namespace = "signal".into(),
        |m: &mut ImportMapping| m.foreign_scope = "!other:server".into(),
    ] {
        let mut m = mapping();
        tweak(&mut m);
        assert_eq!(
            room.check_import(&event(), &m),
            Err(ImportRefusal::MappingMismatch)
        );
    }
}

#[test]
fn an_unverifiable_origin_must_say_so_and_never_becomes_a_dedup_key() {
    let mut room = RoomProvenance::new(SECRET);
    // the origin leg is another network, so the fingerprint cannot be recomputed here
    let far = |flags: Vec<Value>| {
        let mut e = edit(&event(), &[14, 9, 0], Value::Text("telegram".into()));
        e = edit(&e, &[14, 7], Value::Array(flags));
        e
    };
    assert_eq!(
        room.check_import(&far(vec![Value::Uint(2)]), &mapping()),
        Err(ImportRefusal::MissingUnverifiedFlag)
    );
    let v = room
        .check_import(&far(vec![Value::Uint(9)]), &mapping())
        .unwrap();
    assert!(!v.verified);
    room.commit(&v);
    assert!(!room.is_materialized(&v.fingerprint));
    // so the same unverified event is imported again (and flagged) instead of being merged on weak evidence
    assert!(
        room.check_import(&far(vec![Value::Uint(9)]), &mapping())
            .is_ok()
    );
}

#[test]
fn loops_and_exhausted_hops_never_reach_the_engine() {
    // the wire refuses a repeated bridge, a trace not ending in the author, and hop_count at the limit
    let room = RoomProvenance::new(SECRET);
    let twice = Value::Array(vec![
        Value::Bytes(BRIDGE.to_vec()),
        Value::Bytes(BRIDGE.to_vec()),
    ]);
    let looped = edit(&event(), &[14, 9, 2], twice);
    assert_eq!(
        room.check_import(&looped, &mapping()),
        Err(ImportRefusal::Malformed)
    );
    let spent = edit(&event(), &[14, 9, 4], Value::Uint(1));
    assert_eq!(
        room.check_import(&spent, &mapping()),
        Err(ImportRefusal::Malformed)
    );
    assert_eq!(DEFAULT_HOP_LIMIT, 3);
}
