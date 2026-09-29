# OM Reconciled Interaction Model

- **Status:** canonical UX architecture integration (ADR-0159)
- **Scope:** reference-client information architecture and user-visible semantics
- **Authority boundary:** presentation only; protocol/account/security authority remains in Core and existing canonical architecture

## 1. Prime interaction rule

> **One OM interaction language, many platform dialects.**

OM presents the same objects, actions, security meanings and recovery consequences across PWA, iOS, Android and desktop. Touch, pointer, keyboard, voice, assistive technology and future HIDs are replaceable input bindings to semantic actions; they are not separate product semantics. Every side-effecting semantic action resolves to a typed Core intent before canonical or security state can change.

The reference family should feel recognizably the same across iOS, Android and mobile PWA. OS-owned surfaces — permissions, biometric prompts, native call UI, file/photo pickers, system share sheets, notification controls and window chrome — remain native.

## 2. Default shell: chat-first, not feature-first

The compact reference shell defaults to **Chats** as the only exposed top-level destination. If only one destination is enabled, no redundant one-item tab bar is rendered.

Users may expose, hide and reorder additional presentation destinations such as Calls, People, Spaces, Publications or Files through OwnMessenger settings. Hiding a tab never hides the underlying capability: the Omnibar, contextual actions, deep links and accessible command surfaces remain available.

On larger windows, the same model unfolds into Split / Expanded / Multi-pane layouts. More space reveals more simultaneous context; it does not create a different information architecture.

## 3. Omnibar

The Omnibar is OM's universal **find-or-do** surface. It resolves, locally where possible:

- people and IdentityProfile-local PersonRecords;
- conversations, Rooms, Spaces, Topics and Publications;
- messages and local content indexes;
- settings and preference actions;
- safe Action Registry commands;
- invite/QR actions;
- calls and common contextual actions.

On compact chat-list surfaces it lives in the lower reach zone by default. Inside a conversation the composer owns that zone; invoking Omnibar semantically transforms the lower surface rather than stacking two competing text-entry fields. Draft, reply target, staged attachments and focus context survive the round trip.

### Ranking rules

1. explicit query meaning wins;
2. explicit pins/preferences outrank learned inference;
3. current context, recency and frequency may rank candidates;
4. optional learned preference state may rank **targets**, not silently substitute verbs;
5. security-critical actions are never promoted into one-step execution merely because they are predicted;
6. no result may bypass Core preconditions/authorization/confirmation.

Local adaptation data is profile-scoped private local state, encrypted at rest under the local-data architecture, resettable, and not uploaded to analytics/model/provider services by default.

## 4. One text-entry purpose at a time

On compact surfaces, the user should not wonder whether text will be sent to a person or interpreted as a command.

- **Composer state:** text/media is communication content.
- **Omnibar state:** text/voice is find-or-do intent.

The transition between them preserves draft state and uses semantic motion where enabled. Reduced Motion preserves the state relationship without requiring spatial animation.

## 5. Direct manipulation and safe discovery

The interacted object should usually be the thing that moves or changes: message, attachment, conversation row, call control, Space/Room item, or setting. Gestures accelerate; they do not monopolize functionality. Every essential gesture has a visible/menu/keyboard/accessibility equivalent.

Exploration remains reversible until intent becomes consequential. Irreversible/security-critical operations deliberately add friction and explicit consequence wording.

## 6. Semantic feedback

Every perceptible response should primarily do one or more of the following:

- explain;
- reassure;
- orient;
- express;
- delight.

Functional state is never communicated by color, motion, sound or haptic alone. Semantic motion describes causality, destination, hierarchy or state change. OhMessenger expressive motion/sound/haptics are optional emotional rendering and may be disabled independently.

The recipient owns their rendering: a sender may express celebration/emphasis, but the receiver's Focus/Balanced/Playful, Reduced Motion, sound and haptic preferences determine how it is presented.

## 7. OwnMessenger versus security policy

**OwnMessenger presentation** may customize themes, typography within safe bounds, density, tab exposure/order, experience profile, sounds/haptics, wallpapers and permitted shortcut bindings.

**Protection policy** is separate and uses the canonical Recommended / Hardened / Maximum Privacy / Custom profiles. Focus/Balanced/Playful never implies stronger or weaker security.

