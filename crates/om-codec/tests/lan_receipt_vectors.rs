// SPDX-License-Identifier: MPL-2.0
//! Replays the ADR-0194 `LanFrameV1` vectors (`verification/vectors/direct-transfer-receipt-vectors.json.gz`, the `structs`
//! list: the new `object_receipt` frame, controls for the earlier frames and refusals) through the product codec as a
//! registered struct: verdict, exact error class and byte-identical re-encoding. A Node decoder, the Rust and Ruby CDDL
//! tools replay the same bytes (`verification/prototypes/direct-receipt/run.sh`).

use flate2::read::GzDecoder;
use om_codec::cbor::{encode, unhex};
use om_codec::schema::Registry;
use serde_json::Value as J;
use std::io::Read;
use std::path::PathBuf;

fn corpus() -> J {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../verification/vectors/direct-transfer-receipt-vectors.json.gz");
    let mut s = String::new();
    GzDecoder::new(&std::fs::read(p).unwrap()[..])
        .read_to_string(&mut s)
        .unwrap();
    serde_json::from_str(&s).unwrap()
}

#[test]
fn lan_frame_vectors_replay_exactly() {
    let reg = Registry::load_w1();
    let doc = corpus();
    let structs = doc["structs"].as_array().unwrap();
    assert!(structs.len() >= 20, "only {} frame vectors", structs.len());
    let mut bad = Vec::new();
    let mut accepted = 0;
    for v in structs {
        let id = v["id"].as_str().unwrap();
        let bytes = unhex(v["hex"].as_str().unwrap());
        match (
            v["expect"].as_str().unwrap(),
            reg.check_struct(v["struct"].as_str().unwrap(), &bytes, 131_068),
        ) {
            ("reject", Err(e)) => {
                if Some(e) != v["error"].as_str() {
                    bad.push(format!("{id}: error {e} != {}", v["error"]));
                }
            }
            ("accept", Ok(val)) => {
                accepted += 1;
                if encode(&val) != bytes {
                    bad.push(format!("{id}: re-encode differs"));
                }
            }
            (want, got) => bad.push(format!(
                "{id}: expected {want}, got {:?}",
                got.map(|_| "accept")
            )),
        }
    }
    assert!(accepted >= 10);
    assert!(bad.is_empty(), "{bad:#?}");
}

/// Kind 8 is a W1 value: a W0 reading of the registry refuses it, so the frozen W0 corpus is untouched.
#[test]
fn kind_8_is_not_a_w0_value() {
    let doc = corpus();
    let receipt = doc["structs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "LanFrameV1/receipt")
        .unwrap();
    let bytes = unhex(receipt["hex"].as_str().unwrap());
    assert!(
        Registry::load_w1()
            .check_struct("LanFrameV1", &bytes, 131_068)
            .is_ok()
    );
    assert_eq!(
        Registry::load()
            .check_struct("LanFrameV1", &bytes, 131_068)
            .map(|_| ()),
        Err("value_range")
    );
}
