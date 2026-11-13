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
- Registry: `OM-RECOVERY-AUTH-KEY-SEED-v1` added to [`om-domain-separation.json`](../../spec/registries/om-domain-separation.json).

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

## TS mirrors: reading ledger and per-file split (2026-11-13, second attempt)

**Design sources read this session, in full:** `client_endpoint_distribution`, `ux_accessibility_personalization`, `discovery_first_contact`, `relay_reachability` (all listed docs and ADRs). **Not read:** `identity_control_recovery`, `crypto_1to1`, `metadata_privacy`, `time_ordering_events`, `constitution_governance`, `provider_deployment_supply_chain` (about 720 KB). Nothing was committed from the TS mirrors.

Per-file domains (`design_gate.py needs <file>`):

| Files | Domains | Unread among them |
|---|---|---|
| `apps/web/src/copy/en.ts`, `en.test.ts` | client_endpoint, discovery, relay, ux | none |
| `packages/om-ui/src/actions/registry.test.ts`, `devices/__fixtures__/copy.ts` | client_endpoint, discovery, ux | none |
| `om-core-harness` `devices.ts`, `harness.ts`; `om-core-wasm` `test-support.ts`, `types.ts`; `om-platform-api` `core/devices.ts`, `core/intents.ts` | crypto_1to1, discovery, identity, metadata, relay, time_ordering | crypto_1to1, identity, metadata, time_ordering |
| `packages/om-provider-http/src/provider.ts` | constitution, discovery, metadata, provider, relay | constitution, metadata, provider |

**Why the four read-only-domain UI files were not committed alone:** they import `RecoveryRequestState` with `stale` (`om-platform-api/src/core/devices.ts`) and the `ConfirmRecovery` destructive kind (`intents.ts`), both in the unread-domain group, so at HEAD they fail tsc and their tests. A green commit needs those two files too, which need identity, crypto, metadata and time_ordering. Smallest committable set: the UI files + `devices.ts` + `intents.ts` + the other core-side mirrors that depend on them; its unread domains are the four above. `provider.ts` additionally needs constitution and provider.
Suggested order for a later session: read identity_control_recovery, crypto_1to1, metadata_privacy, time_ordering_events (~460 KB), commit everything except `provider.ts` with Design-Read listing the eight domains, then read constitution and provider docs for `provider.ts`.

## TS mirrors committed (2026-11-13, third attempt, commit 336c610)

