# Multi-device phase 5: history to a newly linked device (2026-11-13)

Scope: `core_kernel` (`crates/om-core`, `crates/om-wasm`, `packages/om-core-wasm`) plus the two copy lines and the e2e in
`apps/web`. Deviation D27 in [`2026-10-28-relay-status.md`](2026-10-28-relay-status.md). Builds on
[`2026-11-04-multi-device-core-status.md`](2026-11-04-multi-device-core-status.md) (phases 2 to 4) and the phase 5 row of
[`2026-11-02-multi-device-plan.md`](2026-11-02-multi-device-plan.md).

## Design sources read

Read in full: [`AGENTS.md`](../../AGENTS.md) (Design gate), [`MULTI_DEVICE_SYNC.md`](../architecture/crypto/MULTI_DEVICE_SYNC.md) (all of it), the plan and status reports above and the
handoff section 4, ADR-0157 sections 1 to 3 (namespacing and the `DeviceHistorySnapshot`/`GroupHistoryBundle` split),
ACCOUNT_CONTINUITY_AND_CONTROL sections 12 and 13 and the quarantine and link rows, [`spec/cddl/account.cddl`](../../spec/cddl/account.cddl) and the registries for
`DeviceEnrollmentNotice`, the history-archive code and ADR-0184 body shape (as code), the ceremony code in full.

### Design sources read, final statement (2026-11-13, third session)

The earlier paragraph that listed documents as not read is superseded. The operator ruled "read over sessions", and the six
`crates/om-core` domains were then read in full over two sessions; this session read its parts in its own context.

- **crypto_1to1: read in full** (second session, in its own context): CANONICAL_ENCODING, PRODUCTION_CRYPTO_INTEGRATION_GATE, MULTI_DEVICE_SYNC,
  CRYPTOGRAPHY_AND_AGILITY, ASYNC_KEY_MATERIAL, ONE_TO_ONE_SESSION_MANAGEMENT; ADR-0011, 0063, 0065, 0067, 0072, 0073, 0107, 0118, 0119, 0120, 0126, 0140.
- **identity_control_recovery: read in full** (IDENTITY_PROTOCOL, IDENTIFIER_SPOOFING_AND_DISPLAY, IDENTITY_CONTINUITY by the second session; ACCOUNT_CONTINUITY_AND_CONTROL,
  RECOVERY_AND_BACKUP_POLICY, IDENTITY_DISCOVERY_VERIFICATION, ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY and ADR-0002, 0003, 0010, 0026 to 0030, 0066, 0105, 0106, 0111, 0127, 0159 by this one).
- **metadata_privacy: read in full** by this session: PRIVACY_GOVERNANCE_AUTHORITY, MULTI_RELAY_AND_DELIVERY_RESILIENCE, NOTIFICATIONS_AND_PUSH, STORAGE_AND_BLOB_ARCHITECTURE,
  SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION, METADATA_THREAT_MODEL; ADR-0098, 0124, 0128, 0129, 0130.
- **time_ordering_events: read in full** by this session: EVENT_MODEL, TIME_AND_EXPIRY_SEMANTICS, MESSENGER_BASELINE_V1, SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION; ADR-0008, 0076, 0087, 0088, 0121, 0157.
- **discovery_first_contact and relay_reachability: read in full** by this session (DISCOVERY_FABRIC, INVITES_INTROGATE_AND_SPAM, CONTACT_SHARING, INTRODUCTION_AND_SERVICE_ACCOUNTS,
  KEY_TRANSPARENCY_AND_DIRECTORY, RELAY_AND_DELIVERY_ARCHITECTURE, REACHABILITY_AND_OBJECT_REPAIR; ADR-0004, 0019, 0020, 0021, 0025, 0033, 0040, 0012, 0022, 0075).
