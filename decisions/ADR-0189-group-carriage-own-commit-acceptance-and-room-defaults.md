# ADR-0189 — Group carriage over sealed delivery, when a committer accepts its own Commit, and Room defaults by protection profile

- **Status:** Accepted
- **Date:** 2026-11-14
- **Decided by:** operator delegation 2026-11-14 to fill silence, from the operator's words: "make the best decisions so groups are usable and create no friction; easy as possible with different security profiles; users can tighten security later and accept the convenience cost; best balance while giving users control" (ADR-0187 confirmation, same day). This ADR fills silence only and amends no audited text (AGENTS "Design gate" rule 6); where it found a contradiction instead of silence it stops and writes the question (see "Not decided here").
- **Answers:** Q2 (carriage), G9 (signed intent envelope, device-to-account mapping, MLS credential content) and G10 (`commit_accepted`) of [`2026-11-14-groups-driver-status.md`](../docs/reports/2026-11-14-groups-driver-status.md) and [`2026-11-13-groups-plan.md`](../docs/reports/2026-11-13-groups-plan.md); registers the Room context extension `0xF04D`; fixes the Room defaults per protection profile.
- **Amends:** nothing. **Does not amend:** [`GROUP_CHAT_ARCHITECTURE.md`](../docs/architecture/groups/GROUP_CHAT_ARCHITECTURE.md), [`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../docs/architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md), [`SECURITY_PRIVACY_PROFILES.md`](../docs/architecture/ux/SECURITY_PRIVACY_PROFILES.md), ADR-0128, ADR-0142, ADR-0173, ADR-0176, the frozen W0 and W1 registries (additions only, ADR-0119), any CryptoProfile, [`om-room-capabilities.json`](../spec/registries/om-room-capabilities.json).

## Design sources read, and not read

Read in full: [`AGENTS.md`](../AGENTS.md) "Design gate"; GROUP_CHAT_ARCHITECTURE; GROUP_COMMIT_AND_STATE_CONVERGENCE; [`SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`](../docs/architecture/groups/SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md); SECURITY_PRIVACY_PROFILES; [`GROUPS_AND_SPACES_UX.md`](../docs/ux-design/specs/GROUPS_AND_SPACES_UX.md); [`CONTENT_AND_MICROCOPY_VOICE.md`](../docs/ux-design/specs/CONTENT_AND_MICROCOPY_VOICE.md) (rules and security terminology); ADR-0128, ADR-0142, ADR-0187, ADR-0188, ADR-0177 (header and decision); [`group.cddl`](../spec/cddl/group.cddl) (first 150 lines), [`delivery.cddl`](../spec/cddl/delivery.cddl) (sealed-delivery part), the room-capabilities registry, the object-type, domain-separation and W1-event-body registries, the `event_kind` and `extensions` parts of the numeric-fields registry; the two groups reports named above; the delivery and metadata passages on groups ([`RELAY_AND_DELIVERY_ARCHITECTURE.md`](../docs/architecture/delivery/RELAY_AND_DELIVERY_ARCHITECTURE.md) "Group fanout correlation", [`METADATA_THREAT_MODEL.md`](../docs/threat-model/METADATA_THREAT_MODEL.md) sections 5 and 6, home-api deposit-capability rows).

Not read in full (read by grep or not at all): the other documents `python3 tools/design_gate.py needs` prints for the code-path domains (about 50 documents, 67 ADRs), in particular [`NOTIFICATIONS_AND_PUSH.md`](../docs/architecture/client/NOTIFICATIONS_AND_PUSH.md), [`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](../docs/architecture/delivery/MULTI_RELAY_AND_DELIVERY_RESILIENCE.md), ADR-0121, ADR-0122, ADR-0173, ADR-0176 (only cited from the documents above), the identity documents behind the ACS head, and the whole of [`group.cddl`](../spec/cddl/group.cddl) after line 150. Because of that, the CDDL below is **not** placed in `spec/cddl/` (a design-bound path): the implementing session moves it there after reading those domains and adds the two-codec vectors, as ADR-0176 did for the first W1 objects.

## Context

The documents say that a Room invitation, the accept, and the Welcome travel "over the existing 1:1 relationship" and that every group copy is wrapped per recipient and sent to that device's opaque delivery capability (GROUP_CHAT "Creating a group", "Adding a person", "Delivery"; ADR-0128 rules 5 and 7). They give no object type, no frame, and no way for a member to learn another member's delivery capability. They say a relay acknowledgement is never acceptance (GROUP_COMMIT section 5 rule 3) and that a member "merges the best valid Commit it has seen at once" (section 3.2 rule 3), but not when the committer may treat its own Commit that way. They name the profiles Recommended, Hardened and Maximum Privacy and the Room policy fields, but give no Room defaults per profile. Without these, no real group can be created, and a first group must be usable with no setup.