**Design sources read this session, in full:** all ten domains from `design_gate.py needs` on the 11 files: `client_endpoint_distribution`, `constitution_governance`, `crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `provider_deployment_supply_chain`, `relay_reachability`, `time_ordering_events`, `ux_accessibility_personalization` (every listed document and ADR). The commit carries `Design-Read` for those ten and `Design-Departures: none`.

**Review against Core (1afadd8):** the mirrors match `RecoveryRequestView` (`requestId`, `activatesAt`, `state` incl. `stale`, `canStop`, `ready`), `ConfirmRecovery` (destructive, confirmation target `requestId`) and the two stub `NetCall`s (`recovery_request_fanout`, `recovery_veto_fanout`, answered `rejected(unsupported)` by the test host and HTTP provider). No code changes were needed.

**Verified:** vitest in om-ui, apps/web, om-core-harness, om-platform-api, om-provider-http (104 src tests pass; its `dist` stale suite fails as before), `pnpm exec tsc -b apps/web`, wasm `conformance.test.ts` and `ui.test.tsx`. No UI behaviour changed beyond one copy string, so no Chromium e2e was run.

## Slice 4 planning note (not built)

- **G2 Home endpoints:** `RecoveryRequestFanout` and `RecoveryVetoFanout` need Home API operations (the frozen home-api has fan-out for requests via `fanout_recovery_request`/`drain_recovery_requests` per IMPLEMENTATION_INTERFACES, but no veto endpoint; adding one is a design question, likely an ADR), plus RecoveryRecord put/get. Until then adapters answer `rejected(unsupported)`.
- **G3 recovering device:** an intent that takes the recovery key and record locator, a restricted recovery state before the device is enrolled, wasm ingest of the Home responses, UI entry, and an e2e.
- **Core follow-ups:** `verify_chain` replay of recovery links (so completion enters `acs_chain`, rotates the policy and notifies contacts), operator ruling on the N>=4 co-sign case, a test for `VetoRecovery` by a quarantined self, and a veto-before-request redelivery path.
- Reading needed before building: `identity_control_recovery`, `provider_deployment_supply_chain` (Home API), `relay_reachability`, `ux_accessibility_personalization`.

## Slice 4 build status (2026-11-14, build and test only, NOT committed)

**Design sources read (this session, in full):** RECOVERY_AND_BACKUP_POLICY (sections 1-10, ADR-0131/0154 summary inside it); the plan report; the relay-status deviations table (D1-D29); [`AGENTS.md`](../../AGENTS.md) Design gate. The Core code read: `control.rs` (verify_successor, verify_recovery_successor, verify_chain), `flows/recovery_watch.rs`, genesis in `flows.rs`, state and engine recovery views.
**Not read this session:** ACCOUNT_CONTINUITY_AND_CONTROL (relied on only through the plan's and RECOVERY's citations), ADR-0111, ADR-0127, ADR-0131, ADR-0147, ADR-0154, ADR-0168 themselves, and every other document of the six Core domains. The committing session must read them (command below) before claiming `Design-Read`.

**Rulings applied.** (a) N>=4 co-sign: RECOVERY 10.3 ("one co-sign completes immediately") and 10.4 ("one vote toward floor(N/2)+1") cannot both hold for N=4 (recovery + one co-signer = 2 < 3), so the conservative behaviour is kept and pinned by a test: `ConfirmRecovery` is rejected (`conflict`), the request stays waiting and completes by the delay path (RECOVERY 10.3/10.7) with no veto. Operator may overrule; the text itself needs the fix. (b) No veto endpoint: `RecoveryVetoFanout` stays a stub NetCall answered `rejected(unsupported)`; no Home change.

**Built (Rust, `crates/om-core`, uncommitted).**
- `control.rs`: `RecoveryLinkEvidence` and `verify_chain_with(chain, creds, links)`. A link with evidence must verify as a COMPLETE recovery through `verify_recovery_successor` (delay run or co-signed, no veto); `verify_chain` is `verify_chain_with(.., &[])`, so a recovery link is still refused wherever there is no evidence (every peer chain).
- `state.rs`: `AppState.recovery_links` (evidence: policy, next policy, request, this verifier's credited seconds; additive) and `recovery_join` (restricted recovery state; additive). `AppState::link_evidence()`.
- `flows/recovery_watch.rs::finish_recovery`: a completed recovery now appends the child to `acs_chain`, adds the new device's credential, stores the evidence, rotates `recovery_policy` to the next epoch's, sets the device's generation, re-verifies the whole chain (rolls back on failure) and calls the existing `broadcast_account_update`. Own-chain readers (`acs_head`, `devices_view`, `recovery_status`) use `verify_chain_with`.
- Recovering-device intent `BeginRecovery { recovery_key, home }` (api.rs, flows.rs `parse_recovery_key`, recovery_watch.rs `begin_recovery`): only on a profile with no device, account, ceremony or earlier attempt; checks the key's own checksum (inverse of the display groups; case, spaces and dashes ignored) and that the Home is an http(s) URL without whitespace (max 256); keeps the root (`SecretHex`) and Home in `recovery_join`. `RecoveryStatusView.recovering` (`{home, restricted}`, omitted when absent) shows the restricted state. No new derivation label; the label `OM-RECOVERY-AUTH-KEY-SEED-v1` is used only later, when the request is signed.

**Tests (all in `crates/om-core/tests/recovery_observe.rs`, 8 new):** completion by co-sign enters the chain, rotates the policy, replays after restart, and is unverifiable without evidence; completion by delay enters the chain (elapsed 72 h recorded); tampered/missing evidence fails; N=4 ruling (a); `VetoRecovery` and `ConfirmRecovery` by a self quarantined by the other controllers are `unauthorized`; veto before its request is dropped with no state and counts when redelivered (decision recorded: the Home redelivers, the veto fan-out is idempotent); legacy profile (no stored policy, or a policy the head does not commit to) reports `pending`; `BeginRecovery` (bad checksum, short key, non-URL refused; good key gives the restricted state; second attempt conflicts; survives restart). Verified: `cargo test -p om-core` (all 31 binaries pass), `cargo test -p om-wasm`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt`, `python3 tools/om_docs.py fmt` (0 rewritten), `make check`.

