# ADR-0187 — The MLS engine on wasm: unforked OpenMLS in the existing Core module, with entropy and time through the existing host imports

- **Status:** **Accepted** by the operator on 2026-11-14 (option A2, the size budget and the `cargo-deny` exception as proposed below; recorded in "Operator confirmation"). Drafted earlier the same day by a delegate of the coordinating session; the operator's confirmation replaces the delegation, so this is no longer an ADR by delegation. It still does not amend audited text, see "Does not amend".
- **Date:** 2026-11-14
- **Decides:** how `groups` (ADR-0177) gets a real `GroupCrypto` engine in the PWA, which is the canonical client (ADR-0178)
- **Evidence:** [`2026-11-14-mls-wasm-decision.md`](../docs/reports/2026-11-14-mls-wasm-decision.md); the question and the failed dependency gate are in [`2026-11-13-groups-plan.md`](../docs/reports/2026-11-13-groups-plan.md) ("Slice 2 dependency gate")
- **Amends:** nothing. **Does not amend:** [`CRYPTOGRAPHY_AND_AGILITY.md`](../docs/architecture/crypto/CRYPTOGRAPHY_AND_AGILITY.md), [`PRODUCTION_CRYPTO_INTEGRATION_GATE.md`](../docs/architecture/crypto/PRODUCTION_CRYPTO_INTEGRATION_GATE.md), ADR-0107, ADR-0118, ADR-0177, ADR-0178, any wire object, any CryptoProfile, the v1 MLS suite.

## Context

ADR-0177 selected OpenMLS 0.9.0 as the groups runtime and ADR-0178 says the PWA runs the same group code as any shell, because every native shell is a web view of the same signed bundle. The slice 2 dependency gate (2026-11-13) then found that stock OpenMLS does not build into the Core wasm module as it stands:

1. OpenMLS has a `compile_error!` on wasm unless its `js` feature is on. That feature pulls `web-time` and `getrandom 0.4` with `wasm_js`, both of which import wasm-bindgen glue.
2. [`.cargo/config.toml`](../.cargo/config.toml) selects the getrandom `custom` backend for the whole target, which is how `om-wasm` takes entropy from the host with no wasm-bindgen.
3. The stock RustCrypto provider seeds its own generator from the OS inside the library, so Core's per-call RNG cannot be handed to it.

The plan listed A (patched fork, injected RNG and time), B (wasm-bindgen glue) and C (native-only first). The coordinating session added D (separate wasm module behind the `GroupCrypto` boundary).

## What the text requires (the tests an option must pass)

- Operatorless, no indispensable institution; normal users never see infrastructure; the PWA is the reference client ([`OPERATORLESS_CORE_AND_ECOSYSTEM.md`](../docs/constitution/OPERATORLESS_CORE_AND_ECOSYSTEM.md), [`PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`](../docs/constitution/PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md) P-01, P-02, P-09; ADR-0178 rule 1).
- No silent security downgrade; no user-facing cipher choice; groups are classically confidential in v1 and nothing may claim otherwise (CRYPTOGRAPHY_AND_AGILITY, ADR-0107, R-173).
- Do not invent cryptography: no new primitive, no ad-hoc combiner, no **custom random-number generator**, no custom AEAD (PRODUCTION_CRYPTO_INTEGRATION_GATE section 2).
- A library is replaceable behind OM's interface and its serialization is never the canonical format (CRYPTOGRAPHY_AND_AGILITY "Provider independence"; ADR-0118 rule 3). Groups are implemented "behind `GroupCrypto`" (gate section 6).
- Pinned, reviewed dependencies, no GPL/AGPL in core crates, production excludes test providers (ADR-0118 rules 4 and 5).
- Where did authority go (AGENTS Part 2).

The "narrow ABI, no wasm-bindgen" form of `om-wasm` is an engineering convention recorded in the `om-wasm` crate docs and the core-kernel status report. It is not audited design text, so keeping or changing it is an engineering decision, not an amendment.

## Options