## Decision

### 1. Carriage: two lanes, both over delivery that already exists

No new Home endpoint, no directory, no OM-run service, no new authority. The Home sees what it sees for a 1:1 message today: an opaque deposit capability and a padded sealed envelope (ADR-0142 section 12).

**1.1 Relationship lane (consent only).** Invite, Accept and Decline are `ConversationEvent`s (object type 10) in the existing 1:1 conversation with the person, inside the ordinary sealed 1:1 envelope, unchanged. Three W1 event kinds are **reserved** here and entered in [`om-numeric-fields.json`](../spec/registries/om-numeric-fields.json) and [`om-w1-event-bodies.json`](../spec/registries/om-w1-event-bodies.json) with the CDDL: `15 group.invite`, `16 group.accept`, `17 group.decline`. Each carries `required_capabilities = ["om.group.v1"]`, so a client without groups shows the ordinary unsupported-content placeholder (ADR-0128 rule 18). Bodies are in the appendix. The invite carries the Room name, an optional member count and an opaque `invite_id`; it carries no ConversationID, no member names and no KeyPackage, so nothing about the invitee reaches the Room before it accepts (GROUP_CHAT "Adding a person" step 1).

**1.2 Device lane (everything after consent).** Commit, LeaveRequest, application messages, Welcome, admit requests and route updates travel as a new unsigned object `GroupTransportFrame` (type 33), sealed in a `SealedDeliveryEnvelope` (type 23, ADR-0142 unchanged, same buckets, same padding) to **one destination device through its per-Room route**. The frame carries no sender, no ConversationID and no MLS group id, and relays never see it (ADR-0128 rule 7). HPKE open authenticates nothing (ADR-0142 section 7): the sender is authenticated by what is inside, which is MLS (leaf signature for a message, Commit signature plus the device-signed `GroupCommitBinding` for a Commit) and the signed intent. Frame kinds: `1 commit`, `2 leave_request`, `3 app`, `4 welcome`, `5 admit_request`, `6 route_update`. The Welcome therefore goes straight to the invitee's released route, so the committer needs no 1:1 relationship with the invitee. An inviter who cannot author the admit sends the accepted release to a device that can in an `admit_request`.

**1.3 Per-Room routes (how a member learns where to send).** A device gives each Room its own `GroupRouteRecord` (type 32, appendix): its device id, a fresh HPKE `delivery-key-material` (the existing struct, bound by `route_binding` to the relay entry) and one to four existing `relay-entry`s, signed by the device (`OM-GROUP-ROUTE-SIG-v1`, role device). It is a new object, not a re-use of `SealedDeliveryKeyRecord` (type 22), because ADR-0142 section 2 confines type 22 to the accepted relationship. The deposit capability inside it is minted with the existing `POST /v1/relay/deposit-capabilities` (home-api), one per Room per device on the device's own queue, so the Home can neither tell a Room route from a 1:1 route nor link two Rooms. A route is released in the Accept (to the inviter), reaches existing members in the Commit's added-device list, reaches the new member in the Welcome's directory, and is replaced by `route_update` frames. **After every accepted Commit that removes a member or device, each remaining device mints a fresh route, sends `route_update` to the new roster and revokes the old deposit capability after a bounded overlap**, so a removed member's copy of its capability stops working (the ADR-0142 section 9 pattern). A sender never sends to a device whose route is missing or expired: it queues and refreshes, never falls back (ADR-0142 section 9).

**1.4 G9 resolved from the existing objects.** (a) The intent in a Commit, LeaveRequest or admit request is the frozen `SignedEnvelope` of a `GroupControlIntent` (type 29, device role, label from ADR-0176), not a Core-private string. (b) The device-to-account mapping is made by the **receiver** from the account's `AccountControlHead` it has validated (type 2), never from the frame; the releases carry the account's current signed head and the device's `DeviceCredential` (type 7) as evidence, refreshed by the `acs_head_hash` in `group-authorization`. (c) The MLS credential is a BasicCredential whose identity is the canonical signed `DeviceCredential` of the device, which is what GROUP_CHAT "OM authentication for MLS" says it binds to; a receiver refuses an Add whose credential does not chain to a validated head. This replaces the released `pk` of the current Core stand-in.

