# OM Private Group Chat Architecture

- **Status:** Phase 0 working architecture; core direction selected
- **Date:** 2026-08-18
- **Scope:** Private groups, membership, devices, cryptography, roles, delivery, history, bots, bridges, and attachments

## Decision summary

OM private groups are **provider-independent application groups** secured using the IETF Messaging Layer Security (MLS) architecture, subject to implementation/library review during Gate C.

OM will not invent a new group key agreement protocol.

The human group is made of **AccountIDs**. The cryptographic MLS group is made of **DeviceIdentities**. Users see people; the protocol can revoke individual devices.

## Core invariants

1. `GroupID` is random and provider-independent.
2. No relay, directory, storage provider, or OM-operated service owns the group.
3. Human-visible membership is AccountID-level.
4. Cryptographic membership is DeviceIdentity-level.
5. Removing an account removes all its active device clients from future group epochs.
6. Removing one compromised device does not remove the human account.
7. New members do not automatically obtain pre-join history.
8. Optional history sharing uses a separately encrypted history transfer, not old group epoch secrets.
9. Roles/admin policy are application-layer, not implicit in MLS.
10. Bots and bridges are explicit principals/trust boundaries.
11. External bridges never silently inherit the OM-native E2EE security label.
12. Public channels/feeds are a separate product class and are deferred.

## Group identity

```text
GroupIdentity
  group_id        random value; exact encoding TBD
  created_at
  initial_policy
```

The `group_id` is bound into the MLS group context but is not a relay URL, domain, or homeserver room ID.

## Human roster vs cryptographic clients

```text
Human roster                Cryptographic roster

Alice                       Alice / phone
Bob          ->             Alice / web
Carol                       Bob / phone
                            Carol / phone
                            Carol / tablet
```

Each MLS client credential binds to an OM DeviceIdentity and therefore to an AccountID.

Normal UI collapses devices under the person.

## OM authentication for MLS

MLS requires applications to authenticate client credentials. OM satisfies that through Gate-A identity objects, not one mandatory central authentication server.

```text
MLS client credential
 -> OM DeviceIdentity authorization
 -> AccountState
 -> AccountID
```

Directory providers may distribute KeyPackages, but they are not roots of trust.

## GroupState

Application state is distinct from raw MLS cryptographic state.

```text
GroupState
  group_id
  state_version
  previous_state_hash
  profile
    name
    avatar_ref
    description
  membership[]
    account_id
    role
  policy
    who_can_invite
    who_can_manage_members
    who_can_edit_group
    who_can_post
    join_approval
    new_device_policy
    history_policy
    bot_policy
  mls_epoch_ref
  authorization_evidence
```

Membership-changing state is bound atomically to the MLS epoch transition so the human roster and cryptographic roster cannot silently diverge.

## Roles

Keep v1 small:

- `admin`
- `member`
- `bot` / service principal where applicable

Do not require a unique permanent owner. Multiple admins avoid making the creator a permanent single point of failure.

Default:

- admins manage members, metadata, invite links, and admins;
- members use normal chat features;
- bots have explicit service semantics.

High-assurance groups may require multiple admin approvals for selected critical changes.

If the last admin is about to leave, the client warns and asks for another admin. The protocol should not invent a magical global owner.

## Creating a group

Normal UX:

```text
New group
 -> select people
 -> optional name/photo
 -> Create
```

Underneath:

1. generate GroupID;
2. initialize GroupState;
3. initialize MLS group;
4. fetch current KeyPackages for selected members' eligible devices;
5. add device clients;
6. distribute Welcome messages;
7. send encrypted group state/events through normal OM delivery.

## Adding a person

Adding an AccountID adds its eligible authorized devices.

Group policy decides whether any member may invite, only admins may invite, invite links are allowed, or admin approval is required.

## New device for an existing group member

Default low-friction behavior:

- AccountState authorizes the new DeviceIdentity.
- Existing group membership is at the account level.
- A group client may commit that device into MLS after validating its authorization.
- Human admin approval is not required merely because Alice replaced her phone.

High-assurance groups may set `new_device_policy = admin_approval`.

## Device revocation

```text
AccountState revokes Alice phone A1
 -> groups learn the revocation
 -> MLS Remove for A1
 -> epoch advances
 -> A1 cannot decrypt future group traffic
```

The revoked device may retain plaintext it legitimately received before removal.

## Removing a person

Removing Alice means:

1. application membership removes Alice AccountID;
2. all current Alice device leaves are removed from MLS;
3. group epoch advances;
4. future delivery to those device capabilities stops.

## Cryptography direction: MLS

Design around RFC 9420 and RFC 9750.

Reasons:

- standardized asynchronous group key agreement;
- forward secrecy;
- post-compromise security;
- efficient membership changes;
- explicit client/device model;
- designed for large groups;
- compatible with an untrusted/replaceable delivery service.

Exact library, cipher suite, KeyPackage policy, and implementation are Gate-C decisions.

OM may still choose a separate reviewed 1:1 protocol; the application event model must not depend on this choice.

## Group history

