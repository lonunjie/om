Feasibility prototype for V-001 (`production_crypto_path`) and the V-001 half of V-022 (`performance_budgets`). Not product code. Not for production.

# V-001 crypto path prototype

- **Authorship:** written by a Claude agent (verification executor under ADR-0134). The rules come from ADR-0118 and ADR-0131 (integrator); the byte-level constructions come from [ADR-0135](../../../decisions/ADR-0135-om-crypto-v1-construction-pinning.md) (the executor, independent review pending).
- **Gate effect:** evidence only. Per-criterion status is in [`verification/results/v001-crypto-latest.json`](../../results/v001-crypto-latest.json); the ledger is [`machine/architecture_verification_gates.jsonl`](../../../machine/architecture_verification_gates.jsonl).
- **History:** the 2026-09-24 version of this directory ran the Triple Ratchet with Signal's `spqr` crate. ADR-0118 moved SPQR to OM-Crypto-v2, so on 2026-09-25 the prototype became exactly OM-Crypto-v1. The old code is in git history (tag `checkpoint/20260925-0010`).

## What it is

A Rust implementation of OM-Crypto-v1, written from the public specifications:

| Layer | Specification | Where |
|---|---|---|
| Device identity signatures | Ed25519 (RFC 8032, strict) **and** ML-DSA-65 (FIPS 204, empty context) over DS-SIG bytes; composite layout per ADR-0135 §1 | `rust/core/src/sig.rs` |
| PQXDH binding | frozen W0 `AccountControlState` → `DeviceCredential` → `signed-AsyncKeyBundle` and signed `prekey-item`s (ADR-0131 rule 1, ADR-0135 §2) | `rust/core/src/binding.rs` |
| Async handshake | Signal **PQXDH** rev 3: curve25519, SHA-256, ML-KEM-768 (`libcrux-ml-kem`), OM info label; prekeys authenticated by the composite device key instead of XEdDSA | `rust/core/src/pqxdh.rs` |
| Last-resort replay ledger | exact per-epoch `handshake_id` set, cap 4096, no eviction, committed to the vault before plaintext (ADR-0131 rule 2) | `rust/core/src/ledger.rs`, `store.rs` |
| Ratchet | Signal **Double Ratchet** rev 4 §3 (KDF_RK / KDF_CK, MAX_SKIP 1000, cap 2000). No SPQR and no header encryption (ADR-0131 rule 4) | `rust/core/src/ratchet.rs`, `session.rs` |
| Payload AEAD | RFC 8452 AES-256-GCM-SIV; K/N from `OM-DR-AEAD-KEY-v1` / `OM-DR-AEAD-NONCE-v1` ‖ header hash; AD per ADR-0135 §4 | `rust/core/src/kdf.rs`, `session.rs` |
| MessageRequest HPKE | draft-ietf-hpke-pq-05 MLKEM768-X25519 / HKDF-SHA256 / ChaCha20Poly1305, base mode, with its normative references (hybrid-kems-12, concrete-hybrid-kems-03, hpke-hpke-03) | `rust/core/src/hpke.rs` |
| State and wire encoding | strict deterministic CBOR (RFC 8949 §4.2.1 subset, the [`CANONICAL_ENCODING.md`](../../../docs/architecture/crypto/CANONICAL_ENCODING.md) decoder rules) | `rust/core/src/cbor.rs` |
| Prekey publication | toy `KeyMaterialProvider`: one disjoint pool per device; a fetch returns `[signed bundle, one-time items…]` | `rust/core/src/store.rs` |

Other pieces:

- `rust/core/src/otk.rs`: 49 named selftest checks (binding, one-time keys, ledger, AEAD, HPKE).
- `rust/core/src/persist.rs`: a 249-step conversation (367 messages). Every step runs in a fresh process or instance, and out-of-order messages and skipped keys cross the reloads.
- `rust/core/src/vectors.rs` writes `vectors/om-v001-proto-vectors.json`. The file is deterministic and holds binding positives and 12 negatives, two session cases and the HPKE envelope.
- `python/check_vectors.py` + `python/hpke_pq.py` are the **second implementation**. They use a different language and different libraries: pyca/cryptography on OpenSSL for Ed25519, ML-DSA-65, ML-KEM, X25519, AES-GCM-SIV and ChaCha20Poly1305, plus kyber-py for derandomized ML-KEM. They include their own CBOR codec, Double Ratchet, binding verifier and HPKE. They also run the NIST ACVP ML-KEM-768 KATs and the draft-ietf-hpke-pq-05 KATs.
- `rust/core/src/acvp.rs` runs the official NIST ACVP ML-KEM-768 KATs against libcrux (native and WASI). `rust/core/src/hpke.rs` `check_kats` runs the draft-05 KATs (native, WASI and Node).
- `rust/node-wasm` + `node/run_wasm.mjs` build for wasm32-unknown-unknown and run it in Node/V8 through a raw ABI, with host entropy from `crypto.getRandomValues`.

