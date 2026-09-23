# OM Spaces, Rooms, Communities and Publication Architecture

- **Status:** Canonical pre-engineering architecture
- **Checkpoint:** 2026-09-23 06:15 PHT
- **Scope:** Groups/rooms, Spaces/Communities, linking, membership, authority delegation, announcement rooms and public broadcast publication

## Decision summary

OM uses three distinct object classes rather than stretching one “group” abstraction over incompatible trust models:

```text
SpaceID                       organizational/control container
   |
   +-- ConversationID         interactive Room; private V1 rooms use MLS
   +-- ConversationID         another independent Room
   +-- PublicationID          public signed one-to-many feed
   +-- SpaceID                nested Space / organizational grouping
```

A **Community** is the ordinary-user presentation of a `Space`; it is not a second protocol object.

A private group chat is an interactive **Room** whose stable wire identity is its existing `ConversationID`. OM no longer needs a separate `GroupID` for the same object.

## Why these objects stay separate

A Space answers “what belongs together and who may organize it?”

A Room answers “who may participate in this interactive conversation, under what security and room policy?”

A Publication answers “who may publish signed public posts and how may untrusted mirrors distribute them?”

Collapsing those questions creates hidden coupling between organizational membership, encryption membership, subscriber privacy and public-content hosting. OM keeps them explicit.

## ConversationID is the one interactive-room identity

For OM-native interactive conversations:

- `ConversationID` is the stable provider-independent identifier;
- a private group Room binds that `ConversationID` into its MLS/group-control context;
- “RoomID” is a product/architecture term, not another mandatory wire identifier;
- moving/linking a Room into a Space does not change its ConversationID;
- a 1:1 conversation remains a ConversationID established by its RelationshipBinding and is not made into a Space child by default.

This removes the previous redundant `GroupID`/`ConversationID` pair.

## Space identity and state

`SpaceID` is random, stable and provider-independent.

Conceptual state:

```text
SpaceControlState
  space_id
  state_version
  previous_state_hash
  profile
    name
    avatar_ref?
    description?
    style_ref?
  membership[]
    principal_ref
    role_ids[]
  role_definitions[]
  child_links[]
  governance
  authorization_evidence
```

A Space is not itself a giant chat transcript and is not one giant MLS group.

Space control state is authenticated by its governance authority. Storage, directories, relays, catalogs and discovery services can distribute or index it but do not become its owner.

## Space control-state convergence

Space authority/security state is a canonical hash-chained control state, not last-writer-wins UI data. Every accepted transition references the prior accepted state hash and satisfies that prior state's governance policy.

If two otherwise valid authority-changing successors of the same head appear, clients do **not** choose a winner by timestamp, relay arrival order or provider preference. The Space enters a control-conflict/frozen state for privileged transitions until a conflict-resolution transition references the competing heads and is authorized under the **common pre-fork governance**. If that quorum can never be recovered, participants can fork/migrate to a successor Space.

A policy-neutral coordinator may reduce routine conflicts but is never a root of authority. This mirrors OM's broader rule that availability machinery cannot become constitutional power.

## Space privacy and distribution

A public Space may intentionally publish its profile and public child catalog as signed data. Following/browsing a public Space does not require a canonical public member roster.

A private Space's non-public control state (membership/roles/private organization) is E2EE-distributed to authorized recipients using normal OM delivery primitives; it is not required to become a public directory object or one giant chat transcript. Providers may optimize ciphertext fanout, but the optimization does not own the Space.

Former members may retain metadata legitimately learned before removal, just as former Room members may retain prior plaintext. Exact traffic-analysis resistance, member/link selective disclosure and large-Space metadata minimization are owned by the dedicated metadata-threat-model pass and must not be overstated by the UI.

## Child links are affiliation, not possession

A Space can reference Rooms, Publications and nested Spaces. A Room/Publication may be listed in more than one Space.

An **official linked-child relationship is bilateral**:

1. the Space control state proposes/records the child link; and
2. the child control state accepts the affiliation.

Either side may revoke the link. A unilateral hyperlink/reference may still be displayed as an external reference, but it cannot claim the child is an official member of the Space.

This prevents a malicious Space from falsely “claiming” an independent Room, creator or Publication.

