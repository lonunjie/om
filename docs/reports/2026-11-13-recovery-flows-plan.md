# Recovery flows (work group E): design reading, findings and implementation plan (2026-11-13)

Status: **no code written.** This session read the recovery design, mapped it onto the current Core and stopped before coding
because the design gate cannot be met in one context together with the build (see "Why it stopped"). Everything below is
what the next session needs so it does not re-derive it.

## Design sources read

Read in full, in this session's own context:

- [`AGENTS.md`](../../AGENTS.md) (lonunjie root), `om/AGENTS.md` (Design gate), handoff [`2026-11-04-handoff-next-week.md`](2026-11-04-handoff-next-week.md), plan report group E row,
  [`2026-11-13-multi-device-phase5-status.md`](2026-11-13-multi-device-phase5-status.md).
- **identity_control_recovery, all 7 documents:** RECOVERY_AND_BACKUP_POLICY, ACCOUNT_CONTINUITY_AND_CONTROL, IDENTITY_PROTOCOL,
  IDENTITY_CONTINUITY, IDENTITY_DISCOVERY_VERIFICATION, IDENTIFIER_SPOOFING_AND_DISPLAY, ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.
- **crypto_1to1, all 6 documents:** CRYPTOGRAPHY_AND_AGILITY, PRODUCTION_CRYPTO_INTEGRATION_GATE, ASYNC_KEY_MATERIAL,
  ONE_TO_ONE_SESSION_MANAGEMENT, MULTI_DEVICE_SYNC, CANONICAL_ENCODING.
- **discovery_first_contact, all 5 documents:** DISCOVERY_FABRIC, INVITES_INTROGATE_AND_SPAM, CONTACT_SHARING,
  INTRODUCTION_AND_SERVICE_ACCOUNTS, KEY_TRANSPARENCY_AND_DIRECTORY.
- **relay_reachability, all 3 documents:** RELAY_AND_DELIVERY_ARCHITECTURE, MULTI_RELAY_AND_DELIVERY_RESILIENCE, REACHABILITY_AND_OBJECT_REPAIR.
- **metadata_privacy, 3 of 6:** PRIVACY_GOVERNANCE_AUTHORITY, NOTIFICATIONS_AND_PUSH, STORAGE_AND_BLOB_ARCHITECTURE.
- Code read: `crates/om-core/src/control.rs` (authorize, verify_successor, plan), the recovery stubs in `api.rs`, `engine.rs`, `flows.rs`,
  genesis creation in `flows.rs` and `flows/ceremony.rs`; the [`account.cddl`](../../spec/cddl/account.cddl) recovery objects; the domain-separation registry;
  the UI/harness recovery contract (`om-platform-api/src/core/devices.ts`, `om-core-harness/src/devices.ts`, `om-ui/src/recovery`).

