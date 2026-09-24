# OM Scoped Permissions, Moderation, Threads and Topics

- **Status:** Canonical pre-engineering architecture
- **Checkpoint:** 2026-09-23 06:15 PHT
- **Scope:** Capability-scoped roles, delegation ceilings, restrictions, moderation, governance approvals, lightweight threads and persistent forum topics

## Decision summary

OM uses **capabilities as authority** and **roles as named bundles of capabilities**.

It deliberately avoids making either a numeric role hierarchy or one unrestricted “Administrator” switch the root of all power. Rooms and Spaces can still expose familiar roles such as Admin, Moderator and Member in the UI, but wire authorization is based on explicit capabilities and governance rules.

Conversation organization is layered:

```text
reply       simple reference to one message
thread      lightweight view anchored at a root message
TopicID     persistent named room-local organizational unit
Room        independent membership/security object
Space       organizational/control container above independent Rooms
```

Threads and Topics do not become hidden sub-rooms or new MLS groups.

## Capability model

A grant is conceptually:

```text
CapabilityGrant
  subject_principal
  capability_id
  scope_ref
  grantable_scope?     # what this principal may delegate further
  authorization_evidence
```

The one vocabulary is the capability registry [`om-room-capabilities.json`](../../../spec/registries/om-room-capabilities.json) (ADR-0128 rule 9). It lists every Room
and Space capability id and maps every event kind and control action to its capability; docs, grants, roles and
SpaceAuthorityBindings use only its ids. The lists below are a summary of it.

Room capabilities:

```text
room.post
room.reply
room.react
room.upload
room.pin
room.create_thread
room.create_topic
room.manage_topics
room.invite
room.manage_admission
room.remove_member
room.ban_member
room.timeout_member
room.restrict_member
room.set_slow_mode
room.redact_others
room.manage_profile
room.set_disappearing_timer
room.manage_roles
room.start_call
room.install_service
room.add_bridge
room.manage_security_policy
room.manage_governance
```

(`room.create_thread` is reserved: W1 thread replies need only `room.reply`, ADR-0121 rule 7.)

Space capabilities:

```text
space.view
space.invite
space.manage_members
space.manage_profile
space.manage_links
space.manage_roles
space.moderate
space.manage_governance
```

The registry is versioned and extensible under the normal public registry rules: ids are permanent once assigned and new ids may be added. An unknown critical authority capability cannot be silently approximated.

## Roles are convenience bundles

A role contains a set of capability grants/scopes plus presentation metadata.

```text
RoleDefinition
  role_id
  display_name
  grants[]
```

Assigning several roles produces the union of their valid grants **before restrictions are applied**.

Common defaults can be offered:

- Admin
- Moderator
- Member
- Read-only / Publisher
- Service/Bot-specific role templates

Those names are not protocol superpowers. An independent client can present equivalent role bundles differently while preserving capability meaning.

## No ordinary-role deny maze

Core roles primarily **grant** capabilities. OM does not copy a generic allow/deny/neutral override matrix across server/category/channel layers.

Temporary or disciplinary removal of authority is represented as explicit **restriction state** (for example timeout, posting restriction, ban), not as an ordinary role named “Muted.”

Effective authorization is conceptually:

```text
valid direct/base grants
+ valid role grants
+ accepted delegated grants
- active explicit restrictions
= effective capabilities
```

Restrictions win over ordinary grants for the restricted action.

This keeps “why can Alice do this?” inspectable and avoids surprising conflicts between unrelated roles.

## Delegation ceiling prevents self-escalation

Possessing `room.manage_roles` is not equivalent to possessing every capability.

A principal may grant only capabilities inside its authenticated **grantable scope/ceiling**. For example, a moderator allowed to assign ordinary Member/Helper roles cannot manufacture `room.manage_governance` or `room.add_bridge` for itself or another account.

A role-management action is rejected if the grant being created exceeds the actor's delegation ceiling. Precisely
(ADR-0128 rule 10):

1. **Creating or editing a RoleDefinition is itself a grant.** Every capability and every `grantable_scope` in the
   resulting role must lie within the actor's ceiling. Otherwise, adding a capability to an existing role would raise
   the authority of every holder of that role beyond what the editor may grant.
2. **Assigning a role** requires the entire role to lie within the assigner's ceiling, not only the capabilities the
   assigner looked at.
3. **A created grant's `grantable_scope`** must be a subset of the grantor's own `grantable_scope`.

V-008 carries a negative case for each rule.

`room.manage_governance` / `space.manage_governance` are protected governance-root capabilities, not something an ordinary role manager can mint merely by holding `manage_roles`. The governance state itself defines who may alter that root and what threshold is required.

## High-risk changes may require quorum

Each Room/Space governance policy can declare approval thresholds for selected high-risk transitions. Candidate high-risk actions include:

- changing governance principals/thresholds;
- changing non-cryptographic Room security policy (`room.manage_security_policy`: the privacy SecurityProfile, `new_device_policy`, `history_policy`), never below an established floor. CryptoProfile transitions are automatic, monotone and not governable (ADR-0107, ADR-0128 rule 8);
- adding a bridge that receives plaintext;
- adding a full-room service/bot that receives plaintext;
- delegating authority to a parent Space;
- rotating public Publication signing authority;
- granting another principal the ability to grant those powers.

Consumer groups may use a simple 1-of-1 default. The architecture does not force committees onto families, but it does not hard-code unilateral super-admin control either.

