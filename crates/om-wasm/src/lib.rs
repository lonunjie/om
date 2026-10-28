// SPDX-License-Identifier: MPL-2.0
//! `om-wasm`: the narrow bridge between `om-core` and a JavaScript host.
//!
//! The whole surface is `om_in_alloc(len) -> ptr`, `om_call(len) -> out_len`, `om_out_ptr() -> ptr`: the host
//! writes one UTF-8 JSON request `{"cmd": ..., ...}` into the input buffer, calls `om_call`, and reads one JSON
//! response `{"ok": ...}` or `{"err": {"code", "message"}}` from the output buffer. Coarse typed calls, no chatty
//! FFI, no raw key material or mutable internals exposed (ENGINEERING_REPOSITORY_LAYOUT.md "Core-to-UI API").
//!
//! Host imports (module `om_host`): `random_fill(ptr, len)` backed by `crypto.getRandomValues`, and
//! `now_ms() -> f64` backed by `Date.now()` (display and expiry hints only, never authority).
//! A panic aborts the instance; the host must treat a trapped instance as dead and re-open from storage.

pub mod bridge;

#[allow(unsafe_code)]
mod abi;
