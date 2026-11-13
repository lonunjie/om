# MLS on wasm: options A, B, C, D and the evidence for the choice (2026-11-14)

Status: evidence for the proposed ADR [`ADR-0187`](../../decisions/ADR-0187-mls-engine-on-wasm-unforked-openmls-with-host-entropy-and-clock.md) (delegated draft, not accepted). Docs only: no file under `crates/`, no Cargo change, no wasm config change was made. The probe below was built and run in a scratch directory outside the repository.

## Design sources read

**Read in full:** [`AGENTS.md`](../../AGENTS.md) "Design gate"; [`2026-11-13-groups-plan.md`](2026-11-13-groups-plan.md) (all of it, gaps G1 to G9, the slice 2 gate, options A/B/C, Q1, Q2); ADR-0177; ADR-0118; [`OPERATORLESS_CORE_AND_ECOSYSTEM.md`](../constitution/OPERATORLESS_CORE_AND_ECOSYSTEM.md); [`PRIVACY_GOVERNANCE_AUTHORITY.md`](../constitution/PRIVACY_GOVERNANCE_AUTHORITY.md); [`PRODUCTION_CRYPTO_INTEGRATION_GATE.md`](../architecture/crypto/PRODUCTION_CRYPTO_INTEGRATION_GATE.md); [`tools/core_guard.py`](../../tools/core_guard.py); the `om-wasm` crate header, manifest and [`.cargo/config.toml`](../../.cargo/config.toml); `crates/om-core/src/crypto/rng.rs`; the head of `groups/mls.rs`.

**Read with lines cut at 700 to 900 characters** (so long table rows and paragraphs may be incomplete): [`CRYPTOGRAPHY_AND_AGILITY.md`](../architecture/crypto/CRYPTOGRAPHY_AND_AGILITY.md), [`PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`](../constitution/PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md), [`PROTOCOL_EXTENSIBILITY.md`](../constitution/PROTOCOL_EXTENSIBILITY.md), [`NAMING.md`](../constitution/NAMING.md).

