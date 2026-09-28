# ADR-0128 — Group reduction liveness, membership consent and room capabilities

- **Status:** Accepted
- **Date:** 2026-09-24
- **Decided by:** integrator under operator delegation 2026-09-24 ("do everything and complete it"), from the
  architecture review
- **Source:** [`2026-09-24-claude-opus-architecture-r2.md`](../verification/reviews/2026-09-24-claude-opus-architecture-r2.md) (F-R2-1 .. F-R2-21)
- **Amends:** [`GROUP_CHAT_ARCHITECTURE.md`](../docs/architecture/groups/GROUP_CHAT_ARCHITECTURE.md), [`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../docs/architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md),
  [`SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`](../docs/architecture/groups/SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md), [`SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md`](../docs/architecture/groups/SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md),
  [`EVENT_MODEL.md`](../docs/architecture/events/EVENT_MODEL.md), [`MESSENGER_BASELINE_V1.md`](../docs/architecture/events/MESSENGER_BASELINE_V1.md), [`TIME_AND_EXPIRY_SEMANTICS.md`](../docs/architecture/events/TIME_AND_EXPIRY_SEMANTICS.md), [`DECENTRALIZED_ABUSE_MODERATION.md`](../docs/architecture/safety/DECENTRALIZED_ABUSE_MODERATION.md),
  [`EVIDENCE_AND_EXTERNAL_REPORTING.md`](../docs/architecture/safety/EVIDENCE_AND_EXTERNAL_REPORTING.md), [`ABUSE_SAFETY_AND_REPORTING.md`](../docs/architecture/safety/ABUSE_SAFETY_AND_REPORTING.md), [`RICH_TEXT_AND_CONTENT_SECURITY.md`](../docs/architecture/safety/RICH_TEXT_AND_CONTENT_SECURITY.md),
  [`CALLS_AND_REALTIME_MEDIA.md`](../docs/architecture/calls/CALLS_AND_REALTIME_MEDIA.md), ADR-0121 rule 7 (its ids are now registry ids), ADR-0122 (rules 1, 2 and 10
  extended to groups, invites and bans); new registry [`om-room-capabilities.json`](../spec/registries/om-room-capabilities.json); logical schemas
  [`publication-event.logical.schema.json`](../spec/protocol/publication-event.logical.schema.json), [`group-control-transition.logical.schema.json`](../spec/protocol/group-control-transition.logical.schema.json),
  [`call-session.logical.schema.json`](../spec/protocol/call-session.logical.schema.json), [`moderation-safety.logical.schema.json`](../spec/protocol/moderation-safety.logical.schema.json)

## Context

The R2 review found that the group authority model holds, but its MLS boundary does not. Security reductions depend
on one coordinator device being alive and honest. The docs claim deniability that MLS does not give. Group calls and
history sharing work at Room scope while users expect participant scope. Several texts disagree with the frozen W0
CDDL and registries, or with each other. This ADR records the decisions; the canonical docs carry the normative text.

## Decision

1. **Reductions are live without the coordinator.** `GroupControlState` records a coordinator **succession order**.
   A GroupProfile parameter `sequencing_timeout` (monotonic local duration; default 5 minutes, maximum 1 hour,
   calibrated by V-004) bounds how long a pending authority-*reducing* intent may wait. Reducing intents are: removing
   a revoked or quarantined device, removing a member, a self-leave, removing a service or bridge endpoint, and a
   Space-delegated reduction (rule 11). After the timeout, any member holding the capability that authorizes the
   intent may commit it directly. Removing a revoked or quarantined device needs no capability. The same member may
   commit the coordinator's replacement, which is always the next live device in the recorded succession order, so
   nobody chooses it. The coordinator never sequences its own removal or replacement. The coordinator is operational,
   never authority: ADR-0122 rule 1 applies to group reductions. A rebased **non-reducing** intent is re-validated
   against its new parent and dropped if it fails there.
2. **Account-control events in groups.** On learning, at a validated AccountControlState head, that a device is
   revoked or quarantined, a member sends no further application messages until a Commit removing that leaf is
   accepted, and it proposes the Remove itself (rule 1 gives liveness). A lifted quarantine re-adds the device only
   through a fresh add backed by a later authorized head. While an account is in CONTROL_CONFLICT or SAFETY_FREEZE,
   none of its devices is added; removals continue. Automatic device adds are GroupControlIntents deduplicated by
   `(AccountID, device_id, ACS generation)`.
3. **Authentication honesty.** 1:1 sessions give participant-deniable authentication. MLS groups give
   **transferable** sender authentication: the leaf signature chains through DeviceIdentity and AccountControlState to
   the AccountID. EvidencePackage may include MLS FramedContent and signatures as independently verifiable. Deniable
   groups would need a research ADR (absolute rule 3; design queue A-036). Threat T-060 records the device-seizure
   consequence.
4. **HistoryBundle content is attributed to the bundle author.** Events received in a HistoryBundle render as "shared
   by <bundle author>", never as authenticated original authorship. Bundles exclude expired, deleted and redacted
   content. `history_policy` is cooperative policy, not a cryptographic control against members who hold plaintext.
5. **Consent before any group add.** Adding a person is invite, accept, then Add. The adder sends a Room invitation
   over its existing 1:1 relationship; only after the invitee accepts are its KeyPackages released for that Room and
   the MLS Add committed. Group creation follows the same flow for every initial member. KeyPackages are fetchable
   only through relationship-scoped capabilities, never by arbitrary principals.
6. **Group invite links.** A link carries an opaque, rotatable `join_ref` and never the ConversationID. It records
   its authorization basis and is valid only while the issuer holds `room.invite` at the redeeming commit's epoch
   (the ADR-0122 cascade applied to invites). Redemption is a GroupControlIntent sequenced like any other, so
   `max_uses` is counted in GroupControlState. Admin approval is the default join policy for link invites (MUST); a
   Room may choose open links explicitly through a `room.manage_admission` policy change.
7. **Relays never see raw MLS messages.** Each recipient copy is wrapped in an independently encrypted per-recipient
   envelope. The MLS `group_id` is random and unrelated to the ConversationID, which travels only inside the encrypted
   GroupContext extension. A `group_broadcast` provider declares recipient-set linkage in its
   MetadataExposureDescriptor.
8. **CryptoProfile transitions are not governable.** They are automatic and monotone (ADR-0107, ADR-0126 rule 6).
   Governance may change only non-cryptographic Room security policy (the privacy SecurityProfile,
   `new_device_policy`, `history_policy`) and can never lower an established floor. The Space-binding exclusion is
   renamed `room.manage_security_policy`.
9. **One capability registry.** [`om-room-capabilities.json`](../spec/registries/om-room-capabilities.json) lists every Room and Space capability id and
   maps every event kind and control action to its capability. Ids are permanent once assigned; new ids may be added.
   ADR-0121 rule 7 is expressed in these ids. Docs use only registry ids.
10. **Role edits are grants.** Creating or editing a RoleDefinition is itself a grant: every capability and every
    `grantable_scope` in the resulting role must lie within the actor's ceiling. Assigning a role requires the whole
    role to lie within the assigner's ceiling. A created grant's `grantable_scope` is a subset of the grantor's.
11. **SpaceDelegatedIntent.** Space authority acts in a Room through a `SpaceDelegatedIntent`: a GroupControlIntent
    carrying the Space's signed evidence and the Space head it was validated against. Any Room member may commit it;
    Space-side reductions use rule 1's liveness path. The binding obliges the Space to deliver its governance head to
    the Room's members. Admission checks use the freshest head held and fail closed when it is older than the
    binding's `freshness_bound`.
12. **ModerationDecision is a record.** It is a record, export and appeal object and never changes Room state by
    itself. In-Room effect comes only from a GroupControlTransition (remove, ban, restriction) or a
    `moderation.redact` event. ADR-0122 rule 10's committed lift applies to bans and every restriction, not only
    timeouts. ModerationDecision and SafetyAdvisory time fields are display or wall-clock-advisory time with
    `authority_effect: none`.
13. **Publication posts are sequence-chained.** Each PublicationEvent carries a per-signing-key monotonic `seq`, a
    `prev_post_hash` chain and a `control_state_hash`. A revocation transition records the last valid `(key, seq)`;
    posts beyond it are invalid regardless of clock. Subscribers keep the highest validated control head and reject
    rollback (ADR-0122 rule 6).
14. **Group call keys are participant-bound (amended by ADR-0132).** The earlier MLS-exporter-only construction
    was rejected because every Room endpoint holding the epoch could derive it. The resolved design uses per-sender
    SFrame media keys delivered only to explicitly joined participant DeviceIDs and rotates them whenever the joined
    participant-set generation changes. Room MLS epoch possession alone grants no call-media key. Services/bridges
    obtain media keys only by joining as visible call participants under policy. Starting a call that rings the Room
    needs `room.start_call`.
15. **Unknown callers do not ring.** A `call.offer` from a principal without an accepted relationship is dropped. At
    most it becomes a MessageRequest-class "missed call" notice: no ringing, no ICE, no TURN allocation. For
    RoutePolicy an "unknown" peer is any peer that is not an accepted contact, Room co-members included, and mesh
    topologies use relay-only candidates toward such peers.
16. **One expiry rule** (merged with F-R1-12, ADR-0126 rule 9). Envelope `expiry` (key 12, absolute wall clock) is the
    canonical v1 timer. `expires_after` is an optional local bound from receipt. Content is deleted at whichever comes
    first, never later than `expiry`. In W0 1:1 conversations the per-message `expiry` is the only carrier; there is
    no shared 1:1 timer state. Groups carry `policy.disappearing_timer` in GroupControlState, changed under
    `room.set_disappearing_timer`.
17. **Reactions and votes are epoch-major.** Reaction and vote state are last-writer-wins registers ordered by
    `(sending epoch, EventClock)`, as ADR-0121 rule 3. A vote ordered after the poll's `poll.close` in that order is
    ignored. Backdating within one epoch is a documented residual risk.
18. **Later-wave content never breaks W0 receivers.** New rich-text node kinds and attachment fields travel only in a
    named event extension (key 23) beside a W0-valid body, or under a new kind; a critical extension or a
    `required_capabilities` entry makes a W0 receiver show the unsupported-content placeholder. A sender emits a
    reserved later-wave encoding (for example inline type 4, `custom_emoji`) only when every recipient device
    advertises it, and otherwise sends a W0-valid fallback. **ADR-0150 supersedes the earlier list-block deferral in this clause:** bounded ordered/unordered list presentation is now the optional non-critical `om.rich-list.v1` extension over W0-valid paragraph blocks, while the frozen W0 RichText grammar remains unchanged. ADR-0148 separately promotes voice-note presentation as the optional non-critical `om.voice-note.v1` extension beside a W0-valid audio attachment.
19. **Safety references are honest and injective.** The labeler-scoped reference prevents enumeration by parties that
    do not already know the principal; it does not prevent linkage by those that do. Before W3, a DS-HASH construction
    over the CBOR array `[issuer_scope_id, principal_ref]` is registered under a new label and `OM-SAFETY-REF-v1`
    stays unused. Relays and mailbox providers act only on subjects they can already see and never resolve
    AccountID-keyed advisories to delivery capabilities.
20. **Mentions stay inside the Room.** Forwarding converts Mention nodes to plain Text with their display label and
    drops the mentions set. A sender mentions only current members; a receiver renders a mention of a non-member as
    text.
21. **Names.** `GroupControlState` (not GroupState); `GroupControlIntent`/`GroupCommitBinding`, with
    `GroupControlTransition` as their logical union; the envelope's `required_capabilities` is called "required
    features" in prose.

## Consequences

- Requirements R-220..R-239, threats T-060..T-066 and design-queue items A-035 (call key isolation; later resolved by ADR-0132)
  and A-036 (deniable groups, research only) are added. V-004, V-007, V-008, V-009 and V-016 gain criteria or
  remaining items.
- W1 CDDL for GroupControlState must carry the succession order, `sequencing_timeout`, `policy.disappearing_timer`,
  invite `join_ref` and redemption counters; the W3 Publication CDDL must carry `seq`, `prev_post_hash` and
  `control_state_hash`. Nothing in the frozen W0 set changes.
- Registry additions outside this ADR's files (time-fields rows for ModerationDecision and SafetyAdvisory, the new
  safety-reference label) are made by the registry owner under ADR-0119's additions-only rule.
