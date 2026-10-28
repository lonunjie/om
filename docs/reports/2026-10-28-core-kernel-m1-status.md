# core_kernel M1 status (pass 1 of 2)

- **Date:** 2026-10-28
- **Scope:** `core_kernel` (ADR-0169). Author: a Claude Sonnet 5.5 subagent; a second Sonnet pass and a later Opus audit follow.
- **Goal:** the real OM Core as Rust crates compiled to WASM, behind the same typed boundary as the dev harness (task C1 in [`2026-10-11-reference-client-ui-first-web-tasks.md`](2026-10-11-reference-client-ui-first-web-tasks.md)).

## What exists

| Piece | Where | Notes |
|---|---|---|
| `om-codec` | `crates/om-codec` | Strict OM-CBOR-v1 codec and the registry-driven W0 validator, adapted from the V-005 prototype. Replays the committed 758-vector W0 corpus (verdict, exact error class, byte-identical re-encode, derived signature/hash inputs) and the CBOR fixtures. |
| `om-core` crypto | `crates/om-core/src/crypto` | OM-Crypto-v1 (PQXDH, Double Ratchet with AES-256-GCM-SIV, dual Ed25519 + ML-DSA-65 device binding, exact last-resort ledger, PQ/T HPKE, sealed-delivery HPKE) adapted from V-001. Replays the V-001 selftests and reproduces the committed V-001 vector corpus byte for byte. Secret-holding types zeroize on drop and redact `Debug`. |
| `om-core` engine | `crates/om-core/src/{api,engine,flows,state,host}.rs` | Sans-IO state machine. The UI-facing types in `api.rs` mirror `packages/om-platform-api/src/core`. |
| `om-wasm` | `crates/om-wasm` | One JSON-in/JSON-out entry point over a raw ABI (no wasm-bindgen), host entropy and clock imports, all `unsafe` code confined to `abi.rs`. Release build is about 2.3 MB. |
| `@om/core-wasm` | `packages/om-core-wasm` | `CoreClient` over the wasm module, plus the async driver (commit-before-release, network outbox, polling). `test-support` holds an in-memory provider and store, test only. |
| Guards | [`tools/core_guard.py`](../../tools/core_guard.py), [`tools/web_guard.py`](../../tools/web_guard.py), [`apps/web/build/check-no-harness.mjs`](../../apps/web/build/check-no-harness.mjs) | Test-only providers cannot reach a production build; the harness stays forbidden; the real Core is permitted. |
| Make | `make core-check`, `make core-wasm`, `make core-guard` | `core-guard` is in `make check`; `core-check` and `core-wasm` are in `make verify`. |

## Design as built

- **No storage and no network inside Core.** Every mutating call stages a transaction and returns a `Commit` (one atomic record write per profile, so a single `EncryptedKv.put` is a whole transition). State and effects are released only after the host acknowledges the commit (`finish_commit(id, true)`). A refused write discards the stage: no outcome, no plaintext and no network effect existed.
- **Persisted outbox.** Network work is listed from state (`net_ops`), identified by a stable id, and survives restart. `Unavailable` keeps `PendingNetwork`; a `Timeout` after a deposit is `PendingReconciliation` and is retried with the same envelope bytes and the same operation id; a relay redelivery is deduplicated, not reported as an integrity failure.
- **Profile namespacing.** One record per profile in that profile's encrypted store, no shared identifier. A context captured before close, reopen or restart is stale (per-boot nonce in the generation) and fails closed with an error, not an outcome.
- **Outcome classes** are exactly those in the contract. Destructive intents without a confirmation token are `Rejected(unauthorized)`.
- **First contact (pass-1 shape).** Inviter publishes an invite descriptor to the introduction service; the link `om:invite/<80 bytes base64url>` commits to the invite id, the ACS head hash and the request-key digest, so the service cannot substitute either. The invitee verifies, pins the inviter's ACS head, and deposits a W0 `MessageRequest` (validated by `om-codec`) HPKE-sealed with the pinned draft-ietf-hpke-pq-05 suite. The inviter sees an `IntroductionRequest`, pins nothing and can decrypt nothing until `AcceptIntroduction`; then the inviter fetches the invitee's key material, runs PQXDH and sends the first session message. The relationship context is mandatory for every inbound session envelope (ADR-0137).
- **Dependencies added:** none beyond the V-001/V-005 prototype set (RustCrypto, dalek, libcrux ML-KEM/ML-DSA, `getrandom` custom backend on wasm32). New non-crypto crates: `serde`, `serde_json`, and dev-only `flate2` (rust backend, to read the gzipped corpus) and `proptest`. No new crypto dependency, so nothing here needs operator approval.

## Tests that prove it

- `cargo test --workspace`: 3 W0/CBOR tests (758 vectors), 2 V-001 tests (all named selftests plus the full vector corpus), 1 W0-validity test of Core's published objects, 13 Core scenarios (including a 16-case property test: exactly-once, per-sender-ordered delivery under random sends, restarts and lost acks), 2 JSON-bridge tests.
- `pnpm test` (286 tests, all pass; the existing UI suites are unchanged), including 10 in `@om/core-wasm`: the real wasm Core through `CoreClient` (onboarding, invite, introduction, two-way messaging across restarts, refused store write, timeout after send, profile-bound contexts), a contract test run against both the harness and the real Core, and the unchanged `OnboardingFlow` UI running against the real Core.
- Not run: Playwright e2e against the real Core (see "What remains").