**Read in part:** [`PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md`](../constitution/PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md) (sections 9 and 17, and a keyword search); ADR-0178 (decision and consequences); ADR-0183 and ADR-0186 (headers); [`ENGINEERING_REPOSITORY_LAYOUT.md`](../process/ENGINEERING_REPOSITORY_LAYOUT.md) ("Core-to-UI API" and "Storage"); [`GROUP_CHAT_ARCHITECTURE.md`](../architecture/groups/GROUP_CHAT_ARCHITECTURE.md) (keyword search for MLS and wasm lines, and the plan's summary of it).

**Not read:** [`LEGAL_AND_OPERATIONAL_BOUNDARIES.md`](../constitution/LEGAL_AND_OPERATIONAL_BOUNDARIES.md), [`LICENSING_AND_CONTRIBUTION_POLICY.md`](../constitution/LICENSING_AND_CONTRIBUTION_POLICY.md) and [`PROJECT_STEWARDSHIP_AND_CONTINUITY.md`](../constitution/PROJECT_STEWARDSHIP_AND_CONTINUITY.md) beyond a keyword search (nothing in the matches concerns vendored dependencies); the rest of the six `crates/` domains. No code commit claims a design domain here, so no `Design-Read` claim is made beyond the above.

## The question

Stock OpenMLS 0.9.0 has a `compile_error!` on wasm unless `js` is on, and `js` pulls wasm-bindgen glue; the repo selects getrandom's `custom` backend for the whole wasm target; the stock provider seeds itself from the OS. The plan offered A (fork with injected RNG and time), B (wasm-bindgen glue), C (native first). The brief added D (separate module).

## Findings that decide it

1. **The "narrow ABI, no wasm-bindgen" rule is not audited text.** It is in the `om-wasm` crate docs and the core-kernel status report. Nothing under `docs/constitution` or `docs/architecture` says it. It is an engineering convention that can be kept by a mechanical check.
2. **C is ruled out by ADR-0178**, which makes the PWA the canonical client, says every capability ships there first, and says native shells embed the same web bundle. A native-only engine reaches no user.
3. **Core's production RNG is not a separate generator.** `system_rng()` is `OsRng`, which on wasm is getrandom's `custom` backend, which is the `om_host.random_fill` import, which is `crypto.getRandomValues`. The plan's gap G2 ("must use Core's RNG, not getrandom directly") describes the injected-per-call form that the slice 2a trait comment (D-g9) chose. Core's own production path is getrandom to the host. The honest equivalence requirement is therefore the same host source by the same route.
4. **Per-call seeding of the stock library is not possible.** `openmls_rust_crypto::RustCrypto::default()` seeds `ChaCha20Rng::from_entropy()` (private field); `hpke-rs-rust-crypto` builds a fresh PRNG from `SysRng` on each HPKE operation, and its seedable mode is the `deterministic-prng` test feature. So "host passes seed material per call" cannot work without editing crypto code or writing an OM provider, which the gate forbids (section 2, "custom random-number generator", and any new provider surface).
5. **Time is not authority.** OpenMLS uses `SystemTime::now()` in `key_packages/lifetime.rs` (KeyPackage validity) and in `past_secrets.rs` and `message_secrets.rs` (past-epoch retention timestamps). It does not use it for membership or ordering.
6. **A separate module (D) does not remove the entropy or wasm-bindgen question.** The same two imports problem recurs inside the second module, so D needs the same shims or glue as the single module. D also needs a host bridge (an additive `om-wasm` import), moves an engine snapshot with MLS private keys across the JS bridge between two memories (against "do not expose raw private keys to JS"), and adds a second signed asset and version-skew checks. Its genuine advantages are isolation and lazy loading.

## Evidence: probe build (scratch, not in the repository)

A crate depending on `openmls =0.9.0` (features `js`, `fork-resolution`), `openmls_rust_crypto =0.6.0`, `openmls_basic_credential =0.6.0` with one function that creates a KeyPackage, a one-member group and an Add Commit (Ed25519/X25519/AES-128-GCM suite). Built with the rustup toolchain (`/opt/homebrew/opt/rustup/bin` first on PATH), `--release --target wasm32-unknown-unknown`, `opt-level = "s"`, LTO, `--offline`.

| Configuration | Result |
|---|---|
| Stock, no shims, cfg `custom` | compile error in `getrandom 0.2.17` (target not supported without `js` or `custom` feature) |
| Add getrandom 0.2 `custom` feature and registration; cfg `custom` | compile error in `getrandom 0.4.3` `error.rs:185`: `Error::WEB_CRYPTO` not found (feature `wasm_js` on, backend `custom`) |
| Same, cfg `wasm_js` | builds (1,740,352 bytes); imports `om_host.random_fill` plus eight `__wbindgen_*` imports (`getRandomValues`, `now`, `throw`, externref table, ...) |
| Plus patched `getrandom 0.4.3` (one `cfg` condition on one match arm), `web-time` shim reading `om_host.now_ms`, `__getrandom_v03_custom` calling `om_host.random_fill`; cfg `custom` | builds (1,306,570 bytes); **imports are exactly `om_host.random_fill` and `om_host.now_ms`** |

The last module was instantiated under Node 24 with those two imports (`crypto.randomFillSync` and `Date.now`). `probe()` returned 677 (the serialized Add Commit length); the host supplied 352 bytes of entropy and 5 clock reads. A first run failed in my shim (an unchecked `Duration` subtraction) and a second returned an error code because I reused one signing key for both parties; both were fixes to the probe, not to OpenMLS. The shim implements `SystemTime` with a serde form equal to the standard library's, since OpenMLS persists these timestamps.

Not measured: the size of `om_wasm.wasm` with the engine linked (baseline 4,213,013 bytes per the plan), run time on a phone, a real two-party Welcome round trip on wasm, and equality of a native snapshot and a wasm snapshot. These are the first acceptance items of the implementing slice.

## Option table

| | Constitution fit | New crypto surface | Core module changes | Verdict |
|---|---|---|---|---|
| A fork + injected RNG | maintained crypto-adjacent fork (ADR-0118 rule 3); highest burden | yes, HPKE/provider edits | engine linked in | rejected as specified |
| B wasm-bindgen glue | fits text; weaker narrow-ABI convention | none | glue and second cfg | fallback only |
| C native-only | fails ADR-0178 | none | none | rejected |
| D separate module | fits; extra bridge and key bytes through JS; second asset | none | additive host import + adapter | not now; revisit if size demands |
| **A2 stock engine + two plumbing shims in the Core module** | fits; fewest new trusted pieces; one signed module | **none** (shims contain no crypto) | engine linked in, shims, allow-list guard | **proposed** |

## What D means for the entropy requirement (the brief's question)

Met in the same form as Core, by routing the module's getrandom (0.2 and 0.4) to a host import. Not met in the "host passes a seed per call" form, because the library cannot take one. It is not new crypto surface: the source (browser CSPRNG), the route (custom backend to the host import) and the guard (a host that cannot supply entropy traps) are Core's existing ones. What changes is that `CryptoEnv.rng` goes unused by the real engine, and the trait comment saying the engine "must not read the OS RNG" needs rewording; that comment is D-g9, an engineering decision of the plan, not audited text.

## What the operator must confirm

As in the ADR: (1) A2 over D, B or a fork; (2) host entropy and clock read directly by the engine, D-g9 reworded; (3) a size budget for `om_wasm.wasm` and first load; (4) a `cargo-deny` duplicate-crypto exception for the OpenMLS provider tree; (5) optional upstream report of the getrandom defect.

## Not done here

No Cargo, `.cargo`, guard or crate file was changed. Implementing A2 is slice 2 work after acceptance: the shims, the `[patch]` entries, the registration in `om-wasm`, the `OpenMlsGroupCrypto` engine, the import allow-list in [`tools/core_guard.py`](../../tools/core_guard.py), a generic-over-engine `groups_loopback`, and the size and snapshot-equality measurements above.
