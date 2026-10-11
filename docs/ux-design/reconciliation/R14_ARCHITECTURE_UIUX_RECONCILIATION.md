# R14 — OM Architecture ↔ UI/UX Reconciliation Audit

**Status:** audit complete; **no architecture merge performed**  
**Architecture base:** `om-design-20260929-0634-PHT-reviewed-fixed.zip`  
**Architecture SHA-256:** `78ddae29c9ad65972ed6cb346df1cfe16c6316ec30ce3ddb94be4e3d020bbe99`  
**Architecture snapshot:** 2026-09-29 06:34 PHT (ADR-0157–0158, design_open=0, engineering_authorized=false)  
**UI/UX base:** `om-ui-ux-design-20260928-r13c.zip`  
**UI/UX SHA-256:** `3d225b564ba873093f74ceffdd0ca5265af74549c67d9adc21032959a4612278`

## Executive verdict

R13C is **architecturally compatible in direction** with OM. Its strongest ideas — chat-first shell, Omnibar, one interaction grammar/many platform dialects, semantic motion/color, OwnMessenger presentation safety, local-first search/adaptation, accessible multimodal input, object-preserving messaging states and calm security UX — fit the architecture's existing `ux_accessibility_personalization` domain and `om-ui → Core → PlatformAdapter` split.

The audit found **no need to redesign OM Core or the protocol merely to accommodate R13C**. The main work before merge is reconciliation, not invention.

However, the two tracks are **not ready for a blind directory copy**. There are three important classes of work:

1. **R13C scope drift:** several capabilities that the architecture already classifies as V1 were treated as optional/deferred or were not designed in detail (Spaces, lightweight threads/topics, Publication, polls, scheduled send, some expressive-media and messaging-policy surfaces).
2. **Architecture-required security/onboarding UX missing from R13C:** T0/T1 Home onboarding disclosures, default recovery custody, Protection presets, suspend/quarantine/Safety Freeze, service/bridge disclosures and related V-018 study flows.
3. **Stale architecture UX seed documents:** the architecture's older `docs/ux-design` screen map still hard-codes three persistent tabs and older onboarding. Those seed docs should be superseded by the reconciled R13C shell, not allowed to survive as competing normative UI.

### Audit counts

- total reconciliation items: **40**
- major reconciliation items: **9**
- moderate reconciliation items: **11**
- external blocking evidence gates: **2**
- informational/aligned/partial items: **18**

Human testing remains deferred by user choice; therefore **V-018 remains open** and this audit does not claim reference-client engineering authorization.

## Source-lineage choice

Two very recent Library packages existed. The 06:07 package was uploaded later, but its internal snapshot is an earlier semantic-follow-up branch. The selected 06:34 `reviewed-fixed` package contains ADR-0157 **and** ADR-0158, 341 requirements, 180 decisions / 158 ADRs and 873 current files. R14 therefore treats it as the authoritative reconciliation base.

## Must reconcile before merge

### 1. V1 product-scope mismatches

The architecture's [`MESSENGER_BASELINE_V1.md`](../../architecture/events/MESSENGER_BASELINE_V1.md) already makes the following part of the first-release design boundary:

- device-local scheduled send;
- Communities/Spaces;
- lightweight threads and persistent topics;
- public signed Publication;
- polls;
- first-class stickers and bounded looping animation;
- cross-device account-private draft synchronization when linked eligible devices are available.

R13C must stop describing these as absent/v2-only where that contradicts the architecture. **Progressive disclosure is still allowed.** A feature can be architecturally V1 while hidden from the default shell until a user/conversation actually needs it.

### 2. Home/onboarding model

The R13C invite-first direction is good and compatible, but it currently starts too late in the flow. Architecture onboarding must handle:

- T0 sponsored starter mailbox;
- T1 hosted Home;
- T2 own cloud;
- T3 self-hosted;
- T4 local only;
- sponsor/co-resident metadata disclosure;
- migration away from sponsor dependency;
- sponsored-invite client-source choice (native plus independent PWA origins).

The correct synthesis is **invite-first, infrastructure-light onboarding**, not infrastructure-free semantics.

### 3. Recovery and protection defaults

R13C's recovery UI is conceptually correct about separating identity/history/provider recovery, but architecture is more specific:

- recovery is on by default;
- synced passkey custody is preferred when available;
- recovery key is the fallback/secondary custodian;
- recovery can be pending if no custodian can be established;
- history recovery remains separate;
- Recommended / Hardened / Maximum Privacy are operational privacy-policy bundles.