**1.5 Room context extension.** MLS GroupContext extension type `0xF04D` (RFC 9420 private-use range `0xF000` to `0xFFFF`) is registered as `om.room_context` in the new [`om-mls-extensions.json`](../spec/registries/om-mls-extensions.json): critical, content `{0: 1 version, 1: conversation_id}`, declared in every KeyPackage's capabilities, never in a clear relay header. It is the only place the ConversationID lives inside MLS (ADR-0128 rule 7). Further OM MLS extension ids need their own registry row.

**1.6 Size.** A Welcome frame and an Accept must fit one sealed bucket (at most 131,068 bytes of frame, the largest ADR-0142 bucket). A Room whose Welcome would not fit refuses the Add with `room_too_large` until the larger-envelope profile that ADR-0177 and ADR-0176 F-27 leave open exists. No chunking is invented here.

### 2. G10: a committer accepts its own Commit at once, when it is durable and unopposed

A committer treats its own Commit as accepted, promotes MLS and `GroupControlState` together (GROUP_COMMIT section 5 rule 4) and releases its Welcome and effects, when **all** hold:

1. the exact Commit bytes, the `GroupCommitBinding` and the intent are durable in the journal and the outbox, with every destination's frames queued in order, the Commit before any frame of the new epoch (section 5 rules 1 and 2, ADR-0120 K1);
2. the parent `(GroupControlState hash, MLS epoch)` is still canonical locally;
3. no valid Commit with the same parent that sorts before it (section 3.2 rule 2) has been received.

There is no timer and nothing waits on a relay or a recipient: a relay or recipient acknowledgement is not consulted, because it is not acceptance (section 5 rule 3), and relay order is not canonical order ([`RELAY_AND_DELIVERY_ARCHITECTURE.md`](../docs/architecture/delivery/RELAY_AND_DELIVERY_ARCHITECTURE.md)). `sequencing_timeout` stays what ADR-0128 rule 1 makes it, the limit on a pending reducing intent, and is not part of acceptance. This is section 3.2 rule 3 applied to the committer, which is not special: it merges the best valid Commit it has seen. It is what makes Add feel instant, including offline.

A better Commit that arrives later is the existing losing-branch case (section 3.2 rule 3, section 5 rule 7): `SUPERSEDED`, messages sent on the losing branch re-queued, a still-applicable reduction rebased, a non-reducing intent revalidated. Competition is rare: non-reducing intents are sequenced by the one coordinator (section 3), so only a reduction, which sorts first and rebases, can win. A Welcome already sent for a Commit that loses leaves the invitee on a dead epoch; a revalidated admit is committed again and its new Welcome, carrying the same `invite_id`, replaces the old state; undecryptable late messages render as a gap (ADR-0121 rule 4). The driver's `commit_published` is a progress event only; `CommitAcceptance` is implemented by the three conditions, and the Core step that finishes a Commit returns `superseded` if condition 3 fails.

### 3. Room defaults by protection profile

The profile names are the existing ones. **Recommended** is the default, and is what the operator calls "easy"; the product does not add a fourth name (canonical-term rule, CONTENT_AND_MICROCOPY_VOICE). `group-policy.security_profile` takes `1 Recommended`, `2 Hardened`, `3 Maximum Privacy`; values `4` to `65535` are reserved; "Custom" is not a value, the client shows it when any field differs from its bundle (SECURITY_PRIVACY_PROFILES section 2). Defaults are applied at creation only. Presets never change the cryptographic floor (section 1 of that document); they set the Room fields below and the per-person settings in the last table.

**Per Room (shared, in `GroupControlState`; changed only by a holder, never by a profile change elsewhere):**

| Setting and plain label (what it means) | Field | Recommended (default) | Hardened | Maximum Privacy |
|---|---|---|---|---|
| **Who can add people.** "Anyone in the group can add people" / "Only admins can add people" | `Member` role holds `room.invite` or not | Anyone | Only admins | Only admins |
| **Do new people need an admin's OK?** "Joining is automatic once they tap Join" / "An admin approves each new person" | `join_rule` | `invite_only` (1): the admission holder's device completes the add by itself | `admin_approval` (2): an admin approves each | `admin_approval` (2) |
| **Invite links.** "Links are off until an admin turns them on" | `invitations[]`, `join_rule` | off; turning on sets `admin_approval`; `open` only by a separate explicit choice, never by a profile | same | same, and links are not offered at creation |
| **Earlier messages for new people.** "New people don't see earlier messages" | `history_policy` | `since_join` (1) | `since_join` | `since_join` |
| **Disappearing messages.** "Messages stay until deleted" / "Messages disappear after <time>" | `disappearing_timer` | off (opt-in) | off (opt-in, offered at creation) | off (opt-in, offered at creation) |
| **A member's new device.** "New devices of people in the group join automatically" / "An admin approves new devices" | `new_device_policy` | `automatic` (1) | `automatic` | `admin_approval` (2) |
| **Protection level** | `security_profile` | 1 | 2 | 3 |