Themes alter presentation, not behavior/authority. They cannot remap actions, hide required security state, weaken confirmations, reduce target/focus accessibility below release requirements, or recolor protected security semantics into misleading meanings.

## 8. Architecture-rich, presentation-simple V1

The V1 architecture includes more capability than the default shell exposes. The UI must be capable of rendering and operating architecture-valid state while preserving progressive disclosure:

- ordinary 1:1 and group conversations remain simple;
- Spaces may contain/nest Spaces and link independent Rooms; the default UI reveals hierarchy only when present;
- lightweight threads and persistent Topics appear only where a Room uses them;
- public signed Publication is visually/security-distinct from private Rooms and never inherits an E2EE implication;
- polls, stickers/bounded looping media and scheduled send are first-release capabilities without becoming permanent navigation;
- rich text, disappearing/view-once content, link previews, message requests, ContactCapsules and reporting/evidence remain contextual actions/surfaces.

## 9. Messaging object preservation

The user's object remains recognizable through transport state. A message, file, photo, voice note or scheduled compose intent does not disappear into a generic spinner/job.

Failures attach to the affected object. Retry means retry the same logical object where the Core model permits it; the UI must not invent duplicate user-visible messages for transport attempts.

Drafts are locally durable user work. The reference family may also use architecture-defined account-private E2EE draft sync among eligible linked devices; conflicts are preserved rather than silently resolved by a single timestamp. Such sync is private convenience state, not conversation authority and not a cross-implementation V1 promise.

Scheduled send is V1 and stores only profile-bound compose intent. It becomes an ordinary send only at due-time revalidation; the UI truthfully represents `scheduled`, `sent late`, discarded-on-lost-eligibility and other Core outcomes.

## 10. Trust and security language

The UI keeps separate:

- route/provider state;
- cryptographic/session continuity;
- automated cryptographic checks;
- **Verified by you** user action;
- device authorization;
- recovery custody;
- real-world human identity.

A route is not a security score. A successful automatic cryptographic check is not proof of a person's real-world identity.

Device actions distinguish immediate **Suspend/quarantine**, pending/final authorization, contested/Safety Freeze state, and permanent revocation/removal. The UI never promises deletion of data already stored on a revoked device unless the architecture can guarantee it.

Recovery is on by default per canonical architecture, normally through synced passkey custody where available or recovery key fallback. Identity recovery, history restoration and provider/discovery reconnection remain visibly separate.

## 11. Onboarding and invite-first discovery

OM remains no-address-book-upload by default. A new user with no contacts should see a useful invite-first empty state: share link/QR, show QR, scan QR, or receive a bounded invitation.

That simple surface must still truthfully handle the canonical Home tiers:

- T0 sponsored starter mailbox;
- T1 hosted Home;
- T2 own cloud;
- T3 self-hosted;
- T4 local-only.

Sponsored/hosted flows disclose sponsor/co-resident observation and migration facts when they materially affect the choice. Sponsored-invite client acquisition offers the architecture-required native and independent PWA-origin choices without turning onboarding into infrastructure configuration.

## 12. Calls

Calls stay anchored to people/conversations/Rooms and can also be started through Omnibar or an optional Calls destination. LAN/P2P/TURN/SFU routing is normally invisible and never changes the displayed E2EE semantics.

Call signaling is not a durable ConversationEvent. Conversation history may render a **call timeline item/notice** derived from call history/view state without claiming that signaling itself entered the canonical message-event log.

## 13. Accessibility and internationalization

Semantic action identity is input-independent. Keyboard, screen-reader/custom accessibility actions and visible controls are first-class, not fallback modes. Motion/color/sound/haptics add information bandwidth but never form the sole essential channel.

All strings are externalized; layout uses logical direction and survives RTL, long translations, mixed-script names/content, large text and current-window resizing. User-generated names/messages are treated as opaque content with bidi isolation where needed.

## 14. Evidence status

This integration does **not** close:

- V-018 security UX comprehension — human/multilingual study still required;
- V-022 performance budgets — reviewed measurement plan and real-device evidence still required;
- final logo/wordmark/icon-comprehension/physical ergonomics human gates.

Browser/static UI prototypes are candidate evidence only.