`Focus / Balanced / Playful` must remain an **experience profile**, not a security/privacy profile.

### 4. Device-control semantics

R13C currently emphasizes `Revoke device`. Architecture distinguishes:

- immediate `SuspendDevice` / quarantine;
- pending/final authorization for stronger permanent control changes;
- contested/frozen account-control states;
- permanent revocation/removal.

The reconciled UX must expose these as different semantic actions.

### 5. Spaces / Rooms / topics

R13C's simplicity goal is preserved, but its normative one-level `Space → Channel` limit cannot constrain the architecture. The architecture allows:

- independent Rooms linked to Spaces;
- Rooms linked to more than one Space;
- nested Spaces;
- lightweight threads;
- persistent TopicID organization;
- Publication as a separate public object class.

The UI should **hide complexity until present**, not deny valid architecture state.

### 6. Calls timeline wording

R13B's “call event” language should become **call timeline item/notice**. Architecture deliberately keeps session signaling outside durable ConversationEvents. The UI may materialize missed/ended call history without claiming a canonical chat event exists.

### 7. Typed intent boundary

R13C's Action Registry is a strong UI abstraction, but architecture is authoritative that:

```text
UI semantic action
    → typed Core intent
    → Core validates canonical/security state
    → view-state/event stream
    → UI render
```

The merge needs an explicit mapping table. No Action Registry entry may become a generic string-RPC escape hatch or let `om-ui` mutate protocol/account authority directly.

## Missing UX domains to add before final UI scope freeze

These are already architecture-designed but incomplete/missing in R13C:

1. polls;
2. lightweight threads + Topics;
3. Publication/public broadcast;
4. disappearing/view-once/retention UX;
5. ContactCapsule forwarding + MessageRequest provenance flow;
6. privacy-controlled typing/presence/read-receipt presentation;
7. rich-text composer formatting/sanitized paste UX;
8. link-preview privacy/failure UX;
9. block/report/selective EvidencePackage flow;
10. Protection presets;
11. sponsored/hosted Home onboarding disclosures and migration;
12. Suspend/quarantine/Safety Freeze device-control flows.

## Architecture-neutral R13C improvements that can merge without protocol changes

Subject to normal UX ADR/requirement updates, these are compatible presentation/client improvements:

- Chats-only default root with user-composable tabs;
- bottom Omnibar and composer↔Omnibar semantic morph;
- Omnibar local indexing/ranking/voice/keyboard grammar;
- explicit local-learning opt-in/reset and “learn targets, not hidden verbs” rule;
- semantic color system and OM Calm theme candidate;
- Focus/Balanced/Playful expression profiles;
- semantic motion + recipient-controlled delight rendering;
- expanded icon/media-asset system;
- component state-machine/accessibility mappings;
- responsive Compact/Split/Expanded/Multi-pane presentation;
- content/microcopy voice;
- haptic/sound semantic vocabulary;
- stricter component-level performance targets, as long as they do not silently rewrite V-022's normative budgets.

## Existing architecture UX domain is the merge seam

Do **not** create a parallel architecture root called `docs/design/ui-ux` inside the architecture repo and leave the old UX model intact. Recommended destination:

### `docs/architecture/ux/`
Keep/merge normative cross-domain contracts here:

- UI/Core/Platform typed-intent boundary;
- shell/information architecture;
- accessibility/input invariants;
- security/privacy profiles;
- performance gates;
- theme/security boundaries.

### `docs/ux-design/`
Place detailed design-system/product artifacts here:

- component/state specs;
- Omnibar behavior;
- icon/media specifications and assets;
- design tokens/preset themes;
- prototypes;
- local browser/static validation harnesses;
- identity exploration/validation preparation.

### machine ledgers
Add traceable requirements/decisions for newly normative UX semantics. Reuse the existing `ux_accessibility_personalization` / `reference_client_ui` scope and V-018/V-022 gates rather than inventing a parallel engineering authority.

## Evidence/gate status

### V-018 — security UX comprehension

Still open. R13B/R13C prototypes can become study material, but the architecture requires a real participant study including T0/T1 onboarding, recovery, device suspension, contact continuity, bridge/service boundaries and sponsor disclosures. Human testing was explicitly deferred, so **R14 cannot close this gate**.

### V-022 — performance budgets

Still open. R9/R10/R13C browser/Omnibar evidence is useful input, but architecture owns the normative end-to-end budgets and requires a reviewed measurement plan plus real-device release evidence. R13C's tighter local interaction targets should be treated as component targets until promoted through the architecture process.