- **client_endpoint_distribution and ux_accessibility_personalization: read in full** by this session, for the `apps/web` files only (the nine client/ecosystem documents PWA_NATIVE_AND_CLIENT_DISTRIBUTION_STRATEGY, PWA_REFERENCE_CLIENT, CLIENT_AND_EXTENSION_ARCHITECTURE,
  DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION, MULTI_IDENTITY_AND_PROFILE_ISOLATION, CLIENTHOST_UPDATE_LIFECYCLE, NOTIFICATIONS_AND_PUSH, ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE,
  PLUGIN_AND_APP_EXTENSION_ARCHITECTURE; the twelve ux, overview and research documents PRODUCT_AND_UX_PRINCIPLES, UI_UX_SYSTEM, UI_CORE_PLATFORM_CONTRACT, ACCESSIBILITY_AND_INPUT, MOTION_AND_PERFORMANCE,
  THEMES_FONTS_AND_PERSONALIZATION, SECURITY_PRIVACY_PROFILES, RECONCILED_INTERACTION_MODEL, ACTION_TO_CORE_INTENT_MAP, NAVIGATION_AND_OMNIBAR, VISUAL_LANGUAGE_COLOR_ICONS_AND_ASSETS,
  CURRENT_UI_REFERENCES_20260929; and ADR-0007, 0037, 0038, 0044, 0045, 0051 to 0060, 0068, 0077, 0103, 0115, 0129, 0130, 0159, 0162). The copy lines match them: plain words, no crypto vocabulary,
  migration to a new device as a device add with history transfer (DEVICE_VAULT section 6, PWA strategy "PWA-to-native migration"), and `ApproveDeviceLink` stays a Core-confirmed destructive intent (UI_CORE_PLATFORM_CONTRACT UI-11).

**Check of the staged work against what was read: no departure found beyond D24 and D27.**
- ADR-0157 section 2 and MULTI_DEVICE_SYNC: `DeviceHistorySnapshot` is implementation-private, carries a 32-byte `codec_id` plus `codec_major` inside the sealed frame, unknown pairs ignored whole. Matches.
- Backups restore history, never authority (RECOVERY section 8, STORAGE): the snapshot carries text and delivery facts only; no key, route, session, ACS or recovery state. Matches.
- Erased content is never resurrected (TIME section 4, ADR-0129 rule 11): `backup::collect` leaves deleted messages out. Core has no per-message disappearing timer yet, so no `expiry` exists to carry; when disappearing messages land, the snapshot must exclude expired content and carry `expiry`.
- Revoked and quarantined devices get nothing (ACCOUNT_CONTINUITY sections 4 and 12, MULTI_DEVICE_SYNC "Revocation"): `history_eligible`. Matches.
- Metadata (METADATA_THREAT_MODEL sections 5 and 7): chunks are sealed, padded to one bucket and carried as ordinary queue messages; the Home learns sizes and times only. Matches, and it is declared above.
- Authorship caveat: merged messages are the sending device's own record (the account-private channel authenticates them as the owner's device), not re-authenticated events; they carry their digest as evidence. This is the same-owner case, not the ADR-0128 rule 4 group-bundle case.

## Answers to the open questions of the phase 2 to 4 report

1. **Transcript MAC (`OM-DEVICE-LINK-CEREMONY-MAC-v1`): still a gap.** The registry label is `reserved` with no construction and no
   document gives one. Nothing was invented: the provisional derivation in `flows/ceremony.rs` is unchanged. Needs the design owner.
2. **Notice signer role: answered, no gap.** [`om-object-types.json`](../../spec/registries/om-object-types.json) (type 8) allows `controller` or `recovery`; a link is approved by a
   control device, so `controller`. [`account.cddl`](../../spec/cddl/account.cddl) numbers device class 1 messaging, 2 control. The code already did this (D24 note 2 closed).
3. **Does a quarantined device block an approval: no.** The notice goes to "the account's other *active* devices" (section 12, R-166) and
   the link authorization is "1 active, non-quarantined controller" (the approver). A quarantined device is neither. The code matches.

## What is built

After the D24 ceremony the approving device sends a `DeviceHistorySnapshot` to the new device. The confirmed `ApproveDeviceLink` is the
explicit account authorization (no second prompt; D27). Only a device the new head lists as active gets one (`history_eligible`: control
or messaging, not quarantined, not revoked); the receiver re-checks its own head per chunk.