**Not read (so a commit claiming them is not yet honest):** metadata_privacy: SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION,
METADATA_THREAT_MODEL; **time_ordering_events, all 4:** EVENT_MODEL, TIME_AND_EXPIRY_SEMANTICS, MESSENGER_BASELINE_V1,
SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION; **all listed ADRs** (the recovery-relevant ones, 0111, 0119, 0127, 0131, 0137,
0138, 0141, 0147, 0154, 0168, were only seen through the documents' summaries). `design_gate.py needs crates/om-core` maps six
domains, so a Core commit needs all of these read. They cost about 100k tokens on top of the 150k already spent.

## What the design requires (recovery, as it maps to Core)

1. Every identity is created with a working `RecoveryPolicy` (default on): method `recovery_key` (a random 256-bit RecoveryRoot,
   shown once), recovers identity, provider_metadata, relationships; activation delay 72 h (min 24 h, veto by any live
   controller); its hash `OM-RECOVERY-POLICY-HASH-v1` is ACS key 10. The root never leaves the person, there is no wrapped
   secret anywhere.
2. `RecoveryRequest` (signed by the recovery method, bound to target device key, epoch, parent state hash, nonce) goes to every
   route of the account's Home(s); every live controller shows "was it you?"; a controller co-sign completes it at once, a
   `RecoveryVeto` (signed by a controller) aborts it, silence for the delay completes it. The delay is **verifier-local**
   (ADR-0131/0147/0154): each verifier starts its own interval at its first valid observation.
3. Recovery is one vote toward the parent majority (ADR-0127); never resolves a fork or a freeze alone; a pending request is bound
   to its target device and epoch, and completion is rejected if that device was enrolled or revoked meanwhile (V-002 finding).
4. After completion: new device is the sole controller, every earlier controller is revoked, epoch increments, contacts see a
   continuity notice. History restore (backup file) and provider reconnect stay separate tasks (done separately: ADR-0184).
5. Backups never restore authority; a restored device uses a fresh DeviceID (already built).

## What the code has today

- `control.rs::authorize` already implements the recovery-vote rows (`Ctx.recovery { elapsed, delay, veto }`, `Verdict::Pending`)
  and is differential-tested against the V-002 mirror. **`verify_successor` does not use it:** it passes `Ctx::default()`, so no
  recovery-voted successor can be verified from real signed states. `open_signed` accepts only role `controller` signatures.
- Genesis commits **no** policy: `recovery_commitment: None` (`flows/ceremony.rs` ~1924, `crypto/store.rs` ~342). The recovery key
  shown at creation (`flows.rs` ~842) is 32 random bytes turned into display groups and then zeroized: nothing is derived from it.
- `RecoveryStatusView` is a fixed `Pending` stub (`engine.rs::recovery_status`); `StartRecovery` and `VetoRecovery` answer
  `unsupported` (`flows.rs` ~750). The UI and the harness (`devices.ts`) already model requests, delay, veto and cancel, against a fake.
- No Home support for `RecoveryRequestFanout`, `RecoveryRecordPut/Get` (the wire shapes are frozen in [`home-api.cddl`](../../spec/cddl/home-api.cddl); nothing in
  `infra/cloudflare-home` or `om-home-wire` implements them).
- `StartRecovery{scope, custodian, confirmation}` has **no field for the recovery key or the account locator**, so it can describe
  "stop/start from a device that already has a profile" but not "I have lost everything, here is my key".

## Design gaps found (answer or record before coding)

- **G1. The key that signs a RecoveryRequest is not specified.** The policy method carries a public `commitment`
  (`recovery-method` bstr 32..2592 bytes), the object type says signature role `recovery`, and RecoveryRoot derives the record
  keys (labels `OM-RECOVERY-*-KEY/CAP-v1`). No document or registry entry says how the recovery **signing** key pair
  (Ed25519 + ML-DSA-65, like every durable signature) is derived from the RecoveryRoot, nor the label. Deriving it needs a new
  domain-separated label, which [`PRODUCTION_CRYPTO_INTEGRATION_GATE.md`](../architecture/crypto/PRODUCTION_CRYPTO_INTEGRATION_GATE.md) section 2 ("undocumented key derivation/domain separation")
  puts behind review. Proposed answer, consistent with the existing pattern: add `OM-RECOVERY-AUTH-KEY-SEED-v1`
  (HKDF-SHA256 from RecoveryRoot; info = label || recovery_rotation_epoch; output 64 bytes: 32 Ed25519 seed, 32 ML-DSA-65 seed
  as the device signer already does), the method `commitment` = the composite public key (1984+32 bytes, in the allowed range).
  Needs the operator's yes (new label) and goes to the non-Anthropic review list.
- **G2. The record the recovering device sends and how controllers receive it.** `RecoveryRequestFanout` goes to Homes; how a
  controller device learns of it (poll of a well-known per-account queue? the account's `RelaySet`-less fan-out?) is not defined
  beyond "delivered to every route of the account's Home(s)". A recovering device has no sessions and no capabilities, so this needs
  a Home endpoint keyed on something it can derive from the RecoveryRoot (`recovery_scope_id`, ADR-0138). `provider_reference` work.
- **G3. Entering the key.** UX_BRIEF and SCREEN_MAP only describe Settings -> Recovery on an existing profile. A fresh device needs
  an intent to hand Core the recovery key (checksummed groups) plus the locator (Home address); there is no such intent today.
  ADR-0138's `RecoveryScope` id plus the Home address is the likely locator.

None of these is a reason to improvise; G1 is the only one that blocks Core code.

## Plan (slices, each testable without a Home)

**Slice 1 - Core primitives and truthful state.**
1. `om-core/src/recovery.rs`: RecoveryRoot (32 bytes) -> signing seeds (G1), RecoveryPolicy encode/hash (CDDL `RecoveryPolicy`),
   `RecoveryRequest`/`RecoveryVeto` encode, sign (`OM-RECOVERY-REQUEST-SIG-v1` role 2 recovery / `OM-RECOVERY-VETO-SIG-v1` role 1
   controller), verify, `request_digest` (`OM-RECOVERY-REQUEST-HASH-v1`). Bounded decode, golden-style tests, negative cases
   (wrong epoch, wrong parent hash, wrong role, trailing bytes).
2. Genesis commits the policy hash; the policy body is kept in `AppState` (public). Existing profiles stay `pending` (no commitment);
   adding a policy to an existing account is the `recovery_policy` row (parent majority) and is a later slice.
3. `GetRecoveryStatus` reports `on` once the key is confirmed stored and the committed policy matches; custodian `recovery_key`.
   Update `tests/*` that pin genesis bytes or vectors.

**Slice 2 - verifier.**
4. `control::verify_recovery_successor(parent, child_signed, policy, request, observation)`: checks the recovery signature against
   the parent's committed policy (hash match), the request binding (target device credential digest in the child, epoch, parent
   hash), controller co-signers (device-only quorum plus the vote), and the verifier-local elapsed interval (monotonic runtime
   input from the host; ADR-0147/0154), then calls `authorize` with `Ctx.recovery`. Child shape: new device sole controller,
   all earlier controllers and messaging devices moved to `permanently_revoked_devices`, epoch incremented. Rejects: stale target
   (device enrolled or revoked meanwhile), veto observed, elapsed < delay without co-sign, recovery vote on a fork bridge or freeze
   alone. Tests mirror V-002 rows and ID-03, ID-06.

**Slice 3 - live controller side.**
5. Incoming request observation record (first-observation time, request digest, monotonic checkpoint), `VetoRecovery` signs a
   `RecoveryVeto`, controller co-sign path (`That was me`) produces an immediate completion. Requests listed in
   `RecoveryStatusView.requests` with `activatesAt` from this verifier's own interval. Transport as an injected
   `NetCall::RecoveryRequestFanout` stub until the Home implements it.

**Slice 4 - recovering device and Home.** G2/G3: new intent, Home endpoints (`provider_reference`), restricted recovery state,
RecoveryRecord put/get, wasm types, UI entry, e2e. Separate package.

## Why it stopped

The gate wants every mapped document read in the committing session's own context. Six domains are about 250 KB of documents plus
about 120 KB of ADRs (roughly 120k tokens by the handoff's own estimate), and this Core touches all six. This session reached
58% of its context after reading the identity, crypto, discovery and relay domains plus the code map. Reading the rest and then
building Slice 1 to 3 in the same context would have forced a compaction in the middle of the security-critical code, which is the
wrong place for one. Plan budget was never the limit (5% of the 5-hour window used).

## Next session (recommended brief)

Fresh context, Sonnet high, work in `om/`. Read in this order and claim only what was read: the seven identity documents (fast: they
are this report's section 2 source), the six crypto, then the other four domains and the listed ADRs (start with 0111, 0119, 0127,
0131, 0137, 0138, 0141, 0147, 0154, 0168). Get the operator's answer to G1 first (one yes/no), then build Slice 1 and 2 (about
600 lines of Rust plus tests), commit with `Design-Read` for all six domains. Slice 3 follows if the window allows. Run the full
checks listed in the task (cargo test -p om-core, wasm conformance, vitest, tsc, e2e where the UI changed).

## G1 answered (2026-11-13, operator: "Yes, add the label")

New registry label `OM-RECOVERY-AUTH-KEY-SEED-v1` (kdf): HKDF-SHA256 from RecoveryRoot, info = label || recovery_rotation_epoch (uint, big-endian 8 bytes),
64-byte output = 32-byte Ed25519 seed followed by 32-byte ML-DSA-65 seed; the `recovery-method` commitment is the composite
public key. This is a gap answer, not a change to audited text. It is added to the non-Anthropic review list (ADR-0167): the construction follows the
existing `OM-RECOVERY-*` HKDF pattern but is new key derivation and has had no independent review.

### Reading ledger update (same session, after the G1 answer)

Additionally read in full: METADATA_THREAT_MODEL, TIME_AND_EXPIRY_SEMANTICS, EVENT_MODEL. Still unread: SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION,
MESSENGER_BASELINE_V1, SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION and every listed ADR (about 85k tokens). At 67% of the context the build of
Slices 1 and 2 would not fit after them, so no code was written; a Core commit needs `Design-Read` for all six domains, and the commit hook requires it.
The registry label `OM-RECOVERY-AUTH-KEY-SEED-v1` is not yet added to [`spec/registries/om-domain-separation.json`](../../spec/registries/om-domain-separation.json) (that goes in with the code, Slice 1).
Key time rule learned from TIME_AND_EXPIRY_SEMANTICS and RECOVERY section on ADR-0131/0154: the recovery delay is a durable verifier-local monotonic lower bound
(boot-session segments, no credit for unmeasured downtime); web verifiers may add bounded wall-clock gap credit only under Recommended and only with a final veto sweep.

## Slices 1 and 2 built (2026-11-13)

**Design sources read (committing session, in full):** all six Core domains for `recovery.rs`, `state.rs`, `api.rs` per `design_gate.py needs`:
crypto_1to1 (6 docs), discovery_first_contact (5), identity_control_recovery (7), metadata_privacy (6), relay_reachability (3), time_ordering_events (5),
and the listed ADRs 0002-0004, 0008, 0010-0012, 0019-0022, 0025-0030, 0033, 0040, 0063, 0065-0067, 0072-0073, 0075-0076, 0087-0088, 0098, 0105-0107, 0111,
0118-0121, 0124, 0126-0130, 0140, 0157, 0159. **Not read:** ADRs 0131, 0147, 0154, 0161, 0168 (cited by the documents, taken from their summaries) and documents outside the mapped domains.
The code was written by a worker subagent from the plan and reviewed, tested and committed by this session.

**Built.**
- Slice 1: `crates/om-core/src/recovery.rs` (RecoveryRoot, G1 key derivation `OM-RECOVERY-AUTH-KEY-SEED-v1`, RecoveryPolicy encode/hash, RecoveryRequest/RecoveryVeto sign/verify/digest, bounded decode).
  Genesis commits the policy hash; the shown recovery key is the RecoveryRoot; `GetRecoveryStatus` is `on` only if the stored policy matches the committed hash, old profiles stay `pending`.
- Slice 2: `control::verify_recovery_successor` (recovery signature against the committed policy, request binding, stale target, veto, verifier-local elapsed from the host, sole-controller child shape, commitment rotation, then `authorize` with `Ctx.recovery`).
- Registry: `OM-RECOVERY-AUTH-KEY-SEED-v1` added to `om-domain-separation.json`.

**Non-Anthropic review list (ADR-0167), new item:** `OM-RECOVERY-AUTH-KEY-SEED-v1` (HKDF-SHA256 from RecoveryRoot, no salt, info = label || epoch, 64 bytes = Ed25519 seed || ML-DSA-65 seed) is new key derivation without independent review, together with the recovery-role signature framing below.

**Choices the design is silent on (record, confirm before wire freeze of any of it):** the recovery-role signature signer field is the AccountID, over `OM-ACS-SIG-v1` with role 2;
the HKDF has no salt; `verify_recovery_successor` takes the rotated `next_policy` body as an input; any intervening transition makes a request stale (stricter than V-002 wording).
**Possible design conflict:** with N >= 4 controllers, the recovery vote plus one co-signer is below the majority, so `authorize` rejects it instead of falling back to the delay path, while RECOVERY section 10.3 says one co-sign completes immediately. Needs an operator ruling.

**Remains.** Slice 3 (controller side: observation records, `VetoRecovery`, co-sign, `RecoveryStatusView.requests`, injected `NetCall::RecoveryRequestFanout` stub); `verify_chain` cannot replay a completed recovery link yet (needs policy and request per link); Slice 4 (G2/G3, Home endpoints, UI entry).
No test yet for a legacy profile reporting `pending`. `om-core-wasm` `vitest run src` fails on an unrelated stale `dist` test file; `src/conformance.test.ts` and `src/ui.test.tsx` pass.

## Slice 3 built (2026-11-13)

**Design sources read:** same as "Slices 1 and 2 built" above (all six Core domains and the listed ADRs; ADRs 0131, 0147, 0154, 0161, 0168 only through the documents' summaries). Code by a worker subagent, reviewed, tested and committed by this session.

**Built.** `flows/recovery_watch.rs`: persisted observation record per request (first observation, boot session, credited monotonic ms, vetoes, successor; bounded at 8). Verifier-local interval: only growth within one boot session is credited
(`host::MonotonicSample`); downtime, a new session's first reading and backwards readings credit nothing; lost state restarts. Engine entries `recovery_observe_request/veto/successor`, `recovery_tick`.
`VetoRecovery` signs and records a veto (live controllers only); new intent `ConfirmRecovery` ("That was me") adds this controller's co-signature and verifies an immediate completion; `RecoveryStatusView.requests` has `activatesAt`, `state` (incl. `stale`), `canStop`, `ready`.
Stub net calls `RecoveryRequestFanout` and `RecoveryVetoFanout` (Home does not implement them). 204 om-core tests pass; wasm, om-ui, apps/web, harness and tsc green.

**Choices the design is silent on:** the successor travels in memory as `recovery::SuccessorBundle {child, next_policy, credential}` (no CDDL carrier); a veto fan-out call was added because the Home API freezes no veto endpoint;
a veto arriving before its request is ignored (needs Home redelivery); a completed recovery is stored as `Completed` on the observation and NOT appended to `acs_chain`, because `verify_chain` cannot replay a recovery link, so this device still acts as controller afterwards, the policy is not rotated and contacts are not notified.

**Remains (Slice 4, plus):** `verify_chain` support for recovery links (needs policy and request per link) so completion enters the chain; G2/G3 (Home endpoints for fan-out and RecoveryRecord put/get, recovering-device intent with key and locator entry, restricted recovery state, wasm ingest, UI entry, e2e); operator ruling on the N>=4 co-sign case; test for `VetoRecovery` by a quarantined self.

**Slice 3 commit scope (1afadd8):** Rust Core only. The TypeScript mirrors (11 files under `apps/web` and `packages/om-core-harness`, `om-core-wasm`, `om-platform-api`, `om-provider-http`, `om-ui`) are verified green by the worker
(vitest, tsc) but left UNCOMMITTED in the working tree: they map to `client_endpoint_distribution`, `constitution_governance`, `provider_deployment_supply_chain` and `ux_accessibility_personalization`, which no session has read for this work.
A session that reads those four domains (about 40 documents) commits them with `git add apps packages`; `git diff` shows them.

**Handoff for the 11 TS mirror files.** Backup patch: `recovery-slice3-ts-mirrors.patch` in the shared side-session notes folder (`git apply` from `om/`). Domains to read in full before committing them (from `design_gate.py needs apps packages`):
`client_endpoint_distribution`, `constitution_governance`, `provider_deployment_supply_chain`, `ux_accessibility_personalization`, plus the six Core domains (already listed above). Then verify (`pnpm exec vitest run src` in om-ui, apps/web, om-core-harness, om-platform-api; `pnpm exec tsc -b apps/web`) and commit with `Design-Read` listing all ten domains.