- **A, patched fork of `openmls` and the RustCrypto/HPKE provider with Core's RNG injected.** Passes the wasm build, but the HPKE ephemeral randomness lives inside `hpke-rs-rust-crypto`, so injection means editing crypto code or writing an OM crypto provider. That is the "new crypto surface" the gate forbids and a maintained fork under ADR-0118 rule 3. Rejected as stated; its time half and entropy half are done more cheaply below.
- **B, wasm-bindgen glue and the `wasm_js` backend.** Works, but adds `__wbindgen_*` imports and generated glue to the module that holds the keys, and cannot coexist with the target-wide `custom` cfg without a second build. The probe, built that way, has nine imports, eight of them wasm-bindgen internals. Not wrong, but a larger trusted surface than the alternatives. Kept as the fallback inside D.
- **C, native-only first.** Contradicts ADR-0178: the PWA is canonical, every capability ships there first, and native shells are web views of the same bundle, so there is no native engine that reaches a user without the wasm one. Rejected.
- **D, a separate wasm module (`om-groups-wasm`) behind `GroupCrypto`.** Passes the text, and its isolation and lazy loading are real benefits. Three costs the draft brief did not count: (1) Core cannot call another module from inside Rust, so `om-wasm` gains an additive host import and an adapter, so "unchanged" is not literally true; (2) the engine snapshot, which holds MLS private keys, would cross the JS bridge between the two memories, against the rule "do not expose raw private keys to JS" ([`ENGINEERING_REPOSITORY_LAYOUT.md`](../docs/process/ENGINEERING_REPOSITORY_LAYOUT.md) "Core-to-UI API"); (3) a second signed asset, a second guard surface and version-skew checks between two modules. It also does not remove the wasm-bindgen imports or the entropy question: stock OpenMLS in a separate module still needs the same shims or glue. Not chosen now; the trait boundary keeps it available later (see "Revisit").
- **A2 (chosen): stock `openmls` 0.9.0, `openmls_rust_crypto` 0.6.0 and `hpke-rs` unmodified, in the existing `om-wasm` module, with two plumbing shims.** A probe (report, "Evidence") built exactly this for `wasm32-unknown-unknown` under the existing `custom` cfg. The module's only imports were `om_host.random_fill` and `om_host.now_ms`, the two Core already has. It ran under Node, created a KeyPackage and a group and produced an Add Commit, drawing entropy and time only through those imports.

## Decision

1. **Option A2.** The `groups` engine is `openmls` 0.9.0 with the V-004 feature set, pinned and `--locked`, behind the existing `GroupCrypto` and `GroupCryptoLoader` traits, linked into `om-wasm`. No OpenMLS, RustCrypto, HPKE or any other cryptographic crate is forked, patched or reimplemented. No OM-written crypto provider, no OM generator.
2. **Two plumbing shims, no cryptography in either**, as `[patch.crates-io]` entries in the workspace, each with a header naming what it replaces and why:
   - `web-time`: `SystemTime` backed by the existing `om_host.now_ms` import instead of wasm-bindgen. It must serialize as `std::time::SystemTime` does (`secs_since_epoch`, `nanos_since_epoch`), because OpenMLS stores these timestamps in its state; a test pins that a native snapshot restores on wasm and the reverse.
   - `getrandom 0.4.3`: a one-line `cfg` fix so the `wasm_js` feature that OpenMLS' `js` feature enables does not break the `custom` backend (an upstream defect: `Error::WEB_CRYPTO` is referenced where it is not defined). Drop the patch when upstream fixes it; propose the fix upstream.
   - plus the existing getrandom `custom` route extended to `getrandom 0.2` (feature `custom` and one registration) and `0.4` (one `__getrandom_v03_custom` symbol), both calling the same `random_fill` import `om-wasm` already uses.