## Recommended merge sequence (R15)

1. **Create one architecture UX-integration ADR** that accepts the R13C interaction model while explicitly preserving Core authority, V1 architecture scope and V-018/V-022.
2. **Patch R13C contradictions first**: draft sync, scheduled-send scope, Spaces/topics/Publication/polls, recovery defaults, Protection profiles, device suspension and Home onboarding.
3. **Replace stale architecture UX seed docs** ([`SCREEN_MAP_AND_JOURNEYS.md`](../SCREEN_MAP_AND_JOURNEYS.md) and overlapping component seed material) with reconciled content or mark them superseded.
4. **Map Action Registry → typed Core intents/view state** and add conformance cases for dangerous actions, profile capture and capability-unavailable behavior.
5. **Add missing architecture-designed UX domains** listed above.
6. **Integrate detailed assets/tokens/prototypes** into `docs/ux-design/`, keeping architecture-level constraints under `docs/architecture/ux/`.
7. **Update machine requirements/trace/architecture map** and rerun architecture validation/continuity checks.
8. **Do not close V-018/V-022** without their required evidence.
9. Produce a merged architecture ZIP only after all consistency checks pass.

## Detailed reconciliation matrix

The machine-readable source is [`om-r14-reconciliation-v0.json`](om-r14-reconciliation-v0.json). Major items are those requiring a pre-merge correction; `blocking_external` means the design can be merged but engineering authorization remains blocked on external evidence.

