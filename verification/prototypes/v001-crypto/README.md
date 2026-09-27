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
| PQXDH binding | relationship-pinned `AccountControlState` → `DeviceCredential` → `signed-AsyncKeyBundle`; ADR-0140 generation floor/equivocation state and context-bound one-time `prekey-item`s | `rust/core/src/binding.rs` |
| Async handshake | Signal **PQXDH** rev 3: curve25519, SHA-256, ML-KEM-768 (`libcrux-ml-kem`), OM info label; prekeys authenticated by the composite device key instead of XEdDSA | `rust/core/src/pqxdh.rs` |
| Last-resort replay ledger | exact per-epoch `handshake_id` set, cap 4096, no eviction, committed to the vault before plaintext (ADR-0131 rule 2) | `rust/core/src/ledger.rs`, `store.rs` |
| Ratchet | Signal **Double Ratchet** rev 4 §3 (KDF_RK / KDF_CK, MAX_SKIP 1000, cap 2000). No SPQR and no header encryption (ADR-0131 rule 4) | `rust/core/src/ratchet.rs`, `session.rs` |
| Payload AEAD | RFC 8452 AES-256-GCM-SIV; K/N from `OM-DR-AEAD-KEY-v1` / `OM-DR-AEAD-NONCE-v1` ‖ header hash; AD per ADR-0135 §4 | `rust/core/src/kdf.rs`, `session.rs` |
| MessageRequest HPKE | draft-ietf-hpke-pq-05 MLKEM768-X25519 / HKDF-SHA256 / ChaCha20Poly1305, base mode, with its normative references (hybrid-kems-12, concrete-hybrid-kems-03, hpke-hpke-03) | `rust/core/src/hpke.rs` |
| Sealed delivery | ADR-0142 route-bound HPKE info/AAD, the same pinned hybrid suite, exact profile-1 key/encapsulation lengths, and five fixed plaintext buckets; opening is explicitly not sender authentication | `rust/core/src/delivery.rs` |
| State and wire encoding | strict deterministic CBOR (RFC 8949 §4.2.1 subset, the [`CANONICAL_ENCODING.md`](../../../docs/architecture/crypto/CANONICAL_ENCODING.md) decoder rules) | `rust/core/src/cbor.rs` |
| Prekey publication | toy `KeyMaterialProvider`: provider-distinct pools; generation remains global per device/profile; a fetch returns the signed bundle plus pool-bound one-time material | `rust/core/src/store.rs` |

Other pieces:

- `rust/core/src/otk.rs`: named selftest checks for binding, one-time keys, durable freshness, ledger, AEAD, MessageRequest HPKE and sealed delivery.
- `rust/core/src/persist.rs`: a 249-step conversation (367 messages). Every step runs in a fresh process or instance, and out-of-order messages and skipped keys cross the reloads.
- `rust/core/src/vectors.rs` writes `vectors/om-v001-proto-vectors.json`. The deterministic corpus includes binding positives and negatives, initiator-binding cases, two session cases, MessageRequest HPKE, ADR-0140 AsyncKeyBundle cases and an ADR-0142 sealed-delivery seal/open vector.
- `python/check_vectors.py` + `python/hpke_pq.py` are the **second implementation**. They use a different language and different libraries: pyca/cryptography on OpenSSL for Ed25519, ML-DSA-65, ML-KEM, X25519, AES-GCM-SIV and ChaCha20Poly1305, plus kyber-py for derandomized ML-KEM. They include their own CBOR codec, Double Ratchet, binding verifier and HPKE. They also run the NIST ACVP ML-KEM-768 KATs and the draft-ietf-hpke-pq-05 KATs.
- `rust/core/src/acvp.rs` runs the official NIST ACVP ML-KEM-768 KATs against libcrux (native and WASI). `rust/core/src/hpke.rs` `check_kats` runs the draft-05 KATs (native, WASI and Node).
- `rust/node-wasm` + `node/run_wasm.mjs` build for wasm32-unknown-unknown and run it in Node/V8 through a raw ABI, with host entropy from `crypto.getRandomValues`.

Deterministic randomness exists only behind the `insecure-test-rng` cargo feature. Enabling `production` together with it is a `compile_error!` (R-072 sketch).

## How to run

```sh
cd verification/prototypes/v001-crypto && ./run.sh
# after reviewing an intentional frozen-wire/vector change:
./run.sh --update-vectors
```

- Needs rustup (toolchain pinned in `rust/rust-toolchain.toml`, wasm targets installed automatically), wasmtime, Node ≥ 22, uv, python3, curl and network access on the first run (crates, PyPI, ACVP files, the HPKE vector file).
- Pinned downloads: ACVP-Server `975de31e` (SHA-256 checked); hpkewg/hpke-pq `6433c8f` = tag `draft-ietf-hpke-pq-05` (`test-vectors.json` SHA-256 `35c59f4a…613c`).
- Output: [`verification/results/v001-crypto-latest.json`](../../results/v001-crypto-latest.json) is deterministic. Raw timings go to `rust/target/v001-perf.json`, which is not committed. The script exits non-zero if any check fails.

