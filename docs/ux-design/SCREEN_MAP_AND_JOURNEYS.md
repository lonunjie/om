# OM Reference Screen Map and Journeys — Reconciled R15

This design reference implements the canonical architecture in [`docs/architecture/ux/RECONCILED_INTERACTION_MODEL.md`](../architecture/ux/RECONCILED_INTERACTION_MODEL.md). It supersedes the pre-R15 fixed-three-tab seed.

## Primary compact shell

```text
AppShell
├── Chats                                      # only default top-level destination
│   ├── Conversation list
│   ├── Message requests
│   ├── Recent Room/Topic context when relevant
│   ├── Conversation
│   └── Conversation details / media / topic/thread context
├── Omnibar                                    # lower find-or-do surface
├── IdentityProfile/account control            # lower trailing reach zone by default
└── Optional user-exposed destinations
    ├── Calls
    ├── People
    ├── Spaces
    ├── Publications
    ├── Files
    └── future capability views
```

If only Chats is exposed, the destination bar is omitted. Capability remains reachable through Omnibar/context even when its tab is hidden.
│   └── Conversation details / media
├── Secondary surfaces (Omnibar/context/optional user pin)
│   ├── Call history / active call
│   ├── Contacts / people / requests
│   └── Files / other local views
└── Profile → Settings
    ├── Profile / identity
    ├── Privacy & protection
    ├── Devices
    ├── Recovery
    ├── Notifications
    ├── Appearance / themes
    ├── OM Home
    └── Advanced / diagnostics
```


## Journey: use the Omnibar

1. User taps the Omnibar or presses `Cmd/Ctrl+K`.
2. Without typing, OM shows a small recognition-oriented set of recent/pinned conversations and common actions.
3. Typing filters local conversations/people/messages/settings first; external discovery is a visibly separate result class.
4. Repeated successful local selections may adjust ordering inside the current IdentityProfile using bounded recency/frequency state.
5. User can remove one learned suggestion, clear/disable learning, or pin an item.
6. Destructive/security actions never auto-run from autocomplete and keep their normal Core authorization/confirmation.

## Journey: first-run ordinary user

## Expanded shell

```text
Identity rail / navigation
      │
conversation / Space / Publication list
      │
active conversation / feed
      │
optional details / members / media / topic context
```

Responsive states reveal parallel context; they do not create a new object hierarchy.

## Conversation

```text
Conversation header
Timeline
  ├── messages
  ├── media/files/voice notes
  ├── polls
  ├── disappearing/view-once state
  ├── topic/thread affordance when present
  └── call timeline notice when materialized locally
Composer
  ├── rich text
  ├── staged attachments
  ├── reply target
  ├── schedule
  ├── voice note
  └── Omnibar trigger