## Human, service and bridge principals stay typed

A human AccountPrincipal, ServicePrincipal/Bot and BridgePrincipal are distinguishable authority subjects.

A service does not become a human merely because it can post. A full-room service or bridge with plaintext access must remain visibly typed and subject to the separate bot/bridge architecture queue items.

Space membership alone never gives a bot plaintext access to child Rooms.

## Moderation state is not role theater

Moderation actions have explicit semantics:

```text
restriction.set       timeout/posting/media/etc. restriction
membership.remove     room/space membership control action
membership.ban        prevents admission until lifted according to policy
moderation.redact     suppresses another principal's message in shared state
```

Where a private MLS Room removes a member, cryptographic removal follows the existing group-control/MLS transition rules.

Room state changes only through these control transitions and `moderation.redact`. A signed `ModerationDecision`
records a decision for export and appeal but never changes Room state by itself ([`DECENTRALIZED_ABUSE_MODERATION.md`](../safety/DECENTRALIZED_ABUSE_MODERATION.md)
§8). A ban or restriction ends only through a committed lift transition (unban, lift, untimeout), never by a clock
passing (ADR-0122 rule 10, extended by ADR-0128 rule 12).

A Space-wide ban can be used as an admission condition for linked Rooms. A Room may also opt into parent-Space ban propagation, but actual Room membership/MLS removal remains an explicit Room control transition; a Space does not magically rewrite child cryptographic rosters. The Space's decision reaches the Room as a `SpaceDelegatedIntent` that any Room member may commit ([`SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md`](SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md)).

## Constitutional fork behavior

If governance becomes abusive or irrecoverably frozen, participants can create a successor Room/Space and publish a migration/fork reference. OM does not create a universal project-operated appeal court or hidden vendor override key.

Provider/community operators may enforce their own lawful service rules at their boundaries, consistent with [`ABUSE_SAFETY_AND_REPORTING.md`](../safety/ABUSE_SAFETY_AND_REPORTING.md), without changing protocol ownership.

## Reply semantics

`reply_to` points to an EventID in the same ConversationID. It is a semantic relation and quoting aid, not a separate permission/security context.

Clients may render compact quoted text, but the target event remains the authoritative source if available.

## Lightweight threads

A thread is a room-local view anchored to a root message:

```text
message.create
  thread_root_event_id?
```

Rules:

- root and reply are in the same ConversationID;
- membership and permissions are the Room's membership and permissions;
- no separate MLS state;
- no separate Room identity;
- a client that does not offer a threaded UI can flatten thread replies into the Room timeline without changing security meaning;
- follow/mute/notification behavior is local/account-private preference.

This handles “branch off this message” without turning every side discussion into a protocol object.

## Persistent Topics

A Topic is a durable named organizational unit inside one Room.

```text
TopicState
  topic_id            random room-local stable identifier
  conversation_id
  title
  status              open | resolved | archived
  created_by
  created_clock
  last_update_clock
```

Topic operations are authorized room-organization events/control state, conceptually:

```text
topic.create
topic.update
topic.status.set
message.topic.set     move/reclassify an existing message when policy allows
```

A Topic does not have its own independent member roster or encryption context.

## Chat mode vs forum mode

A Room may advertise a presentation mode:

- `chat` — normal chronological chat; topics optional;
- `forum` — new top-level conversational posts require a TopicID; clients emphasize topic navigation;
- `announcement` — presentation hint for a Room whose `room.post` capability is restricted.

The security rule comes from capabilities, not the label. Calling a room “announcement” without restricting `room.post` does not make it one.

## Topic permissions

Topic actions use scoped Room capabilities such as `room.create_topic` and `room.manage_topics`. A capability may be scoped to a Topic where needed, but V1 avoids a second full server/channel-style ACL tree under every Topic.

A Topic-specific moderator can be granted narrowly scoped management/redaction capability without receiving Room governance power.

## Follow, mute and notification state

Following/muting a Topic or Thread is user preference, not shared authority. It belongs in local/account-private state and may later be encrypted-synced across that user's devices.

This lets busy forum-style Rooms remain usable without leaking a member's attention/preferences to the Room or provider by protocol necessity.

## Moving messages between topics

`message.topic.set` changes organizational placement, not authorship/content. It is authorized for the message's author (still holding `room.post`) or a holder of `room.manage_topics` ([`om-room-capabilities.json`](../../../spec/registries/om-room-capabilities.json)), and preserves the original EventID and security context.

Clients should show an unobtrusive “moved to topic” history where useful. Moving a message to another **Room** is not the same operation; cross-Room transfer is a new message/forward because the security and membership context changes.

## Resolution ownership

This document resolves **A-007 permissions** and **A-008 topics_threads**.

Exact bot/agent permissions (A-004), plugin-granted UI/data capabilities (A-009) and bridge fidelity/loops (A-011) are separately resolved and use this capability/typed-principal model rather than adding hidden ambient authority.

## 2026-09-24 verification rulings

Normative, from the V-series models. The ADR is authoritative where this summary is shorter.

- [ADR-0121](../../../decisions/ADR-0121-event-ordering-authorization-epoch-and-materialization.md) rule 7 is the kind-to-capability table (reply, edit, delete, vote, close, topic management).
- [ADR-0122](../../../decisions/ADR-0122-authority-reductions-grant-cascade-and-governance-protection.md): every grant records its authorization basis and lapses when that basis is revoked (cascade). Re-rooting a grant needs a governance transition.