## Linking does not imply membership

Joining a Space does **not** silently join every child Room, and joining a Room does not silently join the Space.

A Space member may see eligible linked Rooms and use one-tap UX to request/join them, but each Room executes its own admission and, for private Rooms, its own MLS membership transition.

Benefits:

- users do not unknowingly join/decrypt unrelated rooms;
- Room membership remains independently portable;
- large Spaces do not force one enormous cryptographic roster;
- leaving one Room does not destroy Space membership;
- independent Rooms can be shared across Spaces.

## Space membership as admission evidence

A Room may explicitly set a join rule such as:

```text
allow if current member of Space X
allow if assigned Space role Y
```

This is **admission eligibility**, not hidden perpetual membership inheritance. The Room still records its own membership and performs its own cryptographic add/remove transition.

If eligibility cannot be authenticated, automatic admission fails closed and an explicit Room invite remains possible if Room policy permits.

## Space-managed rooms: explicit delegation, never ambient inheritance

A Room may opt into selected Space-managed capabilities using a `SpaceAuthorityBinding`.

Baseline constraint: a Room has at most **one authority-parent Space** for delegated management. It may be affiliated/listed in many other Spaces, but those links have no authority.

The binding declares exactly which Room capabilities the parent may manage, for example:

```text
manage_room_profile
manage_members
manage_role_bindings
moderate_content
manage_topics
```

It does **not** automatically include:

```text
change_encryption_or_security_profile
add_plaintext_bridge
add_full_plaintext_service_member
change_room_governance
```

unless the Room explicitly grants those high-risk powers under its own governance policy.

### No live transitive permission magic

Space role changes do not become invisible dynamic authorization checks inside every message.

Where a Space role is mapped into Room authority, the Room maintains an authenticated room-local binding/mirror through its normal control-transition process. Until the Room accepts that transition, its prior room-local authorization state remains the cryptographic truth.

The UI may show “community role update propagating” rather than pretending distributed revocation is instantaneous.

This avoids a Room's correctness depending on a mandatory always-online Space server and prevents ambiguous multi-parent permission diamonds.

## Nested Spaces

Spaces may contain nested Spaces for organization. Organizational links form a graph and clients must traverse them cycle-safely.

Authority does **not** transitively flow through arbitrary nesting. Delegated authority requires an explicit direct binding at the controlled object. This prevents `Space A -> Space B -> Room C` from granting A unreviewed control of C merely because the UI is nested.

## Shared style and policy

A Space may publish reusable templates for:

- names/icons/category organization;
- suggested role bundles;
- notification defaults;
- onboarding text/rules;
- theme/content-pack suggestions;
- Room policy templates.

A child may **snapshot** a template or explicitly **follow** future template updates where safe. Visual style can never override theme-proof security UI, identity warnings, bridge/plaintext boundaries or other mandatory safety semantics.

Templates are configuration assistance, not hidden authority.

## Governance: no permanent magical owner

OM does not require an irremovable unique `owner` field.

A newly created Room or Space can begin with a simple one-person governance set so consumer UX can still say “you created this.” The protocol authority is capability/governance based and may later become multi-party.

“Transfer ownership” is therefore a governance transition:

```text
add successor authority
(optionally change approval threshold)
remove predecessor authority
```

This supports ordinary single-admin UX, organizations with multiple stewards and future constitutional/quorum governance without changing object identity.

### Orphaned governance

If all principals able to change governance disappear and no pre-authorized succession/recovery path exists, the object becomes **administratively frozen** for actions requiring that authority.

The protocol does not ask SeedLife, a relay, a directory or a dominant client vendor to invent a new owner. Already-authorized ordinary actions may continue. Participants can create/fork a successor Room/Space and publish migration guidance.

This is a deliberate anti-capture property.

## Private announcement/broadcast rooms

A private announcement channel is not a new cryptosystem. It is an ordinary private MLS Room whose permission state grants `room.post` only to selected publishers while other members may receive/read and, if policy allows, react/reply.

This covers family/community/organization announcements without adding sender-key broadcast crypto to V1. Because recipients are actual Room members, this is **not** a promise of a hidden subscriber roster. A future large private channel with publisher-only visibility of subscribers would require a separately reviewed private-broadcast design.