| ID | Status | Severity | Scope | Resolution summary |
|---|---|---|---|---|
| `shell-defaults` | `architecture_requires_update` | `major` | `reference_client_ui` | Adopt R13C shell via architecture ADR; tabs are presentation only, capabilities remain accessible via Omnibar/context. Supersede old fixed-tab wording. |
| `omnibar` | `compatible_extension` | `info` | `reference_client_ui` | Merge as detailed UI implementation semantics; keep Core intent boundary and local-index privacy rules. |
| `account-profile-terminology` | `uiux_correction` | `major` | `reference_client_ui` | Normalize user-facing wording as appropriate but map state/actions explicitly to IdentityProfile where architecture means profile. Never invent cross-profile heuristics. |
| `draft-sync` | `uiux_correction` | `major` | `reference_client_ui` | Revise draft spec: local durability required; private cross-device sync is architecture-supported V1 behavior when capability is available, with conflict-safe UX and no conversation authority. |
| `scheduled-send` | `scope_correction` | `major` | `reference_client_ui` | Promote scheduled-send UX to architecture-required V1 product behavior, while preserving truthful local-device execution and actual-send late binding. |
| `spaces` | `scope_correction` | `major` | `communities_publications` | Keep default UI simple/progressive, but do not constrain protocol model to one level. UX must render/navigate architecture-valid nested Spaces safely, even if hidden until needed. |
| `threads-topics` | `missing_or_deferred_ux` | `moderate` | `communities_publications` | Add bounded thread/topic UX before merge. Preserve simple reply as default; reveal topics only where a Room uses them. |
| `public-publication` | `missing_ux` | `moderate` | `communities_publications` | Add Publication feed/follow/source/provenance UX with clear distinction from private Rooms and no implied E2EE. |
| `polls` | `missing_ux` | `moderate` | `reference_client_ui` | Add create/vote/close/result semantics and unsupported-capability fallback before scope freeze. |
| `stickers-looping-media` | `partial_coverage` | `info` | `reference_client_ui` | Map content-pack/sticker selection, send, fallback and static/reduced-motion behavior into messaging UX. |
| `typing-presence-receipts` | `partial_coverage` | `info` | `reference_client_ui` | Add explicit presentation/privacy rules and map them to Recommended/Hardened/Maximum Privacy profiles. |
| `disappearing-messages` | `missing_ux` | `moderate` | `reference_client_ui` | Add conversation timer, per-message expiry/view-once presentation, backup/search implications, and honest non-erasure wording. |
| `contact-capsule-message-requests` | `partial_coverage` | `info` | `discovery_first_contact` | Add Share Contact / Shared by <person> / Message Request flows mapped to architecture. |
| `home-onboarding-tiers` | `missing_critical_ux` | `major` | `provider_reference` | Integrate invite-first empty state with Home tier onboarding. Sponsored invite must disclose observer/closure facts and offer native or multiple independent PWA origins per V-018. |
| `recovery-default` | `uiux_correction` | `major` | `reference_client_ui` | Revise recovery UX to architecture defaults and exact scope. Preserve user-friendly wording and explicit opt-out consequences. |
| `protection-profiles` | `missing_ux` | `moderate` | `reference_client_ui` | Add Protection presets as a separate settings dimension; never conflate emotional/attention profiles with security/privacy policy. |
| `device-suspend-vs-revoke` | `missing_critical_ux` | `major` | `reference_client_ui` | Add Suspend device, pending/final control-state, contested/frozen state and permanent revocation as distinct actions. |
| `verification-language` | `aligned` | `info` | `reference_client_ui` | Merge R13B language into canonical UX docs; retain architecture-specific continuity/safety-number source data. |
| `call-routing` | `aligned` | `info` | `calls` | Merge detailed R13B states into call UI; preserve architecture CallSessionID/key/route semantics. |
| `call-timeline-item` | `uiux_wording_correction` | `info` | `calls` | Rename to call timeline item/notice; do not imply a canonical durable ConversationEvent unless architecture later adds one. |
| `group-call-ui` | `aligned` | `info` | `calls` | Merge, capability-gated by PlatformAdapter/Core. |
| `notifications` | `aligned_with_architecture_detail` | `info` | `reference_client_ui` | Keep R13A attention model as presentation layer; architecture remains source for push/wake capability and privacy. |
| `local-search` | `aligned` | `info` | `reference_client_ui` | Merge Omnibar indexing/ranking under profile-scoped LocalDataClass rules. |
| `local-adaptation-data` | `architecture_mapping_needed` | `moderate` | `reference_client_ui` | Map learning aggregates to profile-scoped private local state, encrypted at rest, resettable/export policy explicit; add architecture requirement/trace if normative. |
| `themes-assets` | `aligned` | `info` | `reference_client_ui` | Merge R11-R13 asset/icon/token details beneath canonical theme boundary. |
| `fonts` | `architecture_stricter` | `info` | `reference_client_ui` | Architecture rule wins; ensure asset/theme validator rejects unsafe fonts and security surfaces never use theme fonts. |
| `motion` | `aligned` | `info` | `reference_client_ui` | Merge R13C richer semantic motion/delight taxonomy without weakening architecture performance/reduced-motion gates. |
| `performance` | `budget_reconciliation` | `moderate` | `reference_client_ui` | Treat R13C numbers as component targets unless promoted by architecture ADR. Do not overwrite V-022 budgets silently. |
| `accessibility-input` | `aligned` | `info` | `reference_client_ui` | Merge R9/R10 component mappings and conformance harness as candidate evidence, not closure of human/AT gates. |
| `security-ux-human-gate` | `open_external_gate` | `blocking_external` | `reference_client_ui` | R14 cannot close V-018. Preserve it as an explicit pre-engineering blocker for reference_client_ui. |
| `performance-gate` | `open_external_gate` | `blocking_external` | `reference_client_ui` | Use R13C harnesses as inputs; do not claim V-022 passed. |
| `invite-client-origin-choice` | `missing_critical_ux` | `major` | `provider_reference` | Expand invite-install UX to origin/provider choice without overwhelming user; V-018 comprehension criteria remain authoritative. |
| `address-book-default` | `aligned` | `info` | `reference_client_ui` | Merge as default; optional export/matching must preserve provenance and consent. |
| `rich-text-composer` | `missing_ux_detail` | `info` | `reference_client_ui` | Add formatting UX, keyboard shortcuts, paste sanitization feedback and accessibility. |
| `privacy-link-preview` | `missing_ux` | `moderate` | `reference_client_ui` | Add per-profile/conversation link-preview controls and failure-neutral rendering. |
| `block-report-evidence` | `missing_ux` | `moderate` | `reference_client_ui` | Add block/report/selective evidence UX without implying a universal OM moderator. |
| `identity-name` | `aligned` | `info` | `constitution_governance` | Keep OM as public name pending future clearance; identity visual testing remains separate. |
| `architecture-ux-seed` | `architecture_stale_doc` | `moderate` | `reference_client_ui` | At merge, replace/supersede seed screen map and component catalog references rather than preserving contradictory UX docs. |
| `action-registry-core-intents` | `mapping_required` | `moderate` | `reference_client_ui` | Create explicit action→typed Core intent/view-state mapping; UI actions never become protocol authority or generic string RPC. |
| `human-validation-brand` | `deferred_evidence` | `info` | `reference_client_ui` | Keep deferred; not a merge blocker for semantic UX, but blocks final brand/accessibility freeze. |