Everything else keeps its existing default: `sequencing_timeout` 300 seconds, `past_epoch_window` 2 (ADR-0121 rule 4, ADR-0128 rule 1), default roles `Admin` and `Member` as GROUP_CHAT offers them.

**Per person (local, never in the Room, never sent; SECURITY_PRIVACY_PROFILES bundles and NOTIFICATIONS):**

| Setting and plain label | Recommended | Hardened | Maximum Privacy |
|---|---|---|---|
| **Notifications.** "All activity" | all activity, content shown | generic lock-screen text | generic text, mentions and replies only |
| **When someone's safety code changes.** | "Tell me" (a notice, sending continues) | "Ask me before I send" | "Block sending until I review" |
| **Typing, read and presence.** | per normal choice | reduced | off |

Blocking on a changed code stops sending to the whole group, because MLS has no per-member send; the screen says so ("Sending to this group is paused until you review Maria's new safety code").

**Joining takes one tap.** The invitation shows in the 1:1 thread as "<Name> invited you to <Room>" with **Join** and **Not now**, the Room name, the inviter and the number of people, and the line "People in the group will see your name and devices." One tap on **Join** sends the Accept with the releases; nothing else is asked. Under `invite_only` the person shows as "Joining" until an admission holder's device completes the add (state "Waiting for an admin to be online" if none is). Member names are not shown before Join in any profile.

**Tightening, loosening, and what is never silent.** The creator may pick a stricter profile at creation (**New group**, then **Protection**). Any holder of `room.manage_security_policy` may tighten later; choosing a stricter profile sets the fields above for that Room and shows the cost in one line each ("New people will need an admin's OK, which can delay joining"). Anything that makes a Room less strict (lower `security_profile`, `join_rule` toward `open`, `history_policy` wider, `new_device_policy` back to `automatic`, `Member` regaining `room.invite`) is an explicit, separately confirmed action, subject to any governance quorum, and every member sees a Room notice saying what changed and who changed it ("Alice changed who can add people: now anyone in the group"). A software update, a theme, a personalization suggestion or a new default never changes an existing Room's fields (SECURITY_PRIVACY_PROFILES closing paragraph and section 5).

**Offered in the opposite direction of the request, and why.** "Hide the member list from other members" is **not offered**: every MLS member holds the ratchet tree and every Add reveals the credential to all members ([`METADATA_THREAT_MODEL.md`](../docs/threat-model/METADATA_THREAT_MODEL.md) section 6: membership known to participants is "not described as hidden"). The product states this once in the Room details ("Everyone in the group can see who is in it") and offers no control that would imply otherwise. Non-members never see the list in any profile (invite previews show only a count).

## Not decided here (the operator's call)

- **Q1.** Whether `join_rule = invite_only` means a `room.invite` holder's invitation alone authorizes the Add. The registry row `control.member.admit` requires `room.manage_admission` and waives it only for the open-link case, so this ADR does not waive it: under Recommended the Add is authored by a device whose account holds `room.manage_admission` (the creator, any Admin), automatically, with no prompt; `admin_approval` makes that a prompt. The cost is the "waiting for an admin to be online" state. Removing it needs an operator-accepted ADR that amends the registry note on `control.member.admit` to say an accepted invitation from a `room.invite` holder suffices under `invite_only`.
- The size profile for Rooms whose Welcome exceeds one bucket (1.6), and the Hardened or Maximum Privacy opt-ins that need a second device class (relayed group calls, push policy), which stay with their own documents.

## Consequences

