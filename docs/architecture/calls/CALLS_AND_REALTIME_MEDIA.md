# Voice and Video Calls / Realtime Media

## Goals

- simple “tap call” UX;
- route-independent E2EE;
- minimal infrastructure cost;
- low latency and good quality;
- privacy policy that can hide peer IPs when required;
- LAN/local resilience;
- scalable group calls;
- replaceable media providers;
- no server-side plaintext or mandatory recording capability.

## Layering

```text
OM relationship / group authentication
            |
      encrypted call signaling
            |
       call key agreement
            |
       SFrame media E2EE
            |
   WebRTC transport / SRTP
            |
LAN / direct P2P / TURN / SFU
```

SFrame (RFC 9605) is the preferred media E2EE framing because it remains independent of whether the media travels directly or through an SFU.

## Signaling

Call control travels as encrypted OM control traffic, not through a separate global call-identity service.

Logical ephemeral messages include:

```text
call.offer
call.answer
call.reject
call.end
call.ice_update
call.media_update
call.participant_update
call.key_epoch
```

They are not ordinary durable chat events unless a separate local “call history” record is desired. APNs/FCM/Web Push carries only opaque wake information.

### Who may ring (ADR-0128 rules 14-15)

- A `call.offer` from a principal without an accepted relationship is **dropped**. At most it is surfaced as a
  MessageRequest-class "missed call" notice: no ringing, no ICE and no TURN allocation (absolute rule 10;
  [`INVITES_INTROGATE_AND_SPAM.md`](../discovery/INVITES_INTROGATE_AND_SPAM.md) §5: no unsolicited call permission). Ringing costs the recipient TURN allocation,
  which is expensive work.
- Starting a group call that rings the Room needs `room.start_call` ([`om-room-capabilities.json`](../../../spec/registries/om-room-capabilities.json)). Joining an ongoing
  call needs only membership.

## Call identity and keys

Each call receives a random `CallSessionID` unrelated to AccountID or delivery capability. Media keys are fresh per call and authenticated by the existing OM relationship/group security context.

### 1:1

Use the established PQ/hybrid OM relationship to authenticate call-specific key establishment/export. The TURN server or direct network path is never a key authority.

### Group

Use MLS-authenticated participant/group state and derive/export SFrame keying according to the selected media/MLS design. Participant membership and key epoch changes remain authenticated even when an SFU routes media.

Call keys come from an MLS exporter whose context binds the CallID, the MLS epoch and the **authenticated, sorted set
of joined participant devices**. Every join or leave changes the set and therefore rekeys (ADR-0128 rule 14).

This binds keys to the participant list everyone sees, but it does not by itself keep non-joined members out: every
Room endpoint that holds the epoch, including members who never joined, full-room ServicePrincipals and plaintext
bridges, can compute the same exporter. The SFU is untrusted, so it could forward streams to such an endpoint, which
would then be a listener not shown in the participant list. Therefore:

- the call UI shows every full-room service or bridge in the Room that could derive call keys;
- the property "a non-joined member cannot decrypt" is **not claimed** until design queue item A-035 selects an
  isolation mechanism (for example an RFC 9420 subgroup branch holding only joined devices, or per-sender SFrame keys
  sent only to joined participants). A-035 blocks the `calls` scope; V-007 tests the result.

## RoutePolicy

Users do not choose network mechanics. A `RoutePolicy` selects the cheapest/safest viable path under the user's privacy policy.

### Default for accepted/trusted contacts

```text
1. same-LAN direct path when safely discoverable
2. direct Internet P2P using ICE/STUN
3. TURN relay when direct fails
4. alternative TURN provider when primary unavailable/budget-limited
```

### Strict privacy / unknown peer

For RoutePolicy, an **unknown peer** is any peer that is not an accepted contact, **including Room co-members** in a
group call (ADR-0128 rule 15). (A caller without an accepted relationship cannot ring at all; see "Who may ring".)

Do not expose direct candidates that reveal a peer's IP to an unknown peer or under strict privacy. Use TURN-only or equivalent privacy relay until policy permits otherwise. A call may begin with privacy-preserving candidates and add direct candidates only after acceptance if allowed.

### Group calls

- very small groups may use mesh P2P when uplink/CPU/battery/network conditions are good; toward any participant who is not an accepted contact, a mesh uses relay-only candidates, so co-members who are strangers never learn each other's IP addresses;
- switch/prefer E2EE SFU as participant count or resource cost grows;
- topology selection is automatic;
- changing topology does not change SFrame content confidentiality.

