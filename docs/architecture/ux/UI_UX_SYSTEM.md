# OM UI/UX System

- **Status:** canonical reference-client UX architecture
- **Reconciled:** R15 / ADR-0159
- **Human evidence:** V-018 remains open

## Product goal

OM should feel like a normal, modern messenger while preserving the project's operatorless/privacy architecture. The user should normally think about **people, conversations and what they want to do**, not relays, cryptographic epochs, provider topology or device-key mechanics.

> **Bulletproof underneath; boringly easy on top.**

The detailed interaction doctrine is in [`RECONCILED_INTERACTION_MODEL.md`](RECONCILED_INTERACTION_MODEL.md).

## Shared client family

Official PWA, iOS, Android and desktop clients use one conceptual `om-ui` above portable Core with capability-oriented PlatformAdapters.

The shared invariant is not pixel identity. It is:

- same object/action vocabulary;
- same trust/security meaning;
- same destructive-operation consequences;
- same information hierarchy;
- same semantic motion/color roles;
- same accessibility/action equivalents;
- platform-native expression where the OS owns the surface.

Mobile iOS, Android and PWA should be perceptually and behaviorally near-identical except where platform-owned interaction requires a difference.

## Default compact shell

The default compact root is **Chats**.

If Chats is the only exposed top-level destination, no redundant one-item tab bar is shown. Users may expose/reorder Calls, People, Spaces, Publications, Files or later destinations through OwnMessenger. Hidden destinations remain available through Omnibar, contextual actions, deep links and accessibility/command surfaces.

The account/IdentityProfile control defaults to a lower trailing reach zone on compact touch layouts; its placement may be mirrored/configured without inferring handedness through behavioral surveillance.

### Conversation surface

Inside a conversation:

- content dominates; chrome is restrained;
- composer owns the compact lower text-entry zone;
- the Omnibar trigger transforms the same zone into find/do mode rather than stacking a second persistent field;
- draft/reply/attachment state survives the transition;
- tabs normally disappear on compact conversation view to maximize content space;
- expanded layouts may keep navigation and global Omnibar visible in parallel.

## Omnibar

Omnibar is the universal local-first **find-or-do** surface. It may resolve people, conversations, messages, Rooms/Spaces/Topics/Publications, settings, invites, calls and safe Action Registry entries.

Default ranking is transparent and deterministic enough to remain understandable. Optional local learning:

- is profile-scoped private local state;
- is encrypted at rest under local-data rules;
- can be disabled/reset;
- is not uploaded merely because personalization is enabled;
- learns likely **targets**, not hidden verb substitution;
- cannot promote security-critical actions into bypassed confirmation.

`Cmd/Ctrl+K` is the desktop/power-user entry point. Keyboard, voice and assistive technologies resolve into the same semantic actions.

## Navigation doctrine

- **Changing place** uses navigation.
- **Acting within a place** prefers contextual surfaces/sheets/popovers where appropriate.
- Back returns to the previous interaction context and preserves source location/focus/draft where feasible.
- Larger windows reveal more simultaneous context rather than adding a different hierarchy.

Responsive capability states are Compact, Split, Expanded and Multi-pane. Window size/capability, not device marketing category, determines the state.

## Messaging doctrine

Messages/media remain recognizable through queued/sending/transferring/retrying states. Failures attach to the affected object.

Scheduled send is V1 and is explicitly `scheduled`, not `sent`; Core releases compose intent at due time after revalidating current state.

Drafts are locally durable. Architecture-supported same-profile account-private draft sync may be used among eligible linked devices while preserving conflicts; it is convenience state, not conversation authority and not guaranteed cross-implementation V1 interoperability.

Rich text, polls, replies, reactions, stickers/bounded looping media, disappearing/view-once content and link previews are contextual capabilities, not top-level navigation.

## Groups, Spaces, Topics and Publication

Ordinary groups remain ordinary conversations.

Spaces organize independent Rooms and may contain/nest Spaces according to canonical group architecture. The UI progressively reveals hierarchy only when present. It must not impose a one-level protocol limit for visual simplicity.

Lightweight threads and persistent Topics remain Room-local organization and appear only when a Room uses them.

Public Publication is a distinct public signed object class. Its UI must clearly distinguish it from private Rooms and must not imply E2EE, hidden subscriber membership or guaranteed erasure of copied public posts.

## Calls

Calls remain conversation/person anchored. Calls/People tabs are optional presentation shortcuts, not capability boundaries. Routing is automatic and route type never changes the security indicator when E2EE semantics are unchanged.

Call signaling remains outside durable ConversationEvents; local history may render a call timeline item/notice.

## First run and invitations

No uploaded address book by default. A zero-contact first run is invite-first:

- Share invite/link;
- Show my QR;
- Scan QR;
- accept an invitation from an existing communication channel.

The simple surface must still expose the canonical Home tier facts when needed, especially T0/T1 sponsor/host observation, migration and independent-client-origin choices.

## Trust/security surfaces

Routine encryption is baseline, not a permanent padlock badge.

Security UI appears when a user decision or material trust boundary needs attention. It distinguishes automatic cryptographic checks, user verification, device authorization, service/bridge plaintext boundaries, recovery custody and real-world identity.

Device management distinguishes Suspend/quarantine, pending authorization, contested/Safety Freeze and permanent revocation.

Recovery is on by default under canonical policy; UI separately presents identity recovery, history restore and provider/discovery reconnection.

Protection profiles Recommended / Hardened / Maximum Privacy / Custom are separate from Focus / Balanced / Playful experience profiles.

## OwnMessenger and OhMessenger

OwnMessenger changes safe presentation/preferences: themes, light/dark pairs, font/density within validated bounds, tab exposure/order, wallpaper, permitted shortcuts, experience profile, haptic/sound preference.

OhMessenger is the optional emotional/delight layer: expressive media, reactions, celebration and ambient effects. The recipient owns rendering intensity.

Themes/assets are declarative data, never executable code or arbitrary remote tracking resources. Protected security surfaces use reserved built-in semantics/fonts/colors where architecture requires them.

## Accessibility

Gestures are accelerators, never exclusive actions. Keyboard/focus operation, named accessibility actions, large text/reflow, Reduced Motion, high contrast and equivalent non-color state cues are release requirements.

The Action Registry is semantic; bindings are platform/input adapters. See [`ACTION_TO_CORE_INTENT_MAP.md`](ACTION_TO_CORE_INTENT_MAP.md).

## Performance

Fast local acknowledgment is a UX invariant. R13C component targets may be used as engineering targets, but V-022 remains authoritative for normative end-to-end budgets and closure evidence. Decoration degrades before typing, focus, semantic feedback or action responsiveness.

## Evidence boundary

Current prototypes/static/browser checks are useful candidate evidence only. They do not close V-018, V-022, physical-device, AT or human usability gates.
