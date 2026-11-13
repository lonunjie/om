# openmls-wasm-shims (ADR-0187, proposed by delegated draft; remove this directory to revert)

Two plumbing shims for building OpenMLS 0.9.0 into the Core wasm module with no wasm-bindgen. Neither contains
cryptography. They are wired in by `[patch.crates-io]` in the workspace [`Cargo.toml`](../../Cargo.toml) and are only used when the
`groups-openmls` feature pulls OpenMLS in on `wasm32-unknown-unknown`.

| Directory | Replaces | What differs from upstream |
|---|---|---|
| `web-time/` | `web-time` 1.1.0 (MIT OR Apache-2.0) | Clean-room small crate, not a copy: `SystemTime` reads `om_host.now_ms`; std-compatible serde form. Upstream licence texts kept beside it. |
| `getrandom-0.4.3/` | `getrandom` 0.4.3 (MIT OR Apache-2.0) | Full upstream copy, one changed line in `src/error.rs` (`not(getrandom_backend = "custom")` on the `WEB_CRYPTO` match arm). Upstream defect: the arm is compiled when feature `wasm_js` is on, but the constant exists only in the `wasm_js` backend, which `custom` replaces. Drop this copy when upstream fixes it. |

`getrandom` 0.2 needs no patch: its `custom` feature plus one registration in `crates/om-wasm/src/abi.rs`. `getrandom` 0.3 and
0.4 share the `__getrandom_v03_custom` symbol, which `om-wasm` already defines. All three call `om_host.random_fill`.

Licences: upstream `LICENSE-MIT` and `LICENSE-APACHE` are kept in each directory. [`tools/core_guard.py`](../../tools/core_guard.py) scans this
directory for `unsafe` (the only allowed use is the single `om_host.now_ms` import declaration in `web-time/src/lib.rs`).