3. **Entropy: the requirement is met in the form Core itself meets it, not in the form the trait comment says.** Core's production RNG is `OsRng`, which on wasm is getrandom's `custom` backend, which is `om_host.random_fill`, which is `crypto.getRandomValues` (`crates/om-core/src/crypto/rng.rs`, `crates/om-wasm/src/abi.rs`). The OpenMLS engine will draw from the identical host source by the identical route, at the points where the library asks (provider construction, every HPKE seal). It is not possible to hand the stock library a per-call generator, and per-call seed material cannot reach the HPKE layer (its seeded mode is the test-only `deterministic-prng` feature). So `CryptoEnv.rng` is **not** consumed by the real engine; the fake engine keeps using it for reproducible tests. This is not a new generator, not new crypto and not a weaker source, but it is a change to the wording in `groups/mls.rs` ("An engine must not read the OS RNG itself"), which is decision D-g9 of the groups plan (an engineering decision, not audited text). The implementing slice rewrites that comment to say what is true. `CryptoEnv.now_ms` is likewise unused by the real engine, which reads the host clock through `now_ms` (KeyPackage lifetimes and past-epoch timestamps; display and expiry hints, never authority).
4. **Guards.** [`tools/core_guard.py`](../tools/core_guard.py) gains an import allow-list check on every `.wasm` in the site: only `om_host.random_fill` and `om_host.now_ms` (plus any import the operator later approves). Any `__wbindgen_*` import fails the release. This turns the narrow-ABI convention into a mechanical check. The shim crates are scanned for `unsafe` with an allow-list of exactly the two host-import declarations.
5. **Dependency policy (ADR-0118 rule 4).** `cargo-deny` is not yet in the repo. When added, its duplicate-crypto ban gets one named exception for the OpenMLS provider tree (second copies of `ed25519-dalek`, `x25519-dalek`, `chacha20poly1305`, plus `aes-gcm`, `p256`, `p384`, `k256`, `ml-dsa`, `x-wing`), with licence check passed (gate report: MIT, Apache-2.0, BSD-3-Clause, ISC, MPL-2.0; `r-efi` via its MIT option, never linked).
6. **Size.** Probe: 1.31 MB for a minimal probe (KeyPackage, group, Add) at `opt-level = "s"` with LTO. The real figure for `om_wasm.wasm` (4.21 MB today) is not measured and is recorded as the first acceptance number of the implementing slice, with a budget the operator sets. If it breaches the budget, the fallback is D with the same engine and shims (revisit below), not a different engine.
7. **What stays untouched.** Wire objects, `GroupCommitBinding` signing (the existing composite device signer, not the MLS credential key), CryptoProfiles, the v1 suite (`MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`), the journal, the storage record. v1 groups stay classically confidential and no surface says otherwise.

## Consequences

- Slice 2 (the real engine) can proceed, and slices 3 to 6 keep their order. The loopback tests become generic over the engine and also run against OpenMLS, with the host-import stub in the test harness supplying a seeded stream, so real-engine tests can still be reproducible.
- Core's wasm module grows by roughly the engine's size and its `unsafe` surface does not grow (the shims' host-import declarations are the only additions and are allow-listed).
- Two small vendored shims become OM's to maintain until upstream changes; both are replaceable and carry no secrets.
- Residual risk: the entropy source is the browser's, as for all of Core. A host with a broken `crypto.getRandomValues` breaks Core and groups together, and the host import must fail closed (trap), which `om_host.random_fill` already must do.
- Independent review: the shims and the allow-list guard are small enough for the same reviewer as ADR-0118's dependency pins. No new cryptographic review item is created, because no cryptography is added or changed.

## Where did authority go?

Nowhere new. No service, origin, account or maintainer gains any power. The two shims route to the host entropy and clock Core already trusts. OM does not become the maintainer of a crypto fork.

## What the operator was asked to confirm

1. **Accept A2** (stock OpenMLS in the Core module, two plumbing shims) rather than D, B or a fork. If the operator wants the engine isolated in its own module for size or isolation, accept D instead; the engine, shims and guard are the same, and the added cost is the bridge and the key-bytes-through-JS point above.
2. **Accept that the real engine uses host entropy directly**, the same source and route as Core, and that D-g9's "Core supplies the RNG on every call" is reworded to say so.
3. **A size budget** for `om_wasm.wasm` (and PWA first-load) to compare against the measured result.
4. **The `cargo-deny` exception** for the OpenMLS provider tree.
5. Optional: file the `getrandom` cfg fix upstream (it is a bug report, not an OM dependency on anyone).

## Operator confirmation (2026-11-14)

The operator, who said "make the best decisions", confirmed this ADR on 2026-11-14:

1. A2 accepted over D, B and a fork.
2. Host entropy used directly by the engine; D-g9 reworded accordingly.
3. Size budget: accepted as this ADR proposes. The measured figures of [`2026-11-14-groups-driver-status.md`](../docs/reports/2026-11-14-groups-driver-status.md) (production-optimised `om_wasm.wasm` and the separate `om_wasm_groups.wasm`, which loads only where groups ship) are the recorded baseline, and the fallback to option D under "Revisit" stays the answer if a later measurement breaches them.
4. The `cargo-deny` duplicate-crypto exception for the OpenMLS provider tree: accepted as described in Decision 5, to be added when `cargo-deny` is added.

## Revisit

Switch to D (same engine, same shims, a separate module with an additive host import) only if the measured size breaches the budget or a reviewer rules that the snapshot must not share a memory with the UI-facing bridge. The `GroupCrypto` boundary is designed so that this is an adapter change, not a redesign.
