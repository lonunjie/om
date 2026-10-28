// SPDX-License-Identifier: MPL-2.0
//! `om-codec`: OM-CBOR-v1 strict deterministic codec (RFC 8949 section 4.2.1 profile in
//! `spec/registries/om-cbor-profile.json`) and the registry-driven validator for the frozen W0 objects.
//!
//! Decoding is strict: non-shortest heads, indefinite lengths, tags, floats, unsorted or duplicate keys,
//! invalid UTF-8, trailing bytes and any size/depth/count limit overrun are rejected with a stable error class
//! before an object is handed to the caller. Nothing here does I/O or panics on input bytes.
//!
//! Adapted from the V-005 feasibility prototype (`verification/prototypes/v005-encoding`); the committed
//! corpus `verification/vectors/v005-w0-object-vectors.json.gz` is replayed by `tests/w0_vectors.rs`.

pub mod cbor;
pub mod derive;
pub mod schema;

pub use cbor::{Class, Limits, Value, decode, encode};
pub use schema::Registry;