## Spec questions and deviations (all resolved conservatively; none silently)

1. **Messages are not yet `ConversationEvent`s.** [`EVENT_MODEL.md`](../architecture/events/EVENT_MODEL.md) defines canonical events with EventIDs; pass 1 carries an internal payload (`kind 1` text, `kind 2` hello) inside the ratchet and dedupes by a sender-chosen 128-bit message id. Highest priority for pass 2, because it changes what is signed, stored and synced.
2. **No sealed-sender wrapping.** The relay sees the sender's `EncodeEC(IK_A)` and the ratchet header on every envelope (the prototype envelope shape). `delivery.rs` (ADR-0142) is ported and vector-checked but not wired because per-relationship delivery-key exchange and its lifecycle are not built. Security-relevant metadata gap; record before any release.
3. **AccountControlState is the unsigned one-device body of the V-001 prototype.** No controller signatures, no `AccountControlHead`, no generation chain, so "the pinned ACS head" is a hash of an unsigned body. The invite link authenticates it for first contact; real continuity needs `om-control` and the formal-model CI coverage the constitution requires.
4. **First contact is not the full IntroGate flow.** Not built: signed `ContactDescriptor` with an intro identity, `IntroductionTicket`, signed `MessageRequestPayload`, `IntroIdentityBinding`, admission proofs. The link commits to the ACS head and request-key digest instead of the spec's "request key trusted only when signed by the intro identity", and `ticket_id` carries the invite id. Spec question: may an invite link commit to the request key digest directly, or is an intro-identity signature mandatory even for invite links? Pass 1 chose the stricter-than-spec-silent reading (nothing is accepted that the link does not commit to) but it is not the specified object set.
5. **Who initiates the session after acceptance.** The spec says both sides establish PQXDH sessions; pass 1 has the accepting inviter initiate exactly one and the invitee answer on it. Simultaneous-initiation policy is V-003 territory and is not exercised.
6. **Invites are single-use.** The first valid request marks the invite used (conservative; `max_uses` per IntroGate is unmodelled). A hostile first requester can burn an invite; real rate and admission controls are IntroGate work.
7. **Recovery key.** [`RECOVERY_AND_BACKUP_POLICY.md`](../architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) calls for a 256-bit checksummed key plus locator, but no display format. Pass 1 shows 56 base32 characters (32 random bytes plus a 3-byte check) in 14 groups of 4. Core does **not** retain the RecoveryRoot after display and does not bind it to a `RecoveryPolicy` commitment, so recovery cannot be performed yet. `GetRecoveryStatus` therefore reports `policy: "pending"` and never `on`.
8. **Delivery receipts.** `DeliveryState.delivered` needs a receipt event the spec does not define for 1:1; outgoing messages stop at `sent`.
9. **Prekey housekeeping.** Bundles carry a 90-day `expires_at` but there is no republish/rotation timer and no one-time-prekey replenishment; after the first 10 each are consumed the signed last-resort keys are used (the ledger covers that path). A retried `FetchKeyMaterial` can consume a second one-time prekey.
10. **Wall-clock use.** `Clock` feeds display timestamps, invite and bundle expiry hints only. Peer-supplied `sentAt` is shown as given and orders nothing.

## What remains for pass 2

1. Canonical `ConversationEvent` messages and EventIDs (question 1); edits, deletes, reactions as event types.
2. Sealed-sender delivery with relationship delivery keys (question 2).
3. `om-control`: signed `AccountControlState`/`AccountControlHead`, device enrollment, **device linking** (`OfferDeviceLink`, `RequestDeviceLink`, `ApproveDeviceLink`, `RejectDeviceLink`, with enrollment notices), `SuspendDevice`, `RevokeDevice`, and recovery (`StartRecovery`, `VetoRecovery`, RecoveryPolicy commitment). All of these currently answer `Rejected(unsupported)`; `GetDevices` shows the single current device and `GetLinkRequest`/`GetLinkOffer` return null.
4. The full IntroGate object set (question 4) and `ShareContact`, `BlockPerson` with provider scope, `ChangePrivacyPolicy`.
5. Per-conversation records and operation-log compaction (state is one record per profile), bundle republish and prekey replenishment timers, an `EncryptedKv` batch API if more than one record per transition is ever needed.
6. C1 wiring: `apps/web` production entry using `@om/core-wasm` with a real provider adapter (reference Home is `provider_reference` scope), the wasm bundled and its hash added to the signed-bundle manifest, and Playwright e2e of two browser profiles. `main.tsx` is deliberately unchanged in pass 1.
7. Consolidate the two CBOR codecs (`om_codec` strict codec vs the V-001 `crypto::cbor` used for state and crypto objects); fuzz targets under `fuzz/`; `conformance/` runner; wasm size and startup measurements against the V-022 budgets; a second CryptoProfile-agility test.
8. Independent review of the crypto adaptations (ADR-0167): the code is adapted from prototypes written by a Claude agent and reviewed by Claude only.
