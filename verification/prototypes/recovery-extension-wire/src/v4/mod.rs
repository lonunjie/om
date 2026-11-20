// SPDX-License-Identifier: Apache-2.0
//! Provisional offline evidence for ADR-0244–0248.

mod controller;
mod kit;
mod record;
mod sheet;

use super::*;
use om_core::crypto::{hex, sha256};
use serde_json::json;

fn vector(id: &str) -> Value {
    let vectors: Value = serde_json::from_str(include_str!(
        "../../../../vectors/recovery-extension-v4.json"
    ))
    .unwrap();
    assert!(vectors[id].is_object(), "missing {id}");
    vectors[id].clone()
}

fn check_bytes(case: &Value, wire: &[u8], expected: &str) {
    assert_eq!(case["size"], wire.len());
    assert_eq!(case["sha256"], hex(&sha256(&[wire])));
    assert_eq!(case["expected"], expected);
}

fn checkpoint(state: &Acs) -> om_core::recovery_sheet::OwnerCheckpoint {
    om_core::recovery_sheet::OwnerCheckpoint {
        account_id: state.account_id,
        generation: state.generation,
        acs_hash: state.hash(),
    }
}