Default:

```text
history_policy = since_join
```

Optional later policies:

```text
since_join
recent_window
full_shared_history
```

Prior history is shared through an independently encrypted `HistoryBundle` created for the newcomer.

OM does not grant history by exporting old MLS epoch secrets.

This keeps live group cryptography separate from archive/history sharing.

## Invites

Invite links/QR codes are capabilities, not identity.

```text
GroupInvite
  group_id or opaque join reference
  invite capability
  expiry
  max_uses?
  join_policy
```

Invites can be rotated/revoked without changing GroupID.

A shareable link should support admin approval by default.

## Delivery

OM does not require a group homeserver.

### Reference privacy-first path

A sender produces one MLS application ciphertext and fans it out to each target device's opaque delivery capability, potentially across different relay providers.

Benefits:

- no mandatory group server;
- relay choice remains per user/device;
- no single provider must own the group;
- works with simple replaceable relays.

Tradeoff: operation count grows with destination devices.

### Optional optimization

A provider may advertise `group_broadcast`, accepting one ciphertext and distributing it to opaque registered recipients.

This is only an optimization. Provider capability metadata must disclose the additional metadata the provider can observe.

The group protocol must work without it.

## Attachments

```text
random content key
 -> encrypt attachment locally
 -> upload ciphertext to one or more BlobStores
 -> include locator(s), digest, size, MIME info, and decryption material inside the E2EE group event
 -> recipients download and optionally re-home according to their own storage policy
```

Ciphertext may be mirrored across providers.

Attachment storage is not group authority.

Exact minimum hot-retention defaults belong to Gate F.

## Group storage and backup

No mandatory shared group archive in v1.

Each user's backup policy may preserve the group history they possess.

Keep separate:

- live MLS/group state;
- message/history archive;
- device onboarding/history sync.

A new device rejoins current group state and obtains history separately. Avoid depending on long-lived backups of raw old MLS epoch secrets.

## Bots

Two modes:

### Command/service bot — preferred default

The bot is not automatically an MLS member. The invoking client sends only the command and selected context through a separate encrypted relationship.

### Full group-member bot

If a bot becomes an MLS client, users must assume it can access group plaintext available under keys it possesses. A UI-only "mentions only" promise is not a cryptographic boundary once the bot has group decryption state.

Full-member bots must be visibly labeled in roster and join/change events.

## Bridges

A bridge to an external network is an explicit gateway principal.

If it decrypts OM group traffic and re-encrypts elsewhere, OM-native E2EE ends at that gateway. The UI must say so.

Bridged groups never silently display the same security state as purely OM-native groups.

## Membership privacy

Group membership and metadata are sensitive.

OM should:

- encrypt control/handshake messages where feasible;
- avoid publishing human-readable rosters to relays;
- keep GroupState encrypted to the group;
- avoid a public global group directory in v1;
- treat relay metadata exposure as a provider capability/property.

MLS does not by itself hide all traffic timing, size, or membership metadata from the delivery topology.

## Group state concurrency

Avoid Matrix-scale state-resolution complexity in v1.

Reference direction:

- membership transitions bind to MLS epoch commits;
- admin mutations reference prior GroupState hash/version;
- conflicting offline updates are rejected/rebased deterministically rather than merged through a large distributed DAG;
- clients repair from the accepted state chain.

Fork/fragmentation tests are mandatory in Gate C.

## Large groups

Do not encode a small protocol limit.

Reference clients can begin with a conservative product cap while performance is tested. Public channels, feeds, and massive broadcast communities are separate semantics and remain deferred.

## Required adversarial scenarios

1. member adds a second device while half the group is offline;
2. compromised device is revoked;
3. member is removed while one device is offline;
4. two admins concurrently change membership;
5. invite link leaks;
6. bot joins and is later revoked;
7. bridge joins;
8. new member requests history;
9. malicious relay drops commits;
10. group spans multiple relay providers;
11. all admins disappear;
12. directory lies about a KeyPackage;
13. recovered account rejoins groups;
14. malicious member attempts state fragmentation.

## UX

Normal users see:

```text
Family
Alice
Bob
Carol
```

Not epochs, KeyPackages, ratchet trees, device leaves, relay capabilities, or signature chains.

Meaningful changes are phrased as human events:

```text
Alice added a new phone.
Bob left.
Carol removed Dave.
WeatherBot joined as a bot.
This group is now bridged to Example Network.
```

## References

- RFC 9420 — Messaging Layer Security Protocol
  https://www.rfc-editor.org/rfc/rfc9420.html
- RFC 9750 — Messaging Layer Security Architecture
  https://www.rfc-editor.org/rfc/rfc9750.html
- Signal group UX reference
  https://support.signal.org/hc/en-us/articles/360007319331-Group-chats
- Matrix event/state reference
  https://spec.matrix.org/latest/

## Exit criterion

Independent clients can create a group, add/remove accounts and devices, exchange encrypted events while offline/reordered, rotate epochs, revoke compromised devices, move participants across relay providers, and converge on the same human-visible roster/state using public specs and vectors.
