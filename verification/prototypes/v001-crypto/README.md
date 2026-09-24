Feasibility prototype for V-001 (`production_crypto_path`) and the V-001 half of V-022 (`performance_budgets`). Not product code. Not for production.

# V-001 crypto path prototype

- **Authorship:** written by a Claude agent; the design package was authored by a ChatGPT design agent and Claude agents; ADR-0117 (proposed) decides whether an AI reviewer counts as independent.
- **Gate effect:** evidence only. No gate is passed by this directory. Per-criterion status is in
  [`verification/results/v001-crypto-latest.json`](../../results/v001-crypto-latest.json) and summarised under [Results](#results).
- **Date/base:** 2026-09-24, cut from `da36f3a`.

## What it is

A Rust implementation of the 1:1 path that OM-Crypto-v1 selects in `CRYPTOGRAPHY_AND_AGILITY.md` (2026-09-23 resolution), written from the public specifications:

| Layer | Specification followed | Where |
|---|---|---|
| Async handshake | Signal **PQXDH** rev 3 (2023-05-24, updated 2024-01-23): curve25519, SHA-256, ML-KEM-768, §2.2 KDF, §3.2–§3.4 | `rust/core/src/pqxdh.rs` |
| Prekey signatures | Signal **XEdDSA** rev 1 §2.3/§3.3/§3.4 (the X25519 identity key signs `EncodeEC(SPK)` and every `EncodeKEM(PQPK)`) | `rust/core/src/xeddsa.rs` |
| KEM | **FIPS 203** ML-KEM-768 via `libcrux-ml-kem` 0.0.10 (formally verified; also what spqr and libsignal use) | `pqxdh.rs`, `acvp.rs` |
| EC ratchet | Signal **Double Ratchet** rev 4 (2025-11-04) §3 (KDF_RK/KDF_CK/skipped keys, MAX_SKIP 1000, §8.4 cap 2000) | `rust/core/src/ratchet.rs` |
| PQ ratchet (SCKA) | **Signal's own `spqr` v1.6.0** (ML-KEM Braid + SPQR chains), used through its public `initial_state`/`send`/`recv` API. Not reimplemented. | dependency |
| Composition | Double Ratchet rev 4 §6 **Triple Ratchet** (`KDF_HYBRID(ec_mk, pq_mk)`), §7.1 PQXDH integration (SK split into SKec/SKscka, SPK_B as Bob's first ratchet key, PQXDH prefix repeated until the first reply) | `rust/core/src/triple.rs`, `store.rs` |
| AEAD | RFC 8452 **AES-256-GCM-SIV** (DR §7.2 recommends SIV or CBC+HMAC; OM asks for misuse resistance). Key and nonce both come from `KDF_HYBRID` (DR §3.1 allows a derived nonce) | `triple.rs` |
| State and wire encoding | Strict deterministic CBOR (RFC 8949 §4.2.1 subset; the decoder enforces `CANONICAL_ENCODING.md` rules) | `rust/core/src/cbor.rs` |
| Prekey publication | Toy `KeyMaterialProvider` with one disjoint pool per device (`ASYNC_KEY_MATERIAL.md` §2/§4/§6) | `rust/core/src/store.rs` |

The domain-separation strings (`OM-Crypto-v1-proto …`), the `EncodeKEM` type byte `0x21` and the header/envelope layouts are **prototype values** and not registry entries. They are listed in `rust/core/src/kdf.rs` and in the vector file's `constants`.

Other pieces:

- `rust/core/src/otk.rs`: one-time-key, depletion, replay and downgrade scenario (20 named checks).
- `rust/core/src/persist.rs`: a 249-step conversation (367 messages). Every step runs in a fresh process or instance, and out-of-order and skipped keys cross the reloads.
- `rust/core/src/vectors.rs` writes `vectors/om-v001-proto-vectors.json` (deterministic, 40 KB).
- `python/check_vectors.py` (uv project, `cryptography` 50.0.1 + `kyber-py` 1.2.0) is the **second implementation**. It has its own CBOR codec, Double Ratchet, XEdDSA verification (Montgomery-to-Edwards conversion, then Ed25519) and PQXDH, and it runs the NIST ACVP KATs on kyber-py.
- `rust/core/src/acvp.rs` runs the official **NIST ACVP ML-KEM-768** KATs (usnistgov/ACVP-Server `975de31e`, SHA-256 pinned in `run.sh`) against libcrux on native and WASI.
- `rust/node-wasm` + `node/run_wasm.mjs` form the wasm32-unknown-unknown build for Node/V8. They use a raw ABI (no wasm-bindgen). Host entropy comes from `crypto.getRandomValues` through getrandom's `custom` backend, and the clock is `performance.now()`.
- `probe_upstream.sh` (`--long`) clones and builds Signal's spqr and libsignal for native and wasm32.

Deterministic randomness exists only behind the `insecure-test-rng` cargo feature (`InsecureTestRng`). Enabling the `production` feature together with it is a `compile_error!`.

## How to run

```sh
cd verification/prototypes/v001-crypto && ./run.sh          # default
cd verification/prototypes/v001-crypto && ./run.sh --long   # + upstream spqr/libsignal build probes
```

- Needs rustup (toolchain pinned to 1.98.1 in `rust/rust-toolchain.toml`, installed automatically along with the wasm targets), wasmtime, Node ≥ 22, uv, python3, git, curl and network access on the first run (crates, PyPI, ACVP files).
- Runtime on an Apple M3 (8 cores, 8 GB): about 1 minute after the first build. The first build adds about 1–2 minutes. `--long` adds about 5 minutes (libsignal build).
- Output: `verification/results/v001-crypto-latest.json` is deterministic (no timestamps, paths or timings). Raw timings go to `rust/target/v001-perf.json`, which is not committed. The script exits non-zero if any executable check fails.
- Installed for this work: Rust toolchain 1.98.1 (+ wasm32-wasip1 and wasm32-unknown-unknown targets), crates from crates.io, spqr from github.com/signalapp (tag v1.6.0, commit `06959b47`), and PyPI `cryptography` and `kyber-py` (pinned in `python/uv.lock`).

## Results

| Gate | Criterion | Status | Evidence | Reason |
|---|---|---|---|---|
| V-001 | Rust PQXDH + Triple Ratchet/SPQR state machine on native and wasm32 with persistence/reload | **met** | `persist.rs`, `triple.rs`, results `summary.persistence` | 249 fresh-process steps and 367 messages (incl. out-of-order and persisted skipped keys, SPQR epoch 5). Native, wasip1, alternating native/wasip1 and Node fresh-instance runs are **byte-identical at every step**. |
| V-001 | Vectors consumed by a second independent implementation or checked against upstream KATs | **met** (scope note) | `vectors/…json`, `python/check_vectors.py`, `acvp.rs` | Python reproduces 180/180 values over 16 messages, including bit-exact GCM-SIV ciphertexts, and rejects a mutated vector. ACVP ML-KEM-768 passes 80/80 on libcrux (native and WASI) and 80/80 on kyber-py. SPQR message keys are inputs, not independently recomputed. |
| V-001 | One-time key handling and depletion without downgrade | **met** | `otk.rs`, results `summary.selftest_checks` | 20/20 checks on all three runtimes (list below) |
| V-001 | Dependency, licensing and update strategy recorded in an ADR | **partial** | [Draft ADR](#draft-adr-dependency-licensing-and-update-strategy-for-the-production-crypto-path) | Draft only. Needs filing, plus an operator/legal decision on AGPL `spqr`. |
| V-001 | No reliance on an unsupported external library API contract | **partial** | [Upstream investigation](#upstream-library-investigation) | libsignal is not used. spqr has a narrow public API but no support statement, no crates.io release and an AGPL licence, and its wire and state formats are library-defined. |
| V-001 (pre-release) | Independent cryptographic review | unmet | — | Needs external reviewers |
| V-001 (pre-release) | Production build provably excludes dev/test CryptoProvider (R-072) | partial | `rng.rs`, `run.sh` §3 | Sketch: `production`+`insecure-test-rng` does not compile, and the production rlib has no `InsecureTestRng` symbol (the test rlib does, as a control). |
| V-022 | V-001/V-005 prototypes record crypto/codec costs on native and wasm32 vs budgets | partial | [Performance](#performance-v-022) | V-001 half done on 3 runtimes. Not on a reference low-end device. V-005 codec is out of scope here. |

One-time-key checks (each passes natively, on wasmtime and in Node):

- depletion falls back to the signed last-resort ML-KEM key;
- every session has ML-KEM and a V1 SPQR state;
- one-time private keys are deleted after use, and the provider inventory drains;
- replay to an existing session is rejected with state unchanged;
- replay of consumed one-time PQ keys is rejected (with and without an EC one-time key);
- replay of a last-resort handshake is rejected by the replay cache;
- a replenished pool serves one-time keys again;
- a tampered initial message is rejected **without consuming keys**;
- a genuine initial message consumes keys exactly once;
- a bundle stripped of its PQ key is refused (no classical fallback);
- substituted PQ prekeys and signed prekeys are refused;
- an initial message without the ML-KEM ciphertext is rejected;
- an **SPQR V0 (PQ-ratchet-off) message is rejected** (`MinimumVersion`), and the session continues;
- the previous last-resort key is accepted during its grace period and rejected after expiry;
- the provider serves the rotated last-resort key;
- device state round-trips through canonical CBOR.

Independence: the Python checker uses a different language and different primitive libraries (OpenSSL via `cryptography`, pure-Python kyber-py) from the Rust side (dalek, libcrux, RustCrypto). ML-KEM is also checked against NIST's own KATs. **Both implementations were written by the same AI agent in one session**, and nobody has reimplemented SPQR/ML-KEM Braid. Whether this is enough for `independent_implementation_or_upstream_kat` is for the integrator and ADR-0117.

## Performance (V-022)

Medians (p95) in **microseconds** over 100 session setups and 600 messages (160-byte payload, bursts of 3 in alternating directions, so both the DH ratchet and SPQR epochs advance; SPQR reached epoch 7). Measured on an Apple M3 laptop while other agent builds were running. Numbers across runs varied by about 2x, and the table shows the slower run.

| Operation | native | wasm32-wasip1 (wasmtime 49) | wasm32-unknown-unknown (Node 24 / V8) | Budget |
|---|---|---|---|---|
| Session setup, initiator (verify 2 XEdDSA sigs, PQXDH, TR init, first encrypt) | 750 (6186) | 1536 (9513) | 1498 (8475) | |
| Session setup, responder (ML-KEM re-expand from seed + decaps, PQXDH, TR init, first decrypt) | 609 (5025) | 1270 (9760) | 1185 (6666) | |
| **Session setup total** | **1475** (12459) | **2873** (18207) | **2709** (16430) | ≤ 50 000 |
| Message encrypt | 19 (64) | 51 (176) | 66 (246) | |
| Message decrypt | 18 (222) | 38 (487) | 51 (504) | |
| **Message encrypt + decrypt** | **59** (296) | **131** (572) | **170** (664) | ≤ 5 000 |
| One-time prekey generation (ML-KEM keygen + XEdDSA sign + X25519) | 114 | 293 | 291 | |
| Device state serialize (1 session) | 1.5 | 2.0 | 1.5 | |
| Device state reload (1 session) | 10.6 | 55.8 | 27.1 | |

Sizes:

- session state (Triple Ratchet incl. spqr state): **4 105 B**;
- device state with one session: 4 488 B;
- per-message overhead: header + 16-byte tag + spqr chunk. The initial messages also carry the PQXDH prefix (about 1.2 KB, mostly the 1088-byte ML-KEM ciphertext) until the first reply;
- release wasm about 1.35–1.39 MB unstripped (includes serde_json and the CLI).

**Scaling caveat.** This Mac is not a low-end device. A sub-USD-200 Android phone running WebView wasm is plausibly 5–15x slower than an M3 on this single-threaded integer workload. Even at 10x, the median message cost (about 1.7 ms) and setup (about 29 ms) stay inside the budgets. The p95 setup at 10x (about 160 ms) would not. That is a p95, and the budget is p75, but p95 must be confirmed on real devices. The p95 tails come from:

- SPQR messages that carry ML-KEM work;
- V8 tier-up (Liftoff to TurboFan) in the first iterations;
- noise from other processes on this machine.

The budgets look attainable, and no ADR revision is proposed. Real-device measurement stays a V-022 pre-release item.

## Upstream library investigation

Checked 2026-09-24 by manual builds outside this tree; the table records those results. A first `run.sh --long` run cloned the probes inside `rust/target`, where they inherited this prototype's `.cargo/config.toml` and toolchain pin, so they gave contradictory results. The probe directory was moved outside the tree afterwards, and the probes have not been re-run.

| | Signal `spqr` (SparsePostQuantumRatchet) | Signal `libsignal-protocol` |
|---|---|---|
| Version | tag v1.6.0, `06959b47` (2026-09-16); **not on crates.io**, git dependency only | `e8cc2dd` (2026-09-22), workspace 0.103.1 |
| Licence | **AGPL-3.0-only** | **AGPL-3.0-only** |
| Support contract | README describes the API. No statement that external use is supported, and no semver policy | README: "Use outside of Signal is unsupported"; APIs "subject to change without notice" |
| Native build / tests | builds; **56/56 upstream tests pass** | builds |
| wasm32-wasip1 | builds; **runs in this prototype under wasmtime** | builds |
| wasm32-unknown-unknown | the default build fails (getrandom 0.3 needs a backend). With a consumer-selected backend it **builds and runs in this prototype under Node** | the default build fails (getrandom 0.4 needs the opt-in `wasm_js` backend); not pursued |
| API used here | `initial_state(Params{version V1, min_version V1, …})`, `send(state, rng)`, `recv(state, msg)` | none |

Findings:

1. spqr's public API maps one-to-one onto DR rev 4 §6 (`SCKARatchetSendKey`/`ReceiveKey` returning message keys). OM can keep all framing, AD, AEAD, persistence and one-time-key logic in OM code, and it did so here.
2. spqr's **state** (protobuf) and **wire message** (`version ‖ varint epoch ‖ varint index ‖ payload` with erasure-coded chunks) are defined by the library, not by a published wire spec. `CRYPTOGRAPHY_AND_AGILITY.md` forbids letting a library serialization become the only canonical wire/storage format without an ADR.
3. `min_version` must be V1. With V0 the library negotiates the PQ ratchet away, which OM rule 6 forbids. The prototype pins V1 and tests that a V0 message is rejected.
4. libsignal wraps the same spqr crate. Using libsignal would add the "unsupported outside Signal" contract without solving the licence.

## Draft ADR: Dependency, licensing and update strategy for the production crypto path

*Status: proposed draft for the integrator to file under `decisions/`. Written by a Claude agent.*

**Context.** V-001 needs the OM-Crypto-v1 1:1 path (PQXDH + Triple Ratchet/SPQR) to rest on maintained libraries whose licences fit the policy in `LICENSING_AND_CONTRIBUTION_POLICY.md` (om-core MPL-2.0). It must also not rely on an unsupported external API contract. This prototype shows the path works on native and wasm32 using the libraries below.

**Decision (proposed).**

1. **Primitive libraries** (all permissive, all build for native, wasm32-wasip1 and wasm32-unknown-unknown):
   - X25519 and XEdDSA group operations: `x25519-dalek` / `curve25519-dalek` (BSD-3-Clause).
   - ML-KEM-768: `libcrux-ml-kem` (Apache-2.0, formally verified with hax/F*). RustCrypto `ml-kem` (Apache-2.0 OR MIT) is the named fallback and cross-check.
   - HKDF, HMAC, SHA-2 and AES-256-GCM-SIV: RustCrypto (MIT OR Apache-2.0).
   - Deterministic CBOR: OM's own strict codec (V-005).
   - Protocol composition (PQXDH, EC Double Ratchet, Triple Ratchet framing, prekey store): OM-maintained code implementing Signal's public-domain specifications (both PQXDH and the Double Ratchet document place themselves in the public domain). This is strategy B of `PRODUCTION_CRYPTO_INTEGRATION_GATE.md` §4, because no supported library exists (see strategy A below).
2. **SCKA / SPQR licence conflict: must be resolved before engineering.** The only SPQR implementation is Signal's `spqr`, which is AGPL-3.0-only. Linking it into om-core would make every distributed client binary an AGPL work, which contradicts the MPL-2.0 core policy (and has app-store distribution implications). Options, in order of preference:
   - (a) Ask Signal for an MPL/Apache relicence or a linking exception for `spqr`.
   - (b) Write an OM-maintained **clean-room** implementation of ML-KEM Braid + SPQR from Signal's published specifications. Implementers must not read the `spqr` source. `spqr` then serves only as an interoperability **test oracle** in a separate, non-distributed AGPL test harness. This needs its own independent review (a new high-risk component).
   - (c) Change om-core's licence (constitutional change; not recommended).

   Until one of these is decided, OM-Crypto-v1 stays `experimental`.
3. **Stable contract is OM's, not a library's.**
   - Library types never cross the `CryptoProvider`/`Ratchet` interface.
   - Persisted state is OM CBOR. Library state is embedded only as a versioned opaque field with a migration story.
   - Every wire byte (including the SCKA message) must be specified in an OM spec document with vectors, so a replacement implementation can be verified against it.
   - libsignal is not a dependency; its README declares external use unsupported. It may be a behavioural reference.
4. **Update and CVE process.**
   - Pin exact versions (`Cargo.lock`, `rust-toolchain.toml`, `--locked`), and review every bump.
   - Watch RustSec/GitHub advisories and the upstream security contacts (Signal, Cryspen/libcrux, RustCrypto, dalek), with `cargo-audit` and `cargo-deny` in CI. `cargo-deny` gets a licence allow-list that rejects GPL/AGPL in core crates, plus bans on duplicate crypto implementations.
   - Every update must pass: the NIST ACVP KATs, OM's own vectors, the cross-implementation (second-language) vectors, and the downgrade/one-time-key negative tests.
   - A critical advisory is answered with a patched release and, where needed, a signed SecurityAdvisory that raises peer-local floors. There is no remote kill switch.
5. **Production exclusion of test providers (R-072).**
   - Deterministic and test RNGs and providers live only behind an `insecure-*` cargo feature.
   - Enabling that feature together with `production` is a compile error.
   - Release CI builds with `--locked`, checks that no test-provider symbol is present in release artifacts, and publishes reproducible build hashes.
6. **Downgrade invariants in the profile:** a PQ prekey is mandatory in every bundle; the ML-KEM ciphertext is mandatory in every initial message; SPQR `min_version = V1`; one-time keys are consumed only after the initial AEAD authenticates.

**Consequences.** V-001 criteria 4 and 5 can close once options 2(a) or 2(b) are chosen and this ADR is accepted. Choosing 2(b) adds an implementation-and-review work item and makes the SCKA wire format an OM spec deliverable.

## Design bugs, contradictions and gaps found

1. **Licence contradiction (blocking).** The architecture selects "Triple-Ratchet/SPQR-class" for OM-Crypto-v1, but the only implementation is AGPL-3.0-only, while om-core is MPL-2.0 by policy. Neither `PRODUCTION_CRYPTO_INTEGRATION_GATE.md` nor the licensing policy addresses this.
2. **Library-defined wire format.** Adopting SPQR today makes spqr's own message/state encoding the de facto canonical format, which `CRYPTOGRAPHY_AND_AGILITY.md` ("Provider independence") forbids without an ADR.
3. **Identity key binding is unspecified.** PQXDH needs one X25519 identity key used for both DH and XEdDSA signing. OM's durable credentials are Ed25519 + ML-DSA-65 (ADR-0107). `ASYNC_KEY_MATERIAL.md` §3 says bundles bind "to the authorized DeviceIdentity" but not how. Proposal: the DeviceIdentity's dual signature certifies the PQXDH IK in AccountControlState-authorised material. Whether ML-DSA should also sign PQ prekeys (PQXDH §4.8's quantum-server substitution) is a research question. It would be a protocol change, so it needs a research ADR (AGENTS.md rule 3).
4. **Last-resort replay window.** PQXDH §4.2 accepts replays of handshakes that used no one-time key. The prototype adds a bounded, device-local replay cache keyed by `SHA-256(tag ‖ EK_A ‖ CT)`. Its size, eviction and persistence need specifying, including its loss on backup restore (`ONE_TO_ONE_SESSION_MANAGEMENT.md` §7).
5. **The AEAD is not pinned.** The 2026-09-23 resolution says "misuse-resistant authenticated encryption" but names no primitive. The prototype uses AES-256-GCM-SIV. The profile ADR must pin it, together with the nonce-derivation rule.
6. **PQ healing is traffic-dependent.** In these runs an SPQR epoch took roughly 80 messages. PQ post-compromise recovery is slow for quiet chats (DR §8.9). The ADR-0107 "Post-quantum protection: on" wording should mean handshake/HNDL protection plus eventual PQ ratchet healing, not per-message PQ healing.
7. **Metadata.** The ratchet header (DH public key, counters, SCKA chunk) is plaintext. DR §4 header encryption is not used, and spqr has no header-encryption mode. V-015 should decide whether relays may see it.

## Proposed design updates

- File the draft ADR above, and add an OM-Crypto-v1 profile ADR that pins: the AEAD (AES-256-GCM-SIV), the KDF info strings, the EncodeEC/EncodeKEM type bytes, the SK split, the header/envelope CDDL, SPQR `min_version = V1`, and the downgrade invariants listed in the draft ADR, item 6.
- `ASYNC_KEY_MATERIAL.md` §6: specify "consume one-time keys only after the initial AEAD authenticates" (PQXDH §3.4) and the last-resort replay-cache semantics.
- `CRYPTOGRAPHY_AND_AGILITY.md` / ADR-0107 table: record the SPQR licence blocker and traffic-dependent PQ healing.
- `verification/FEASIBILITY_RECONNAISSANCE_20260923.md` V-001: add the spqr findings (licence, no crates.io, builds on wasm32, V0 downgrade risk).
- `machine/architecture_verification_gates.jsonl` V-001: add evidence paths (`verification/prototypes/v001-crypto/`, `verification/results/v001-crypto-latest.json`). V-022: note that the V-001 crypto costs are recorded.
- `machine/file_registry.json`: register this directory's files (run `python3 tools/om_registry.py sync`). The validator fails until then. This prototype did not touch `machine/`.
- `verification/README.md`: V-001 now has executable evidence.

## Not covered

- Multi-device session management (V-003), crossed initiations, and multi-provider disjoint pools. The prototype has one provider pool.
- DR header encryption, and secure deletion or zeroization guarantees beyond best effort (DR §8.1).
- Fuzzing of the CBOR/envelope decoders. The strict decoder has unit tests only.
- An independent SPQR implementation.
- Real low-end devices.
- Human review.