Deterministic randomness exists only behind the `insecure-test-rng` cargo feature. Enabling `production` together with it is a `compile_error!` (R-072 sketch).

## How to run

```sh
cd verification/prototypes/v001-crypto && ./run.sh
```

- Needs rustup (toolchain pinned in `rust/rust-toolchain.toml`, wasm targets installed automatically), wasmtime, Node ≥ 22, uv, python3, curl and network access on the first run (crates, PyPI, ACVP files, the HPKE vector file).
- Pinned downloads: ACVP-Server `975de31e` (SHA-256 checked); hpkewg/hpke-pq `6433c8f` = tag `draft-ietf-hpke-pq-05` (`test-vectors.json` SHA-256 `35c59f4a…613c`).
- Output: [`verification/results/v001-crypto-latest.json`](../../results/v001-crypto-latest.json) is deterministic. Raw timings go to `rust/target/v001-perf.json`, which is not committed. The script exits non-zero if any check fails.

## Results

All nine V-001 pre-engineering criteria have passing executable evidence (see the results file):

| Criterion | Evidence |
|---|---|
| v1 state machine on native and wasm32 with persistence/reload | 249 fresh-process steps, 367 messages. Native, wasip1, alternating native/wasip1 and Node fresh-instance runs are byte-identical at every step. |
| vectors consumed by a second implementation / upstream KATs | Python reproduces or verifies 202/202 values and rejects a mutated ciphertext and a mutated ML-DSA-65 signature. ACVP ML-KEM-768 passes on libcrux and kyber-py. |
| one-time keys and depletion without downgrade | depletion to the signed ML-KEM prekey, single-use deletion, replay rejection, a tampered handshake consumes nothing, PQ-less bundles and CT-less messages are refused, epoch rotation with grace and expiry |
| dependency/licensing/update ADR | ADR-0118 + ADR-0135 §6; the resolved graph has no copyleft and no git dependency |
| no unsupported library API contract | only crates.io releases through documented APIs; spqr and libsignal are gone |
| dual Ed25519 + ML-DSA-65 binding | 16 Rust checks + 12 negatives the Python/OpenSSL verifier also rejects: either half substituted or alone, halves split into two entries, prekey / item / IK / signing-key substitution, quarantined device, stale or untrusted ACS |
| exact last-resort ledger | commit before plaintext; a failed commit changes nothing; replay after crash+reload rejected; 4096 real handshakes fill an epoch, the 4097th fails closed, no eviction, one-time initiations and fresh epochs still work |
| GCM-SIV derivation and authenticated header context | Python recomputes header_hash / K / N / AD for 16 messages; ciphertexts match bit-exactly; header and envelope-context tampering are rejected |
| MessageRequest HPKE KATs + OM context | draft-05 KATs 33/33 on Rust (native, WASI, Node) and Python; the OM info/ticket vector is reproduced; info tamper, X25519-half tamper and PQ-only enc are rejected |

Pre-release items stay open: external cryptographic review, and a release-pipeline proof for R-072 (the current sketch is partial).

## Performance (V-022)

Medians on an Apple M3 (8 GB), 160-byte payload:

| Operation | native | wasm32-wasip1 (wasmtime) | wasm32 in Node/V8 |
|---|---|---|---|
| message encrypt + decrypt | 13 µs | 41 µs | 42 µs |
| session setup, total (binding verify + PQXDH + first message) | 0.65 ms | 1.6 ms | 1.5 ms |
| one-time EC + PQ prekey generation incl. two composite signatures | 0.8 ms | 1.4 ms | 1.3 ms |
| device state reload | 10 µs | 26 µs | 25 µs |

All are far inside the [`MOTION_AND_PERFORMANCE.md`](../../../docs/architecture/ux/MOTION_AND_PERFORMANCE.md) budgets (5 ms per message, 50 ms per session setup), even after an assumed 10× low-end-device factor. No reference low-end device was measured, so V-022 stays partial.

## Not covered

- Multi-device session management (V-003) and multi-provider pools. One provider pool per device is exercised.
- Authenticating the initiator's IK_A through the initiator's own ACS. That belongs to the session manager (V-003) and IntroIdentityBinding (V-005/V-015).
- Real DeviceVault durability (fsync, OS keystore). The vault here is an in-memory trait with fault injection.
- Side channels and constant-time properties beyond what the chosen libraries claim.