**Decisions the design is silent on (record, confirm):** (1) chain-link evidence is local: a contact receiving our chain has no evidence and refuses the recovery link, so contacts keep their pinned head. How a verifier that never saw the request credits the delay (or trusts a co-sign) is not specified; needs an operator ruling or an ADR before contacts can accept a recovery link, and so before "contacts see a continuity notice" is real. (2) The device-link ceremony transfers chains with no evidence, so a chain holding a recovery link cannot be handed to a new device yet. (3) A completed recovery leaves this (now revoked) device holding the new head; it simply cannot act (`unauthorized`). No self-revoked UI or wipe flow was built. (4) `BeginRecovery` is not in `DESTRUCTIVE_KINDS` (it creates no authority); the Home address check is only well-formedness, the adapter still enforces https and redirects.

**Remains.** Home side of G2/G3 (RecoveryRecord put/get and request/veto fan-out; needs the veto ADR); deriving the record read/write caps and locator from the root (the registry labels exist, the KDF construction with `recovery_scope_id` is not coded and is a new derivation: stop and report if built); the recovering device making its `RecoveryRequest` and successor (needs account id, epoch and parent hash from the record); a `Device` for the recovering profile; wasm/TS mirrors NOT done (`BeginRecovery` intent and `RecoveryStatusView.recovering` in om-core-wasm types, om-platform-api intents, harness, om-ui entry screen, e2e).

**For the committing session:** `python3 tools/design_gate.py needs crates/om-core/src/api.rs crates/om-core/src/control.rs crates/om-core/src/engine.rs crates/om-core/src/flows.rs crates/om-core/src/flows/recovery_watch.rs crates/om-core/src/state.rs crates/om-core/tests/recovery_observe.rs` (six Core domains: crypto_1to1, discovery_first_contact, identity_control_recovery, metadata_privacy, relay_reachability, time_ordering_events), then `git add crates` and commit with `Design-Read` for them and `Design-Departures: none` (decisions 1-4 above are gaps, not departures from audited text).

### Slice 4 design verification and commit (2026-11-14)

**Design sources read (this session, in full, with the Read tool):** every document `design_gate.py needs` listed for the six Core domains (crypto_1to1, discovery_first_contact, identity_control_recovery, metadata_privacy, relay_reachability, time_ordering_events) and every ADR it listed, plus ACCOUNT_CONTINUITY_AND_CONTROL and ADR-0111, 0127, 0131, 0147, 0154, 0168. Not read: SEALED_SENDER_DELIVERY_PROFILE (listed by the folder glob but not by `needs`) and ADRs the listed ADRs cite but `needs` does not list.

**Checks against the text.**
- *Chain replay of recovery links:* consistent. ADR-0147 5.7 and ID-03 make the delay verifier-local and non-transferable, so replaying with this verifier's own recorded interval (`RecoveryLinkEvidence.elapsed_seconds`) and no evidence for peers is what the design requires. Stale target, parent-state binding, sole-controller shape, policy rotation, veto and freeze rules in `verify_recovery_successor` match RECOVERY 10.3, ACCOUNT_CONTINUITY 13.2 and 5.
- *N>=4 single co-sign:* not a contradiction needing an operator ruling. ACCOUNT_CONTINUITY 3 says a co-sign is necessary for immediate effect, not sufficient; 4.1 and ADR-0127 rule 1 make recovery one vote toward `floor(N/2)+1`, and 10.4 lists the takeover cost as `floor(N/2)` devices plus the custodian (N=4: two). RECOVERY 10.3's "co-signed by >= 1 controller completes immediately" is shorthand valid for N<=3. The code (one co-signer + one vote = 2 < 3 at N=4 is not immediate; the request waits for the delay path with no veto) is what the audited text means. A wording fix in 10.3 would be nice but is not needed to commit.
- *BeginRecovery and the label:* no new derivation. `parse_recovery_key` only inverts the existing display encoding and its existing check; `OM-RECOVERY-AUTH-KEY-SEED-v1` stays used only in `recovery.rs` for signing the request later. The 10.2.1 read/write caps and locator (separate KDF with `recovery_scope_id`) are not built, correctly. Gap: 10.2.1 says the recovering device also needs the store's `recovery_scope_id`; `BeginRecovery` takes only a Home address, so the scope id must come with the Home operation slice.
- *Veto stub:* consistent with 10.3 (fan-out to every Home route is required but the Home API has no endpoint; blocked on the Home API, A-045). Not a departure.