- The Core `GroupWire` JSON becomes: Invite, Accept, Decline on the relationship lane; Commit, LeaveRequest, App, Welcome and the new admit request and route update as `GroupTransportFrame`s; the Target `Device` resolves to a per-Room route. The host `GroupCarriage` port is implemented over the sealed-delivery client that exists; `commit_accepted` is the three conditions above, so `acceptAfterDelivery` is replaced and `VITE_OM_GROUPS` can come off the refuse list once the groups module's shipping (digest in the release manifest) is decided.
- Deviations D32 and D34 in [`2026-11-14-groups-driver-status.md`](../docs/reports/2026-11-14-groups-driver-status.md) close when that implementation lands and cites this ADR in `Design-Departures`.
- New wire: object types 32 and 33, event kinds 15 to 17, label `OM-GROUP-ROUTE-SIG-v1`, extension `0xF04D`; CDDL and two-codec vectors before code ships. Wire cost: one route record per device per Room (a few kilobytes); one deposit capability per Room per device at the Home, against the same per-queue quotas.
- Threat-model consequence (T-060 to T-066 family, METADATA_THREAT_MODEL section 6): the new exposure is that each Room member holds every other member's per-Room deposit capability, so a removed member could spam until the rotation in 1.3 completes; the bounded overlap and revocation limit it. Negative vectors owed with the CDDL: frame from a non-roster device, route for another device id, stale route, Welcome without a preceding accepted invite, Accept for an unknown `invite_id`, Welcome frame over the bucket.
- Group traffic metadata equals 1:1 metadata for the Home (opaque capability, bucket, timing). Every Room member holds every other member's deposit capability for that Room, which is the ordinary Room trust boundary; removal rotates it.

## Where did authority go?

Nowhere new. Invite, accept and decline are consent by the invitee over an existing relationship; the Add is authorized by the same capabilities as before; a route is a capability its owner mints and can revoke; acceptance is a local judgment under the existing ordering rule, never a relay's or the project's.

## Appendix: CDDL (to move to `spec/cddl/group-transport.cddl`)

```cddl
; Reuses common.cddl (opaque-id16, device-id, conversation-id, posix-seconds, signature, extensions, digest) and
; delivery.cddl (delivery-key-material, relay-entry). Signed objects use SignedEnvelope with role device.

signed-GroupRouteRecord = { 1: bstr .cbor GroupRouteRecord, 2: [1*16 signature] }
GroupRouteRecord = {
  0: 32, 1: 1,
  2: device-id,                             ; the destination device
  3: delivery-key-material,                 ; fresh per Room; route_binding over the first relay entry
  4: [1*4 relay-entry],
  ? 23: extensions,
}

GroupTransportFrame = {
  0: 33, 1: 1,
  2: 1..6,                                  ; frame_kind: commit, leave_request, app, welcome, admit_request, route_update
  3: bstr .size (1..131000),                ; body, per kind below
  ? 23: extensions,
}
frame-body-commit = {
  0: bstr .cbor signed-GroupControlIntent,  ; type 29 envelope
  1: bstr .cbor signed-GroupCommitBinding,  ; type 30 envelope
  2: bstr,                                  ; MLS Commit
  ? 3: [1*64 group-release-evidence],       ; devices this Commit adds
}
frame-body-leave-request = { 0: bstr .cbor signed-GroupControlIntent }
frame-body-app = bstr                       ; MLS PrivateMessage; nothing else
frame-body-welcome = {
  0: opaque-id16,                           ; invite_id
  1: bstr,                                  ; MLS Welcome (ratchet tree inside)
  2: bstr .cbor signed-GroupControlState,   ; type 28 head
  3: [1*512 group-member-route],
}
frame-body-admit-request = { 0: opaque-id16, 1: [1*16 group-release], 2: bstr .cbor signed-AccountControlHead }
frame-body-route-update = { 0: [1*512 bstr .cbor signed-GroupRouteRecord] }

group-release = {
  0: device-id,
  1: bstr,                                  ; MLS KeyPackage, released for this Room only
  2: bstr .cbor signed-DeviceCredential,    ; type 7
  3: bstr .cbor signed-GroupRouteRecord,
}
group-release-evidence = { 0: device-id, 1: bstr .cbor signed-DeviceCredential, 2: bstr .cbor signed-GroupRouteRecord }
group-member-route = { 0: device-id, 1: bstr .cbor signed-DeviceCredential, 2: bstr .cbor signed-GroupRouteRecord }

; ConversationEvent bodies, kinds 15 to 17 (relationship lane); kind 15 body:
body-group-invite  = { 0: opaque-id16, 1: tstr .size (1..256), ? 2: 1..512 }   ; invite_id, room name, member count
body-group-accept  = { 0: opaque-id16, 1: [1*16 group-release], 2: bstr .cbor signed-AccountControlHead }
body-group-decline = { 0: opaque-id16 }
```