## LAN/local-first calls

If two established contacts are on the same local network, media should be able to remain local. Future local discovery should avoid broadcasting stable AccountIDs or human-readable device names. A preferred direction is rotating rendezvous tokens derived from existing relationship secrets and time epochs.

Long-term resilience goal: where local connectivity exists but WAN access is unavailable, established peers on the same LAN should be able to exchange messages/files and conduct calls without cloud media transit. Full offline rendezvous details remain an implementation gate.

## Mid-call migration

Calls must survive:

- Wi-Fi -> cellular change;
- IP address change;
- failed direct path -> TURN;
- TURN provider failure -> alternate provider;
- group mesh -> SFU transition where implementation permits.

Use ICE restart/re-negotiation and preserve `CallSessionID`, participant authentication, and SFrame security state or perform authenticated rekey. A route transition must not create a plaintext interval.

## MediaProvider interface

Conceptual interface:

```text
MediaProvider.capabilities()
MediaProvider.issue_turn_credentials(scope, ttl)
MediaProvider.create_sfu_session(call_ref, policy)
MediaProvider.join_sfu_session(session, scoped_token)
MediaProvider.usage_budget_state()
MediaProvider.health()
```

Credentials are short-lived and scoped. A media provider can allocate/deny resources but cannot become an OM identity endpoint.

## Cloudflare reference media provider

Cloudflare Realtime provides STUN, TURN, and SFU services suitable for a frictionless reference deployment. As of 2026-09-21, Cloudflare documents a shared 1,000 GB/month free Realtime egress allowance for SFU/TURN and $0.05/GB beyond it; STUN is documented as free/unlimited.

Therefore:

- direct LAN/P2P should be used when policy permits, saving infrastructure bandwidth;
- OM deployments should implement budget alerts and hard routing policy before paid overage;
- do not assume the service will automatically stop exactly at the free allowance;
- MediaProvider failover prevents Cloudflare from becoming indispensable.

## Recording

Baseline OM infrastructure has no server-side recording feature. Recording, if implemented later, is an endpoint action and must be conspicuous to participants. An SFU is a router, not a hidden participant.

## Screen sharing / future media

Screen share is another SFrame-protected media track/capability. The same participant, route, and permission model applies.

## Required tests

- SFrame ciphertext remains unreadable to TURN/SFU;
- direct <-> TURN path transition preserves E2EE;
- strict privacy mode never exposes host/server-reflexive candidates contrary to policy;
- LAN direct path works without cloud media transit;
- unknown caller cannot force direct-IP disclosure, ringing, ICE or TURN allocation;
- mesh group calls use relay-only candidates toward co-members who are not accepted contacts;
- every join/leave rekeys the group call; full-room services and bridges able to derive call keys are shown;
- a non-joined Room member cannot decrypt the call (requires A-035);
- a member without `room.start_call` cannot ring the Room;
- call survives Wi-Fi/cellular migration;
- group membership removal rotates media access;
- SFU cannot add itself as participant;
- provider failure can fall back to alternative route/provider;
- configured zero-cost policy never spills into billable media usage.

## 2026-09-23 media-key resolution

1:1 call media secrets derive from the existing secure session with CallID/participant-bound exporter labels. Group call keys derive through the MLS exporter bound to CallID, MLS epoch and (since ADR-0128) the sorted joined-participant set, with directional/per-sender SFrame keys and no counter restart under the same key. Interoperability is verified by `V-007`.

## Participant-isolated group media keys (ADR-0132)

Room MLS authenticates call-control context but is **not** the media-key source. Every sending DeviceID generates a
fresh SFrame sender key for each participant-set generation and sends that key only to currently joined participant
DeviceIDs over authenticated OM pairwise device channels (or an equivalent already-authenticated pairwise HPKE
channel). The key package binds CallID, ConversationID, MLS epoch, participant-set generation, the sorted participant
DeviceID set, sender DeviceID and SFrame key id.

Join, leave, revocation/quarantine or participant correction increments the generation and requires fresh sender keys
before further media. The SFU never receives media keys. A Room member/service/bridge that is not joined cannot
decrypt merely by possessing the Room MLS epoch. Joined services/bridges are visible call participants. V-007 must
exercise silent-listener, stale-generation, leave/rejoin and revocation traces.
