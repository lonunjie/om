// SPDX-License-Identifier: MPL-2.0
//! `LanFrameV1` per-kind shape (`spec/cddl/lan.cddl`, ADR-0158, ADR-0194). The registry struct `LanFrameV1` lists every
//! field a frame may carry as optional, so it checks types, ranges and sizes but not which fields each kind must have.
//! This module is the second half of the check: after the registry accepts the bytes, a frame of kind K carries exactly
//! the keys the CDDL gives K. A missing key is `missing_field`, a key that belongs to another kind is `unknown_field`.

use crate::cbor::{Class, Value};
use crate::schema::Registry;

/// Key 0 is the version and key 1 the kind; both are on every frame. `(kind, keys after 0 and 1)`.
const KIND_KEYS: &[(u64, &[u64])] = &[
    (1, &[2, 3, 4, 5]),  // object_begin: transfer_id, object_kind, total_bytes, sha256
    (2, &[2, 6, 7]),     // object_chunk: transfer_id, offset, payload
    (3, &[2]),           // object_end: transfer_id
    (4, &[2, 14]),       // ack: transfer_id, highest contiguous offset
    (5, &[2, 8, 9, 10]), // fetch_async_key: request_id, target device, profile, capability
    (6, &[2, 10, 11, 12]), // fetch_blob: request_id, capability, blob id, ciphertext digest
    (7, &[2, 13]),       // error: id, error code
    (8, &[2, 15]),       // object_receipt: transfer_id, receipt_tag (ADR-0194, W1)
];

/// Checks a decoded `LanFrameV1` map against the exact key set of its kind.
pub fn frame_shape(v: &Value) -> Result<(), Class> {
    let Value::Map(m) = v else {
        return Err("wrong_type");
    };
    let key = |k: u64| m.iter().find(|(x, _)| *x == Value::Uint(k)).map(|(_, y)| y);
    let Some(Value::Uint(kind)) = key(1) else {
        return Err("missing_field");
    };
    let Some((_, keys)) = KIND_KEYS.iter().find(|(k, _)| k == kind) else {
        return Err("value_range");
    };
    let allowed = |k: u64| k <= 1 || keys.contains(&k);
    if m.iter()
        .any(|(k, _)| !matches!(k, Value::Uint(k) if allowed(*k)))
    {
        return Err("unknown_field");
    }
    if keys.iter().any(|k| key(*k).is_none()) {
        return Err("missing_field");
    }
    Ok(())
}

impl Registry {
    /// Registry validation of a `LanFrameV1` followed by the per-kind shape check. Use this, not `check_struct`, for frames.
    pub fn check_lan_frame(&self, bytes: &[u8], max_bytes: usize) -> Result<Value, Class> {
        let v = self.check_struct("LanFrameV1", bytes, max_bytes)?;
        frame_shape(&v)?;
        Ok(v)
    }
}