**Decision 1 answered from the design.** A verifier that never observed the request cannot credit the delay, by design: ADR-0131 6 ("a verifier coming online late starts its own observation interval"), ADR-0147 5.7 (timer credit is never transferred between verifiers) and ID-03 (a verifier waits its own full interval before accepting a recovery-alone successor). So a contact must itself first observe the `RecoveryRequest` digest, start its own monotonic interval (web verifiers on Recommended may add the ADR-0154 gap credit plus a final authenticated veto sweep) and see no veto; the co-signed path needs no interval. Peers therefore do not trust a chain's claim of elapsed time, and our evidence staying local is correct.

**Open operator question (design silent).** How does a contact obtain the RecoveryPolicy body and the RecoveryRequest (the chain carries only the policy hash and the successor)? RECOVERY 10.3 fans the request out to the account's Home routes and live controllers, and Homes are "availability helpers" (ADR-0131 6), but no text names contacts as recipients or says how the policy body reaches them, so "contacts see a continuity notice" (RECOVERY 1) is not yet implementable. Until ruled, contacts keep their pinned head. Related: decisions 2 to 4 above stay open as recorded.

**Verification.** `cargo test -p om-core` (all pass), `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo test -p om-wasm` pass, `python3 tools/om_docs.py fmt` (0 rewritten). Committed with `Design-Read` for the six domains and `Design-Departures: none`.

## Slice 4 TS mirrors build status (2026-11-14, build and test only, code NOT committed)

**Changed (uncommitted):** `packages/om-platform-api/src/core/intents.ts` (`BeginRecovery { recoveryKey, home }` in `CoreIntent` and `INTENT_KINDS`; not destructive, matching Core) and `packages/om-platform-api/src/core/devices.ts` (`RecoveryStatusView.recovering?: RecoveringView { home, restricted }`). Names follow the serde camelCase in `crates/om-core/src/api.rs` (`recovery_key` -> `recoveryKey`, `recovering` omitted when absent, hence optional).
**No change needed:** om-core-wasm (types and test-support carry no intent or status types; NetCalls unchanged), om-core-harness (it handles intents by `kind` with no exhaustive check, and `BeginRecovery` has no simulated path; none was invented), apps/web and om-ui copy (no type change forced them). No UI screen or copy added: the recovering-device entry screen is a later slice needing the ux domain reading and an operator-visible decision.
**Verified:** `pnpm exec tsc -b apps/web` clean; `pnpm exec vitest run src` in om-platform-api (9), om-core-harness (29), om-ui (261), apps/web (138) pass; om-core-wasm `make core-wasm` then `conformance.test.ts` and `ui.test.tsx` (5) pass; its `dist` stale suite fails as before. `python3 tools/om_docs.py fmt`, `make check` pass (exit 0).
**Read:** the plan sections named in the brief, `api.rs` (`BeginRecovery`, `RecoveryStatusView`, `RecoveringView`), commit 336c610 diff. **Not read:** any design domain document; the committing session must read them.
**For the committing session:** `python3 tools/design_gate.py needs packages/om-platform-api/src/core/devices.ts packages/om-platform-api/src/core/intents.ts` (six domains: crypto_1to1, discovery_first_contact, identity_control_recovery, metadata_privacy, relay_reachability, time_ordering_events), then `git add packages` and commit with `Design-Read` for them and `Design-Departures: none`.
