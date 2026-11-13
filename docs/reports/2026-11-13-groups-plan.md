# Ordinary groups (work group F, scope `groups`, ADR-0177): design reading, gaps and slice plan (2026-11-13)

Status: **no code written.** This session read the groups design, found that the first slice the brief suggested (a group text message over
the existing pairwise sealed-sender fan-out) is **forbidden by audited text**, and stopped at the design gate instead of improvising. What is
here is what the next session needs so it does not re-derive it: what the design specifies, the gaps and the operator decisions they need,
and a slice list whose first slice is allowed by the design.

## Design sources read

Read in full, in this session's own context:

- [`AGENTS.md`](../../AGENTS.md) (lonunjie root and `om`), the handoff [`2026-11-04-handoff-next-week.md`](2026-11-04-handoff-next-week.md) (group F, gotchas),
  the completion plan [`2026-11-04-v1-completion-plan.md`](2026-11-04-v1-completion-plan.md) (section 3 Groups row), and the recovery plan [`2026-11-13-recovery-flows-plan.md`](2026-11-13-recovery-flows-plan.md) (header only).
- **`groups` domain (the four documents):** [`GROUP_CHAT_ARCHITECTURE.md`](../architecture/groups/GROUP_CHAT_ARCHITECTURE.md), [`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md),
  [`SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`](../architecture/groups/SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md). The fourth, [`SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md`](../architecture/groups/SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md), was **not** read (Spaces are W3, out of scope for ordinary groups).
- **Wire:** [`spec/cddl/group.cddl`](../../spec/cddl/group.cddl) (all of it), ADR-0177, ADR-0176 (wire freeze and the cross-field rules), ADR-0173 (same-parent ordering), ADR-0174 (materialization rulings).
- **`time_ordering_events` documents:** EVENT_MODEL, TIME_AND_EXPIRY_SEMANTICS, CANONICAL_ENCODING, MESSENGER_BASELINE_V1, SCOPED_PERMISSIONS (4 of 4).
- **`crypto_1to1` documents:** CRYPTOGRAPHY_AND_AGILITY, PRODUCTION_CRYPTO_INTEGRATION_GATE, ASYNC_KEY_MATERIAL, ONE_TO_ONE_SESSION_MANAGEMENT, MULTI_DEVICE_SYNC, CANONICAL_ENCODING (6 of 6).
- **`identity_control_recovery` documents:** IDENTITY_PROTOCOL, IDENTITY_CONTINUITY, ACCOUNT_CONTINUITY_AND_CONTROL (3 of 7).

**Not read (so a Core commit claiming these domains would not be honest):**

- `identity_control_recovery`: RECOVERY_AND_BACKUP_POLICY, IDENTITY_DISCOVERY_VERIFICATION, IDENTIFIER_SPOOFING_AND_DISPLAY, ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.
- `discovery_first_contact`: all five documents (DISCOVERY_FABRIC, INVITES_INTROGATE_AND_SPAM, CONTACT_SHARING, INTRODUCTION_AND_SERVICE_ACCOUNTS, KEY_TRANSPARENCY_AND_DIRECTORY).
- `relay_reachability`: all three documents. `metadata_privacy`: all six (PRIVACY_GOVERNANCE_AUTHORITY, MULTI_RELAY, NOTIFICATIONS_AND_PUSH, STORAGE_AND_BLOB, SPACES_ROOMS, METADATA_THREAT_MODEL).
- **Every ADR** the six `crates/` domains list except 0173, 0174, 0176, 0177 (so ADR-0121, 0122, 0126, 0128, 0157, 0163 and the metadata ADRs 0129 to 0140 were seen only through the documents' summaries). ADR-0128 is the
  group rule set (rules 1 to 20) and is the first thing the next session reads.
- The groups schemas [`group-control-transition.logical.schema.json`](../../spec/protocol/group-control-transition.logical.schema.json), [`capability-grant.logical.schema.json`](../../spec/protocol/capability-grant.logical.schema.json), [`om-room-capabilities.json`](../../spec/registries/om-room-capabilities.json) (needed for the capability checks) and the V-004/V-009 vectors.

`python3 tools/design_gate.py needs crates/om-core/src` maps **six** domains (crypto_1to1, discovery_first_contact, identity_control_recovery, metadata_privacy, relay_reachability, time_ordering_events) and does not list
the `groups` domain at all; the architecture map has no `groups` entry in `code_paths`. A Core commit therefore needs those six read, and a groups commit should also carry the groups documents above. About 525 KB of reading in total (about 135k tokens); this session
read roughly 40 percent of it, which is why no code was committed (rule: a domain is claimed only by an agent that read it).

## What the design specifies for ordinary groups

1. **Cryptography is MLS (RFC 9420), not an OM invention.** One MLS client per authorized device (DeviceIdentity); the human roster is AccountIDs. v1 suite is the standard X25519/AES-128-GCM/SHA-256/Ed25519 suite. OpenMLS 0.9.0 is the selected runtime
   (ADR-0177; proven by the V-004 probe on native and wasm32). `group_id` is random and unrelated to the `ConversationID`, which travels only inside an encrypted GroupContext extension.
   v1 groups are classically confidential; no surface may claim post-quantum confidentiality for groups (R-173, ADR-0107).
2. **Control state.** `GroupControlState` (object 28) is a hash-chained, device-signed state: profile (name, avatar, description), membership (principal, immutable membership incarnation, role ids, endpoint policy), role definitions, governance
   (principals, threshold, protected operations), policy (join rule, new-device policy, history policy, disappearing timer, security profile, `sequencing_timeout` 30..3600 s, `past_epoch_window` 0..8), coordinator succession (ordered devices), invites, `mls_epoch_ref`.
   Membership is capped at 512 and a signed body at 64 KiB (F-27); a larger room needs an additive profile (open design point, ADR-0177 item 3).
3. **Changes** go through `GroupControlIntent` (object 29, 16 operations: member.admit/remove/leave, device.add/remove_revoked, profile.set, disappearing_timer.set, role.define/assign, invite_link.issue/revoke/redeem, admission.set, security_policy.set, governance.set,
   coordinator.replace) bound to an MLS Commit by `GroupCommitBinding` (object 30). Cross-field rules are in ADR-0176 section 3 and are part of validity (12 state rules, 9 intent rules, 3 binding rules).
4. **Ordering.** A member CommitCoordinator (succession order in state) sequences same-epoch changes but holds no authority. After `sequencing_timeout` any authorized member may commit a **reduction** directly (remove, leave, remove revoked device, remove service). Among valid Commits
   sharing a parent the canonical one is the minimum of (non-reducing, direct, committer DeviceID, binding hash); a merged losing branch is marked `SUPERSEDED`, sends nothing, re-queues its events and recovers through a roster-neutral recovery Commit (ADR-0173).
5. **Journal (ADR-0163).** `INTENT_DURABLE -> CANDIDATE_PREPARED -> PUBLISHED_UNCONFIRMED -> ACCEPTED | SUPERSEDED | ABANDONED`; exact prepared Commit bytes are persisted before publication; relay ack is not acceptance; no application message is sent under an unmerged branch; if prepared state is lost, fail closed, never reconstruct Commit bytes.
6. **Create.** New group: random `ConversationID` and `group_id`, genesis `GroupControlState` v1, MLS group with the creator's own devices, invitation to each chosen person over the **existing 1:1 relationship**, add after acceptance, Welcome, then encrypted events through normal delivery.
7. **Join (consent first).** Invite over the 1:1, the invitee **accepts and releases KeyPackages for that Room through a relationship-scoped capability**, only then is the MLS Add committed. A client drops a Welcome that no accepted invitation of its own precedes. Invite links are capabilities (`join_ref`, never the ConversationID);
   default `admin_approval`; redemption is itself an intent so `max_uses` is counted in state.
8. **Leave and remove.** Leave and remove are reductions: remove all of an account's device leaves, advance the epoch, stop delivery to those capabilities. A revoked or quarantined device (learned at a validated ACS head) must not send into the Room until a Commit removing its leaf is accepted; any member may commit that removal. A group never adds a device
   of an account in CONTROL_CONFLICT or SAFETY_FREEZE.
9. **Roles.** Capabilities are authority and roles are named bundles (registry [`om-room-capabilities.json`](../../spec/registries/om-room-capabilities.json): `room.post`, `room.invite`, `room.remove_member`, `room.manage_roles`, `room.manage_governance`, ...). Consumer default: 1-of-1 governance held by the creator, familiar Admin/Moderator/Member labels are
   presentation only. Delegation ceiling (ADR-0128 rule 10): creating, editing or assigning a role must lie inside the actor's ceiling. Reductions are monotone and target an exact incarnation (ABA protection).
10. **Messages.** One MLS application ciphertext, then **one independently encrypted per-recipient envelope per destination device** (fresh randomized outer wrapper, ADR-0128 rule 7) through that device's opaque delivery capability, across any relay. Events are `ConversationEvent`s (the same kinds as 1:1, plus the W1 pin/poll/topic bodies), authorized at their **sending epoch**,
    ordered by `(sending epoch, EventClock)`. Group read receipts default off. MLS gives transferable authentication (no deniability, ADR-0138); the UI must say so.
11. **History.** Default `since_join`. A newcomer gets earlier history only through a separate `GroupHistoryBundle` (object 31): signed by the author device, 1..1024 text/attachment items, delivered as an encrypted blob over the recipient's 1:1 channel, admitted through the ordered gates of ADR-0176 section 3 (GH-01 to GH-15), rendered "shared by <author>",
    written to an archive namespace only, never to the live dedupe set or a reducer. Old MLS epoch secrets are never exported.
12. **UI.** "New group, select people, optional name/photo, Create"; people who have not accepted show as invited; the roster shows people (never epochs, KeyPackages, device leaves); changes are human sentences ("Alice added a new phone.", "Bob left."). Group voice/video, bots, bridges, Spaces and Publications are other scopes.
13. **Backup and devices.** No shared group archive; each person's backup keeps the history they hold; raw MLS epoch secrets are not backed up; a new device rejoins current state and gets history separately. The device-history snapshot (D27) does not carry groups.

## Gaps and decisions needed

| # | Gap | Status |
|---|---|---|
| **G1** | **The brief's slice 1 (group text over pairwise sealed-sender fan-out) is forbidden.** [`CRYPTOGRAPHY_AND_AGILITY.md`](../architecture/crypto/CRYPTOGRAPHY_AND_AGILITY.md) "per conversation type" rule 5: "OM does not build a parallel pairwise-fanout group mode to paper over this." GROUP_CHAT_ARCHITECTURE: "OM will not invent a new group key agreement protocol"; the application message is one MLS ciphertext wrapped per recipient device. A pairwise group would also
  carry no epoch, so the authorization epoch, ordering (ADR-0121/0173) and history-bundle gates have nothing to bind to. This is a change to audited text, not a recordable deviation. | **Operator decision.** Default taken by this plan: do not build it; slice 1 is the control model (below), MLS comes in slice 2. If the operator wants an interim non-MLS mode it needs an ADR first. |
| **G2** | **OpenMLS is not in the workspace.** It exists only in `verification/prototypes/v004-mls-ordering` and `v019` (pinned `=0.9.0`, `openmls_rust_crypto =0.6.0`, features `fork-resolution`, `js` for wasm). Adding it to `om-core` is the selected runtime (ADR-0177) but needs: the MPL-2.0/MIT licence check (ADR-0118 dependency policy), a wasm size and build check, and a
  storage adapter behind OM's own storage/profile boundary (CRYPTOGRAPHY_AND_AGILITY: a library format is never the canonical storage format). Core is sans-IO with its own RNG and test providers; the OpenMLS crypto provider must use Core's RNG, not `getrandom` directly. | Engineering task of slice 2; a dependency-policy decision, not new crypto. |
| **G3** | **No wire for MLS KeyPackage publication and fetch.** ASYNC_KEY_MATERIAL names `publishMlsKeyPackages` / `fetchMlsKeyPackage` and section 8 says KeyPackages are consumable objects with disjoint sets per store, but [`home-api.md`](../../spec/protocol/home-api.md) and [`home-api.cddl`](../../spec/cddl/home-api.cddl) have no endpoint for them (grep: no `KeyPackage`). The group architecture says
  KeyPackages are released **to an accepted invitee's relationship**, not fetched by the public. | Needs a design answer: either an additive Home endpoint under `provider_reference`, or KeyPackages carried inside the 1:1 channel as a sealed message (no Home change; fits "relationship-scoped capability"). **Operator/design-owner decision before slice 3.** |
| **G4** | **Core's 1:1 flow is built around `Relationship`**; a group conversation has no relationship, a different roster source (GroupControlState, not one peer's ACS) and per-device fan-out over **several** relationships. Eligibility must be derived per member from each member's relationship-pinned ACS head (K5/K6), so a group can only include people the local account already has an `ACTIVE` relationship with. | Design follows from the text; the invite step already requires a 1:1 relationship. No gap in the design, but a structural change in Core. |
| **G5** | `room.*` capability checks need the registry's kind-to-capability table (ADR-0121 rule 7) and the invite basis; the registry is read by `om-codec` at build time. The reducer must reuse it, not hard-code ids. | Read the registry in slice 1. |
| **G6** | Open design points ADR-0177 forbids deciding alone: the migration transition wire (ADR-0175 rule 5), a larger-envelope profile for rooms above about 500 members or histories above one bundle (F-27), `reducing` recomputation for state-dependent operations (T-161). | Out of v1 scope; do not build; stop if met. |
| **G7** | The v1 product cap on group size ("reference clients can begin with a conservative product cap") is not specified. | Silent. Provisional choice for the reference client: 32 members until performance is measured (V-022); record as a deviation when first enforced. |
| **G8** | "Not part of `om-open-beta-1`" (completion plan section 3): groups are a separate release after the beta. No UI is promised for the beta. | Informational. |

## Slice list (each one commit-sized, each needs its own design-read claim)

1. **Group control model, no messaging (allowed by the design, no MLS).** New `crates/om-core/src/groups.rs`: typed `GroupControlState` (object 28) and the intent argument types decoded/encoded through the registry-driven codec; the ADR-0176 section 3 cross-field validators (state, intent, binding); genesis creation
   (creator, 1-of-1 governance, default roles Admin/Member as role definitions, `mls_epoch_ref = 0`); a pure reducer for `member.admit`, `member.remove`, `member.leave`, `profile.set`, `role.define`/`role.assign` with the capability check from the registry, the delegation ceiling and incarnation ABA rules; state-hash chain; persistence in
   `AppState`; `tests/groups.rs` with vectors from V-004 (`v004-group-wire-*.json.gz`) and property tests (a sequence of valid intents never breaks the state rules). Nothing is sent. This is the smallest slice that moves toward the design without amending it.
2. **MLS runtime (G2).** `openmls =0.9.0` behind a `GroupCrypto` trait in Core, Core's RNG, an OM storage adapter, create group with the creator's own devices, KeyPackage generation, Commit and Welcome, `GroupCommitBinding` signing, the ADR-0163 journal states persisted. Loopback tests with 3 devices on 2 accounts; wasm build size recorded.
3. **Invite, accept, add (G3 decided first).** Invitation over the 1:1, accept, KeyPackage release, MLS Add by a member holding `room.invite`, Welcome over 1:1, drop of an unsolicited Welcome, declined or ignored invite changes nothing.
4. **Group text messages.** MLS application message, per-device sealed envelopes with fresh outer wrappers across relays, receive, `ConversationEvent` with the sending epoch, delivery states, offline and reordered delivery, `past_epoch_window` gaps. Typed views for the UI.
5. **Leave, remove, device add and revoke (reductions).** ACS-driven removals, the coordinator, `sequencing_timeout`, direct reductions, same-parent ordering, `SUPERSEDED` branches and recovery Commit (ADR-0173), journal restart tests.
6. **Roles, admission, invite links, disappearing timer, `GroupHistoryBundle`** (GH-01 to GH-15), then wasm query/intent surface, TypeScript mirrors and the UI (New group, roster, "invited" state, human-sentence system messages). Pre-release items of ADR-0177 item 2 (multi-client partition harness, V-009 corpus run, V-019) stay open and block any public group release.

## What remains (handoff)

- **Nothing is built.** `git status` was clean at start; this report and its file-registry line are the only change.
- **Operator decisions needed before building beyond slice 1:** G1 (confirm no interim pairwise mode), G3 (how KeyPackages travel), G7 (member cap).
- **Next session, exact steps:**
  1. Read, in this order, the unread parts of the six `crates/` domains: ADR-0128 (the group rules), ADR-0121, ADR-0122, ADR-0157, ADR-0163, ADR-0126; then `discovery_first_contact` (5), `relay_reachability` (3), `metadata_privacy` (6), the four unread identity documents,
     and ADR-0129/0130/0140 and the older ADRs. Batch it: about 90k tokens. Then read [`spec/registries/om-room-capabilities.json`](../../spec/registries/om-room-capabilities.json), [`spec/protocol/group-control-transition.logical.schema.json`](../../spec/protocol/group-control-transition.logical.schema.json) and the V-004 vectors.
  2. Then commit with `Design-Read: crypto_1to1, discovery_first_contact, identity_control_recovery, metadata_privacy, relay_reachability, time_ordering_events` and `Design-Departures: none` (slice 1 amends nothing).
  3. Build slice 1 as listed above. Existing Core to extend: `crates/om-core/src/state.rs` (`AppState` persistence), `control.rs` (ACS verification, reused for member eligibility), `crates/om-codec/src/schema.rs` (check that objects 28 to 31 decode; the large-body note at line 320 already names the group objects).
  4. Gotchas carried from earlier sessions: `pnpm exec vitest run src` (not bare `vitest run`) in `packages/om-ui`; long messages (8 KB) are refused by `SendMessage` in loopback tests, use about 1.5 KB; the design gate for `crates/` plus `apps/web` is about 14 domains, so batch Core and web work in one session.

## Coordinating-session rulings (2026-11-13, from the design and the constitution; operator may overrule)

- **G1:** confirmed. No interim non-MLS group mode: [`CRYPTOGRAPHY_AND_AGILITY.md`](../architecture/crypto/CRYPTOGRAPHY_AND_AGILITY.md) "per conversation type" rule 5 forbids a parallel pairwise-fanout group mode. Slice 1 is the typed `GroupControlState` model (validators, reducer, persistence, `tests/groups.rs`), with no messaging.
- **G7:** the provisional member cap of 32 stands as a policy constant (not wire), so it can change without a protocol change.
- **G3:** decide at slice 3, after reading ADR-0128 and the Home API documents. Constitution direction: no OM-run service, so KeyPackages travel on the person's own delivery service (an additive Home endpoint, or the 1:1 channel), never a directory. Needs an ADR only if it changes audited text.