## Current-tree ADR-0140 and ADR-0142 execution (2026-09-27)

The prototype source implements the ADR-0140 v1 bundle shape, durable recipient generation floor/equal-generation core-hash check, provider-pool migration semantics, pool-bound one-time-prekey signatures, and restore persistence for the floor. Runtime selftests accept a same-generation/same-core migration across provider pools, advance the floor, serialize and reload device state, reject a lower-generation replay and equal-generation core equivocation, and reject one-time-prekey transplantation across provider-pool, device, profile and generation contexts.

The prototype includes a bounded sealed-delivery HPKE path in `rust/core/src/delivery.rs`. It reuses the pinned draft-ietf-hpke-pq-05 hybrid implementation and binds the exact ADR-0142 route digest into `info`, and delivery-key id, delivery id, expiry and plaintext bucket into `aad`. Runtime cases cover correct seal/open, profile-1 public-key and encapsulation lengths, all five buckets, wrong route/key/expiry, tampered route/info/AAD/encapsulation/ciphertext, malformed bucket/padding lengths, and the rule that outer open is not sender authentication. The receive function returns the inner envelope with `sender_authenticated=false`; callers still must apply RelationshipContext, ACS/DeviceCredential and inner-session checks. There is no clear-sender fallback.

The full `run.sh --update-vectors` suite completed on the current tree. Before accepting regenerated vectors, the new output was compared with the committed pre-amendment corpus: AsyncKeyBundle signed bytes changed under ADR-0140, its retired-key negative cases and current initiator-binding vectors were refreshed, and the ADR-0142 sealed-delivery vector was added. Native, wasip1 and Node/V8 outputs matched byte-for-byte, and the independent Python checker verified the regenerated corpus. Results are current-tree execution evidence, not the pre-ADR-0140 record.

## Current executed results

The latest complete V-001 run recorded passing executable evidence for the current-tree criteria (see the results file):

| Criterion | Evidence |
|---|---|
| v1 state machine on native and wasm32 with persistence/reload | 249 fresh-process steps, 367 messages. Native, wasip1, alternating native/wasip1 and Node fresh-instance runs are byte-identical at every step. |
| vectors consumed by a second implementation / upstream KATs | Python reproduces or verifies 236/236 values, including the ADR-0142 sealed-delivery vector, and rejects mutated ciphertext/signature controls. ACVP ML-KEM-768 passes on libcrux and kyber-py. |
| one-time keys and depletion without downgrade | depletion to the signed ML-KEM prekey, single-use deletion, replay rejection, a tampered handshake consumes nothing, PQ-less bundles and CT-less messages are refused, epoch rotation with grace and expiry |
| dependency/licensing/update ADR | ADR-0118 + ADR-0135 §6; the resolved graph has no copyleft and no git dependency |
| no unsupported library API contract | only crates.io releases through documented APIs; spqr and libsignal are gone |
| dual Ed25519 + ML-DSA-65 binding | 20 Rust checks + 14 negatives the Python/OpenSSL verifier also rejects: either half substituted or alone, halves split into two entries, prekey / item / IK / signing-key substitution, quarantined device, stale or untrusted ACS |
| exact last-resort ledger | commit before plaintext; a failed commit changes nothing; replay after crash+reload rejected; 4096 real handshakes fill an epoch, the 4097th fails closed, no eviction, one-time initiations and fresh epochs still work |
| GCM-SIV derivation and authenticated header context | Python recomputes header_hash / K / N / AD for 16 messages; ciphertexts match bit-exactly; header and envelope-context tampering are rejected |
| MessageRequest HPKE KATs + OM context | draft-05 KATs 33/33 on Rust (native, WASI, Node) and Python; the OM info/ticket vector is reproduced; info tamper, X25519-half tamper and PQ-only enc are rejected |
| AsyncKeyBundle freshness/context binding | Current Rust/native, wasip1 and Node selftests exercise cross-provider migration, persisted floor after reload, lower-generation rollback, equal-generation core equivocation and item transplantation across pool/device/profile/generation; independent Python verifies regenerated vectors |
| Sealed delivery | Rust/native, wasip1 and Node selftests plus independent Python exercise ADR-0142 profile, route-bound info/AAD, all five buckets, wrong route/key/expiry, tampering and the outer-open/authentication boundary |

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

- Full multi-device session management remains V-003. V-001's bounded ADR-0140 freshness/context criterion now has current runtime evidence; real DeviceVault durability (fsync, OS keystore) remains outside this prototype.
- (Covered since 2026-09-26, review F-9.) The responder binds the initiator's IK_A to exactly one active,
  non-quarantined DeviceCredential committed by a trusted relationship peer's ACS head before any secret-key
  operation (`binding::verify_initiator`, `Device::trust_peer`, ADR-0136 rule 4). Nine selftest checks and eleven
  `initiator_binding` vectors cover unknown, route-mismatched, revoked, quarantined, stale, substituted and ambiguous
  keys, and crossed initiation. The Python checker verifies the vectors independently.
- Side channels and constant-time properties beyond what the chosen libraries claim.