- **Carriage:** extra sealed `M_HISTORY` messages in the ceremony response queue, same key as the approval and roster, announced by a
  count in the approval (field 9). The Home sees only sizes and times. No wire or Home change.
- **Namespacing (ADR-0156/0157):** each chunk carries a fixed 32-byte random `codec_id` and `codec_major` 1 inside the sealed frame. A chunk with
  an unknown pair, a malformed body, or arriving at a device no longer active is counted and ignored whole; the link still completes.
- **Body:** the ADR-0184 history archive: text and delivery facts. No key, route, session, attachment reference or draft. Newest first, at most 32 chunks
  of about 50 KB, so a long history is truncated at the oldest end.
- **Merge:** by EventID (message id) union. A message already present is never replaced; the chat's messages are re-sorted by time and clock. A chat the
  new device does not hold live becomes read-only restored history. Messages that never left the writing device, or were refused, are not copied.
- **Core API:** `Query::GetHistoryTransfer` (`HistoryTransferView`: role, chunk counts, ignored, messages added; counts only). `om-wasm` already
  forwards every `Query`, so there is no new wasm `Command`; the TS types, harness and a wasm conformance check are added (`packages/om-platform-api`,
  `om-core-harness`, `om-core-wasm`). The wasm artifact was rebuilt (`make core-wasm`, guard OK).
- **Web:** `apps/web/src/copy/en.ts` link `warning` and `joinCopy.state.approved` now say older chats are copied, newest first, photos and files are not.

## Evidence

- `crates/om-core/tests/device_history.rs`: round trip on a second profile (same messages, same order, later traffic not duplicated); a 60-message,
  multi-chunk history, newest present; no history text in any relay deposit; an empty history announces nothing.
- `tests/device_history_unknown_codec.rs`: unknown codec ignored whole, link completes, nothing interpreted.
- `flows/ceremony.rs` unit test `only_an_active_device_may_receive_history` (revoked, quarantined, unknown, malformed head).
- `apps/web/e2e/device-link.spec.ts` new test: Alice and Bob talk on Alice's device, a second Chromium context links, and shows both older messages in order
  (real Core wasm, local reference Home). Chromium only; Firefox and WebKit were not run.
- Full `cargo test -p om-core`, `pnpm typecheck`, `pnpm exec vitest run` (976 passed), `python3 tools/validate_architecture.py`, clippy clean.
  `pnpm lint` also needed a one-line fix of a non-null assertion left by 0b6a2d9 (`conversations.test.tsx`).

## Not done, not verified

- **Revoked device in a live scenario:** covered by the predicate's unit test and the approval-time check, not by a two-Core run where the new device is revoked
  mid-transfer (the receiver learns its own revocation only through contacts' updates, which `settle` does not order against the chunks).
- **No resume or re-send:** the ceremony lasts five minutes; a lost chunk is lost (the view shows done vs total). A later snapshot needs a new link.
- **Not on the Cloudflare Home:** about 1.6 MB of queue messages per link is unverified against its quota. Not in the Devices screen yet (no UI reads `GetHistoryTransfer`).
- Drafts and the ADR-0148 preferences, attachments and group history are not transferred. The transcript MAC (above) and the non-Anthropic review of
  multi-device Core (ADR-0167) remain open.

## Committed

Core, packages, docs and machine files: one commit with `Design-Read` for the six Core domains and `Design-Departures: D24, D27`. The `apps/web` files (link copy, e2e) and the one-line `om-ui` lint fix follow in a second commit, `Design-Read: client_endpoint_distribution, discovery_first_contact, relay_reachability, ux_accessibility_personalization`, because the gate adds the client and UX domains for them.
Re-run results (2026-11-13): `cargo test -p om-core` (170 passed), wasm conformance (4), `packages/om-ui` `vitest run src` (250), `apps/web` `vitest run src` (138), `tsc -b apps/web` clean, Chromium `device-link.spec.ts` (3 passed).