Operational scale remains subject to MLS/runtime verification; OM does not promise an unbounded subscriber count merely because the UX says “announcement.”

## Public Publication is a different object class

A public one-to-many channel uses `PublicationID` and signed public publication events.

Conceptually:

```text
PublicationControlState
  publication_id
  profile
  publisher_authority
  signing_key_state
  optional_discussion_conversation_id
  governance

PublicationEvent
  publication_id
  post_id
  clock
  kind: post | edit | tombstone
  body
  signing_key_ref
  signature
```

Public posts require durable publication authentication because they may be copied and served by untrusted mirrors outside an MLS/session transcript.

This is intentionally different from the native private-message rule that avoids a separate durable signature on every message.

## Publication control and signing-key continuity

`PublicationControlState` is likewise hash-chained and governed. Publication signing keys are subordinate operational keys authorized by that control state; they are not the permanent identity of the Publication. Rotation/revocation does not change PublicationID.

Each signed public event identifies the signing-key reference and the applicable publication control state. A compromised/retired signing key cannot authorize future posts after an accepted revocation transition. Competing authority-changing publication control heads freeze privileged advancement under the same pre-fork-governance rule used by Spaces.

A post may be signed as the Publication identity without exposing a governor's personal AccountID in every public message.

## Subscriber privacy and discovery

Followers/subscribers of a public Publication are **not** canonical Publication members and need not be disclosed to the publisher or to one another.

Subscription/follow state is local/account-private/provider state unless a user explicitly participates in another object such as a discussion Room.

Directories/search/recommendation services are optional providers. A Publication can exist and be followed via direct descriptor/link without a SeedLife-operated global social graph or mandatory discovery service.

## Public replies/discussion

A Publication may link to an independent discussion `ConversationID`.

A client can present comments/replies naturally, but the post feed and discussion Room remain separate security/authority objects. In the V1 architecture, an OM-native discussion Room uses the existing Room security/membership model; an unrestricted Internet-scale public comment system is **not** silently implied. If OM later standardizes open public interactive Rooms, that receives its own reviewed security/abuse/metadata design rather than piggybacking on Publication signatures.

## Public deletion semantics

A signed public post can later receive a signed tombstone/retraction, and cooperative mirrors/clients should reflect it. OM cannot promise erasure of content already copied from a public feed.

The UI should distinguish “publisher retracted this” from “this never existed.”

## What V1 deliberately does not invent

- no mandatory homeserver owns a Space or Room;
- no Space-wide giant MLS roster;
- no automatic all-child membership inheritance;
- no unrestricted global Administrator bit;
- no unique permanent creator/owner override;
- no transitive nested-Space authority;
- no silent bridge or bot plaintext access inherited from a parent Space;
- no massive private subscriber-broadcast cryptosystem beside MLS;
- no requirement for SeedLife to operate community discovery, hosting or moderation.

## Resolution ownership

This document resolves the architectural core of **A-005 communities** and **A-006 broadcast**. Permission mechanics and topic/thread organization are frozen in [`SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`](SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md).

Bot/LLM installation (A-004), plugins (A-009) and external-network bridges (A-011) are now resolved and all plug into these explicit principal/authority boundaries rather than bypassing them.

## 2026-09-23 08:21 metadata and moderation refinement

A-013/A-014 are now resolved by [`METADATA_THREAT_MODEL.md`](../../threat-model/METADATA_THREAT_MODEL.md) and [`DECENTRALIZED_ABUSE_MODERATION.md`](../safety/DECENTRALIZED_ABUSE_MODERATION.md).

For private Spaces/Rooms, content E2EE does not by itself justify a claim that membership, traffic timing or child-link activity is hidden from every provider/participant. Provider-visible canonical Room/Space identifiers are minimized by scoped routing capabilities and encrypted private control distribution; exact exposure is path-specific.

Space-wide moderation is authoritative in a child Room only when that Room's explicit `SpaceAuthorityBinding` delegates the relevant capability. Shared third-party `SafetyAdvisory` data can influence admission/moderation policy but never bypasses Room/Space authorization or becomes a universal ban.