```

The composer and Omnibar never compete for the same compact text-entry field at the same moment.

## Journey: zero-contact first run

1. OM creates/opens the local IdentityProfile and resolves the applicable Home tier workflow.
2. T0/T1 sponsored/hosted paths disclose the facts required by canonical onboarding without turning the flow into infrastructure setup.
3. Where a sponsored invitation acquires a client, the user is offered the architecture-required supported native/independent PWA-origin choices.
4. The ordinary ready state is **Invite someone to chat** with Share link, Show QR and Scan QR.
5. No address-book upload is required or presented as necessary.
6. Accepting/scanning uses the ordinary Invite → IntroGate → MessageRequest/relationship path; possession of a QR alone is not relationship authority.

## Journey: invite an existing acquaintance

1. Omnibar: `invite`, or empty-state Invite action.
2. Choose one-time/limited/reusable form when architecture/policy permits.
3. Share through existing messaging/email/system share, show QR or let the other person scan directly.
4. Invite card clearly says what accepting does and, where relevant, expiry/use limits.
5. Revocation/reset uses canonical invite semantics.

## Journey: Carol introduces Alice to Bob

1. Carol opens Alice → Share contact.
2. Core creates a ContactCapsule with provenance.
3. Carol chooses Bob/conversation/share sheet.
4. Bob opens card and sees **Shared by Carol**.
5. Bob chooses Send message request.
6. Alice receives a constrained MessageRequest under Requests.
7. Accept establishes the ordinary relationship path; Carol is no longer authority.

## Journey: unknown sender

1. Request appears under Requests, not as an accepted contact relationship.
2. Preview is bounded by canonical request policy.
3. Accept → normal relationship capability.
4. Block → reject/local/provider behavior according to architecture.
5. Report, if chosen, uses selective EvidencePackage disclosure; OM does not imply a mandatory central moderator.

## Journey: message Maria / call Maria with Omnibar

1. Invoke Omnibar.
2. Bare `Maria` resolves to the person/conversation entity; learned ranking may improve Maria's rank but does not change the verb.
3. `call Maria` resolves the explicit Call action.
4. Selection runs through Action Registry → typed Core intent.
5. Security-critical actions never auto-execute from ranking/voice prediction.

## Journey: scheduled message

1. Compose content normally.
2. Choose Schedule and date/time.
3. UI shows **Scheduled**, not Sent/Queued.
4. Before release, edit/cancel remains available.
5. At due time Core revalidates current eligibility/relationship/group state and creates an ordinary send.
6. UI reflects sent-late or lost-eligibility outcomes honestly.

## Journey: drafts across devices

1. Local device persists draft incrementally.
2. Eligible linked devices may receive architecture-defined account-private encrypted draft state.
3. Concurrent conflicting drafts are preserved/resolvable; UI never silently discards user work using a timestamp winner.
4. Draft sync is convenience state and never conversation authority.

## Journey: ordinary group

1. Open group from Chats.
2. The surface behaves like a normal conversation with participant/group details available contextually.
3. Space/topic hierarchy is absent unless the conversation actually participates in it.

## Journey: Space / Room / Topic

1. Omnibar or optional Spaces destination opens a Space.
2. Compact UI uses a contextual Space navigator/sheet; expanded UI may keep organization visible alongside the Room.
3. Nested Spaces are traversed cycle-safely.
4. Rooms remain independent objects and may appear in more than one Space.
5. Threads are lightweight reply-derived context; persistent Topics appear only when Room policy/use calls for them.
6. Topic or Space hierarchy never implies hidden transitive authority.

## Journey: public Publication

1. User opens a Publication descriptor/link or optional Publications destination.
2. UI states that the feed is public/signed and does not present it as a private E2EE Room.
3. Follow state is local/account-private/provider state as architecture defines; follower is not a Publication member.
4. Optional discussion opens the separately linked Room with its own security semantics.
5. Retraction is shown as publisher retraction; OM does not promise copied public content ceased to exist.

## Journey: create/vote in poll

1. Composer → Poll.
2. Create options/settings supported by the baseline capability.
3. Vote action operates on the canonical poll state.
4. Closed poll visibly stops new vote affordance; ordering/authorization is Core-owned, not local wall-clock logic.
5. Older/unsupported presentation must fall back safely according to baseline event semantics.

## Journey: disappearing or view-once content

1. User chooses conversation timer or per-message view-once capability where supported.
2. Composer/preview makes lifecycle consequence visible before send.
3. Search/export/backup presentation respects expiry policy.
4. UI uses honest language: expiry is cooperative client lifecycle behavior and cannot promise erasure from already-captured/copied endpoints.

## Journey: privacy-aware link preview

1. User pastes a link.
2. Current Protection/profile/conversation preview policy determines whether preview fetch is offered/performed.
3. Failure produces neutral link rendering, not message-send failure.
4. Maximum Privacy may disable preview fetching by default.
5. UI does not silently leak the user's network identity to arbitrary link targets contrary to architecture policy.

## Journey: stolen/suspicious device

1. Settings → Devices.
2. Open suspicious device.
3. Choose **Suspend this device** for immediate quarantine.
4. Core applies the canonical suspension semantics.
5. If stronger permanent removal requires authorization, UI shows pending/final state separately.
6. Contested authority renders calm **Safety Freeze** state rather than choosing a last-writer winner.
7. Permanent revoke/remove is a distinct security-critical action.
8. UI never claims already-stored remote data was erased merely because future authorization ended.

## Journey: recovery

1. Settings → Privacy & Security → Recovery (or Omnibar `recovery`).
2. Default configured state names its custodian truthfully: synced passkey where available, recovery key fallback/required under profile as architecture specifies, or pending/unavailable state.
3. Identity recovery, history restore and provider/discovery reconnect remain separate tasks.
4. Start recovery displays the canonical delay/veto semantics in human wording.
5. Existing eligible devices can veto where architecture provides that authority.
6. Turning recovery off requires explicit consequence confirmation: loss of all authorized devices can make identity unrecoverable.

## Journey: Protection profile versus experience profile

- **Protection:** Recommended / Hardened / Maximum Privacy / Custom — network/metadata/push/recovery policy, never weaker cryptographic floor.
- **Experience:** Focus / Balanced / Playful — expressive motion/sound/haptic intensity and attention rendering.

Changing one never silently changes the other.

## Journey: service/bridge plaintext boundary

1. Conversation membership/service change would expose plaintext to a service/bridge or representation path.
2. Mandatory non-themeable notice names who else can read/author and what boundary changed.
3. User can Review and, where authorized, revoke/remove the service/bridge.
4. Routine E2EE remains unbadged; plaintext-boundary changes are exceptional security UI.

## Journey: report with selective evidence

1. User chooses Report from person/message/conversation context.
2. UI explains destination/provider/external handler when known and lets user inspect/select the evidence to disclose.
3. Core creates/scopes the EvidencePackage according to architecture.
4. Blocking is separately available and does not imply a universal OM abuse desk exists.

## Journey: customize OwnMessenger

1. Settings → Appearance / Interaction or Omnibar directly searches the desired setting.
2. Theme pair, density, safe typography, tabs, experience profile, sounds/haptics and permitted shortcut bindings are configurable.
3. Theme validation blocks code/remote tracking/protected-state spoofing/accessibility violations.
4. Security surfaces retain reserved semantics/fonts/colors where required.

## Journey: offline/local

1. WAN degradation is represented by affected capabilities/objects; avoid simplistic global Online/Offline claims when local paths remain available.
2. Local history/search remain available.
3. Existing LAN-related peers may communicate where the canonical LAN profile is actually available.
4. Queued content stays recognizable and reconciles on route restoration.
5. Local receipt and configured provider/storage durability remain distinguishable where relevant.
