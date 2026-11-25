// SPDX-License-Identifier: MPL-2.0
//! `local_delivery` slice 6 (wire half): the typed frames agree with the frozen two-codec corpus. Every accepted corpus
//! frame of a transfer kind decodes and re-encodes to the frozen bytes; every refused one stays refused.
use om_codec::cbor::{Value, encode, unhex};
use om_codec::schema::Registry;
use om_core::lan_frame::*;
use serde_json::Value as J;
use std::path::PathBuf;

fn corpus() -> J {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../verification/prototypes/v024-lan-wire/vectors.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}
fn to_value(x: &J) -> Value {
    match x.as_u64() {
        Some(n) => Value::Uint(n),
        None => Value::Bytes(unhex(x["hex"].as_str().unwrap())),
    }
}
fn bytes_of(pairs: &J) -> Vec<u8> {
    let mut m: Vec<(Value, Value)> = pairs
        .as_array()
        .unwrap()
        .iter()
        .map(|kv| (to_value(&kv[0]), to_value(&kv[1])))
        .collect();
    m.sort_by_key(|(k, _)| match k {
        Value::Uint(n) => *n,
        _ => 0,
    });
    encode(&Value::Map(m))
}

#[test]
fn corpus_frames_decode_and_re_encode_exactly() {
    let reg = Registry::load_w1();
    let doc = corpus();
    let (mut transfer, mut requests) = (0, 0);
    for v in doc["positives"].as_array().unwrap() {
        if v["type"] != "frame" {
            continue;
        }
        let bytes = bytes_of(&v["value"]);
        let f = decode_frame(&reg, &bytes).unwrap_or_else(|e| panic!("{}: {e}", v["id"]));
        let again = encode_frame(&f).expect("every frame kind encodes");
        assert_eq!(again, bytes, "{}", v["id"]);
        if matches!(f, Frame::FetchAsyncKey { .. } | Frame::FetchBlob { .. }) {
            requests += 1;
        } else {
            transfer += 1;
        }
    }
    assert!(transfer >= 4, "corpus shrank: {transfer} transfer frames");
    assert!(
        requests >= 2,
        "corpus has no fetch request vectors: {requests}"
    );
}

#[test]
fn corpus_negative_frames_are_refused() {
    let reg = Registry::load_w1();
    let doc = corpus();
    let mut n = 0;
    for v in doc["negatives"].as_array().unwrap() {
        if v["type"] != "frame" {
            continue;
        }
        assert!(
            decode_frame(&reg, &bytes_of(&v["value"])).is_err(),
            "{}",
            v["id"]
        );
        n += 1;
    }
    assert!(n >= 3);
}

#[test]
fn every_transfer_frame_round_trips_and_hostile_bytes_never_panic() {
    let reg = Registry::load_w1();
    let t = [7u8; 16];
    for f in [
        Frame::Begin {
            transfer_id: t,
            object_kind: 8,
            total_bytes: 40_000,
            sha256: [9; 32],
        },
        Frame::Chunk {
            transfer_id: t,
            offset: 16_384,
            bytes: vec![1; 16_384],
        },
        Frame::End { transfer_id: t },
        Frame::Ack {
            transfer_id: t,
            offset: 40_000,
        },
        Frame::Error { id: t, code: 2 },
        Frame::Receipt {
            transfer_id: t,
            tag: [3; 32],
        },
    ] {
        let b = encode_frame(&f).unwrap();
        assert_eq!(decode_frame(&reg, &b).unwrap(), f);
    }
    // An oversize chunk and a truncated or flipped frame are refused, never accepted or panicking.
    let too_big = encode_frame(&Frame::Chunk {
        transfer_id: t,
        offset: 0,
        bytes: vec![1; 16_385],
    })
    .unwrap();
    assert!(decode_frame(&reg, &too_big).is_err());
    let good = encode_frame(&Frame::Ack {
        transfer_id: t,
        offset: 5,
    })
    .unwrap();
    for i in 0..good.len() {
        let mut x = good.clone();
        x[i] ^= 0xff;
        let _ = decode_frame(&reg, &x);
        let _ = decode_frame(&reg, &good[..i]);
    }
}
