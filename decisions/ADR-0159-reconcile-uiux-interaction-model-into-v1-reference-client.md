# ADR-0159 — Reconcile the UI/UX interaction model into the V1 reference client

- **Status:** Accepted
- **Date:** 2026-09-29
- **Scope:** reference-client UX integration; no wire-format, cryptographic or authority expansion
- **Builds on:** ADR-0037, ADR-0051–0058, ADR-0060, ADR-0068, ADR-0087–0089, ADR-0111, ADR-0115, ADR-0128–0130, ADR-0148–0158
- **Does not amend:** OM-Crypto-v1, AccountControlState authority, canonical event validity, Room/Space/Publication authority, Home tier semantics, recovery timers/custody, notification/push correctness, or V-018/V-022 pass criteria

## Context

A standalone UI/UX design track matured after the architecture's original UX seed documents. It produced a chat-first shell, bottom Omnibar, semantic action registry, cross-platform component/state contracts, accessibility mappings, local-first ranking, semantic motion/color, OwnMessenger customization, messaging/call/security state designs, icon/media assets and browser/static validation harnesses.

R14 audited that track against the 2026-09-29 06:34 architecture snapshot. The audit found the interaction model architecturally compatible but identified stale seed UI, V1 scope drift and several architecture-required surfaces missing from the standalone UI design. A blind directory copy would therefore preserve contradictions.

This ADR integrates the interaction model while making the canonical architecture win every scope/authority disagreement.

## Decisions

### 1. The reference shell is chat-first and user-composable

On compact layouts, Chats is the only top-level destination exposed by default. If it is the only enabled destination, no redundant one-item tab strip is rendered.

Calls, People, Spaces, Publications, Files and future capability views may be exposed, hidden and reordered by the user. Hiding a destination changes presentation only; its capability remains reachable through Omnibar, contextual actions, deep links and accessibility/command surfaces.

This supersedes the old fixed `Chats / Calls / People` seed screen map without removing those V1 capabilities.

### 2. Omnibar is the universal local-first find-or-do surface

Omnibar may search/resolve people, conversations, Rooms, Spaces, Topics, Publications, messages/local indexes, settings, invites and allowed semantic actions.

Ranking rules:

- explicit query semantics win;
- pins/explicit preferences beat inference;
- optional local learned state may rank targets;
- a bare entity query never silently acquires a learned side-effecting verb;
- learned state cannot lower action risk/confirmation/authentication requirements;
- security-critical actions never become predicted one-step execution.

Learned adaptation state is profile-scoped private local state, encrypted at rest under local-data policy, resettable and not uploaded merely because personalization is enabled.

### 3. One semantic Action Registry maps to typed Core intents

Touch, pointer, keyboard, voice, assistive-technology and future-HID bindings converge on semantic UI actions. The Action Registry is not authority and never becomes a generic string-RPC protocol.

Actions that mutate canonical/security state map to typed Core intents; platform-owned capabilities map to PlatformAdapter operations; presentation-only actions remain local UI state. Core remains authoritative for validation and result state. The mapping is documented in [`ACTION_TO_CORE_INTENT_MAP.md`](../docs/architecture/ux/ACTION_TO_CORE_INTENT_MAP.md).

### 4. The rich V1 architecture remains rich; default presentation remains simple

The reference UI must support the existing V1 architecture rather than demote capabilities for visual simplicity. This includes:

- device-local scheduled send with due-time late binding;
- same-profile account-private draft sync among eligible devices with conflict preservation (not a cross-implementation V1 promise);
- Spaces, nested Spaces and independently linked Rooms;
- lightweight threads and persistent Topics;
- public signed Publication distinct from private Rooms;
- polls;
- stickers and bounded looping expressive media;
- rich-text composition/sanitized paste;
- disappearing/view-once content;
- ContactCapsule/MessageRequest provenance;
- privacy-policy-controlled typing/presence/read receipts;
- privacy-aware link previews;
- block/report/selective EvidencePackage flows.

These are progressively disclosed and need not occupy default navigation.

### 5. Onboarding remains invite-first but preserves Home/provider truth

A zero-contact first run emphasizes invite/share/scan and never requires address-book upload. The UI still renders canonical T0–T4 Home semantics and the sponsor/co-resident/migration disclosures required by architecture.

Sponsored client acquisition must offer the required supported native and independent PWA-origin choices rather than letting the sponsor silently become client authority.

### 6. Recovery, Protection and device-control UX follow canonical security defaults

Recovery is on by default under ADR-0111. The UI names the actual custodian/status and separately presents identity recovery, history restore and provider/discovery reconnect.

Recommended / Hardened / Maximum Privacy / Custom remain security/privacy Protection profiles. Focus / Balanced / Playful remain experience profiles; one never silently changes the other.

Device management distinguishes immediate Suspend/quarantine, pending/final stronger control transition, contested/Safety Freeze and permanent revocation/removal.

### 7. Calls stay conversation-anchored and route-neutral

Calls can be launched contextually, through Omnibar or an optional Calls destination. Route changes (LAN/direct/TURN/SFU) do not change E2EE semantics or become a green/red security score.

Call signaling remains outside durable ConversationEvents. The UI may render a call timeline item/notice derived from call history/view state.

### 8. OwnMessenger and OhMessenger are presentation layers with hard safety boundaries

Themes/media packs are declarative inert content. They cannot execute code, load arbitrary remote tracking assets, remap actions, alter permissions/authority, hide required security boundaries or weaken accessibility/confirmation rules.

OwnMessenger may change safe presentation/preferences. OhMessenger may add recipient-controlled expressive media/motion/sound/haptic rendering. Protected security surfaces remain theme-proof where architecture requires it.

### 9. Detailed UI artifacts move under `docs/ux-design/`

The standalone `docs/design/ui-ux` tree is not added as a parallel architecture root. Current detailed specs/contracts/tokens/assets/prototypes/validation material are integrated beneath `docs/ux-design/`; normative cross-domain rules live under `docs/architecture/ux/`.

The old fixed-tab [`SCREEN_MAP_AND_JOURNEYS.md`](../docs/ux-design/SCREEN_MAP_AND_JOURNEYS.md) and seed component catalog are replaced by reconciled versions.

### 10. Evidence gates remain open

This integration does not self-close:

- **V-018** security UX comprehension;
- **V-022** performance budgets;
- physical-device, AT, icon-comprehension or brand human validation.

Browser/static fixtures remain candidate evidence only. Engineering authorization is unchanged.

## Consequences

1. No Core/protocol redesign is needed for the reconciled shell/Omnibar/design system.
2. Reference-client engineering gets a single coherent UX architecture rather than conflicting old/new screen maps.
3. V1 capability scope remains architecture-complete while the default UI stays simple through progressive disclosure.
4. Omnibar/personalization remain private local assistance, not hidden behavioral authority.
5. The UI/Core boundary becomes more explicit, not weaker.
6. Human/security/performance evidence still gates the reference-client scope.
