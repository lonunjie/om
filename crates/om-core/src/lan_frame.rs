// SPDX-License-Identifier: MPL-2.0
//! Typed `LanFrameV1` for the transfer kinds (`spec/cddl/lan.cddl`, ADR-0158, ADR-0194): object_begin, object_chunk,
//! object_end, ack, error and object_receipt. Pure. `decode` goes through the product registry check first
//! ([`om_codec::schema::Registry::check_lan_frame`]), so a frame that the codec refuses never reaches the caller; the
//! request kinds (fetch_async_key, fetch_blob) decode to [`Frame::Request`] with their kind only, because their
//! capability handling belongs to the first-contact and file slices.
//!
//! One DataChannel message carries exactly one frame (the caller enforces that), and chunks are at most 16 KiB.

use om_codec::cbor::{Value, encode};
use om_codec::schema::Registry;

/// The largest frame the codec accepts on this channel (a 16 KiB chunk plus envelope, under the profile's ceiling).
pub const MAX_FRAME_BYTES: usize = 131_068;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frame {
    Begin {
        transfer_id: [u8; 16],
        object_kind: u64,
        total_bytes: u64,
        sha256: [u8; 32],
    },
    Chunk {
        transfer_id: [u8; 16],
        offset: u64,
        bytes: Vec<u8>,
    },
    End {
        transfer_id: [u8; 16],
    },
    Ack {
        transfer_id: [u8; 16],
        offset: u64,
    },
    Error {
        id: [u8; 16],
        code: u64,
    },
    Receipt {
        transfer_id: [u8; 16],
        tag: [u8; 32],
    },
    /// fetch_async_key (5) or fetch_blob (6): shape-checked, contents handled elsewhere.
    Request {
        kind: u64,
    },
}

fn m(kind: u64, rest: Vec<(u64, Value)>) -> Vec<u8> {
    let mut pairs = vec![(0, Value::Uint(1)), (1, Value::Uint(kind))];
    pairs.extend(rest);
    pairs.sort_by_key(|(k, _)| *k);
    encode(&Value::Map(
        pairs
            .into_iter()
            .map(|(k, v)| (Value::Uint(k), v))
            .collect(),
    ))
}

/// Canonical bytes of a frame, or `None` for [`Frame::Request`] (not built here).
pub fn encode_frame(f: &Frame) -> Option<Vec<u8>> {
    let b = |x: &[u8]| Value::Bytes(x.to_vec());
    Some(match f {
        Frame::Begin {
            transfer_id,
            object_kind,
            total_bytes,
            sha256,
        } => m(
            1,
            vec![
                (2, b(transfer_id)),
                (3, Value::Uint(*object_kind)),
                (4, Value::Uint(*total_bytes)),
                (5, b(sha256)),
            ],
        ),
        Frame::Chunk {
            transfer_id,
            offset,
            bytes,
        } => m(
            2,
            vec![
                (2, b(transfer_id)),
                (6, Value::Uint(*offset)),
                (7, b(bytes)),
            ],
        ),
        Frame::End { transfer_id } => m(3, vec![(2, b(transfer_id))]),
        Frame::Ack {
            transfer_id,
            offset,
        } => m(4, vec![(2, b(transfer_id)), (14, Value::Uint(*offset))]),
        Frame::Error { id, code } => m(7, vec![(2, b(id)), (13, Value::Uint(*code))]),
        Frame::Receipt { transfer_id, tag } => m(8, vec![(2, b(transfer_id)), (15, b(tag))]),
        Frame::Request { .. } => return None,
    })
}

fn arr<const N: usize>(v: Option<&Value>) -> Option<[u8; N]> {
    match v? {
        Value::Bytes(x) => x.as_slice().try_into().ok(),
        _ => None,
    }
}
fn uint(v: Option<&Value>) -> Option<u64> {
    match v? {
        Value::Uint(x) => Some(*x),
        _ => None,
    }
}

/// Decodes one channel message. The error is the codec's stable class.
pub fn decode_frame(reg: &Registry, bytes: &[u8]) -> Result<Frame, &'static str> {
    let v = reg.check_lan_frame(bytes, MAX_FRAME_BYTES)?;
    let g = |k: u64| v.get(k);
    let bad = "wrong_type";
    let kind = uint(g(1)).ok_or(bad)?;
    Ok(match kind {
        1 => Frame::Begin {
            transfer_id: arr(g(2)).ok_or(bad)?,
            object_kind: uint(g(3)).ok_or(bad)?,
            total_bytes: uint(g(4)).ok_or(bad)?,
            sha256: arr(g(5)).ok_or(bad)?,
        },
        2 => Frame::Chunk {
            transfer_id: arr(g(2)).ok_or(bad)?,
            offset: uint(g(6)).ok_or(bad)?,
            bytes: match g(7) {
                Some(Value::Bytes(x)) => x.clone(),
                _ => return Err(bad),
            },
        },
        3 => Frame::End {
            transfer_id: arr(g(2)).ok_or(bad)?,
        },
        4 => Frame::Ack {
            transfer_id: arr(g(2)).ok_or(bad)?,
            offset: uint(g(14)).ok_or(bad)?,
        },
        5 | 6 => Frame::Request { kind },
        7 => Frame::Error {
            id: arr(g(2)).ok_or(bad)?,
            code: uint(g(13)).ok_or(bad)?,
        },
        8 => Frame::Receipt {
            transfer_id: arr(g(2)).ok_or(bad)?,
            tag: arr(g(15)).ok_or(bad)?,
        },
        _ => return Err("value_range"),
    })
}

/// [`decode_frame`] against the W1 registry the LAN profile is frozen in.
pub fn decode(bytes: &[u8]) -> Result<Frame, &'static str> {
    decode_frame(&Registry::load_w1(), bytes)
}
