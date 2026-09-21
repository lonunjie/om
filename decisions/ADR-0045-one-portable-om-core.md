# ADR-0045 — One Portable OM Core Across Client Shells

**Status:** Accepted direction

Identity, protocol state, cryptographic integration, canonical encodings, recovery and control logic should converge on one portable memory-safe OM Core compiled for WASM/native targets.

UI/platform shells must not independently redefine security semantics.
