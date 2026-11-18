# ADR-0221 — Own-Home media leases and local call continuity

## Design sources read

The calls build plan, ADR-0043, ADR-0165, ADR-0183, ADR-0185, ADR-0210,
ADR-0217 through ADR-0220, [CALLS_AND_REALTIME_MEDIA.md](../docs/architecture/calls/CALLS_AND_REALTIME_MEDIA.md),
[LIVE_CALLS_UX.md](../docs/ux-design/specs/LIVE_CALLS_UX.md),
[SECURITY_PRIVACY_PROFILES.md](../docs/architecture/ux/SECURITY_PRIVACY_PROFILES.md),
[HOME_WAKE_CHANNEL.md](../docs/architecture/providers/HOME_WAKE_CHANNEL.md),
[NOTIFICATIONS_AND_PUSH.md](../docs/architecture/client/NOTIFICATIONS_AND_PUSH.md),
[TIME_AND_EXPIRY_SEMANTICS.md](../docs/architecture/events/TIME_AND_EXPIRY_SEMANTICS.md)
and [CLOUDFLARE_REFERENCE_DEPLOYMENT.md](../docs/architecture/providers/CLOUDFLARE_REFERENCE_DEPLOYMENT.md).
The V-007 gate and metadata trace plan were read for evidence. The broader mapped design set and non-call gates were consulted selectively.

Accepted by delegation for experimental engineering under ADR-0217. The operator's scope for own-Home STUN and TURN is ADR-0218 rule 7.
The Home API binding is an additive noncritical extension under ADR-0165. This decision adds no CallSignal field, public-release capability or security claim.

## Public Home binding

The optional feature `om.media.turn.v1` enables an owner to request ICE resources from a configured personal Home. It adds
`media: { "stun_urls": [...] }` to ProviderCapabilities. An empty list is valid. A Home with the feature disabled reserves
no media resources. The reference Homes require a configured owner admission, a personal-profile usage meter, hard-free-tier operation, valid STUN
configuration and a bound, budget-enforcing media backend before advertising. No server addresses or account are built into the client.

| Operation | Binding | Authorization | Request | Response |
|---|---|---|---|---|
| Reserve a short-lived TURN lease | `POST /v1/media/turn` | Existing `OM-Admission` for this Home, over the Home API's authenticated encrypted transport | `MediaTurnRequest` in canonical CBOR | Private `MediaTurnLease` in canonical CBOR |

[media-turn.cddl](../spec/cddl/media-turn.cddl) defines the two optional bodies. The request contains the API major and a random 16-byte request ID.
The bound is eight STUN or TURN URLs, each at most 256 characters. A lease contains at least one TURN URL.
The Home and client reject expired leases and leases valid for more than 600 seconds at receipt. The expiry is a media-provider
retention deadline in POSIX seconds. The client also treats its wall time as a conservative local check. Clock disagreement can block
a route and grants no protocol authority. The lease is held in memory for its own peer connection attempt.
A later call or a new TURN gathering operation obtains its own lease. An unknown result does not automatically retry with a new
request ID. An exact authenticated retry while the Home can issue leases receives the existing reservation or `expired`;
an expired ID must never create a fresh reservation.

Responses are no-store. The client omits cookies and referrers and rejects redirects. The owner request carries no CallSessionID,
AccountID, Room, destination, SDP or SFrame material. The media path adds an issuance operation visible to the person's own Home.
It adds no routing field or discriminator to signalling deposits, drain or wake. A direct call without the optional feature
uses the existing queue and wake operations. A relay can still observe their existing timing, expiry and size bucket.

## Media budget and deployment

The Cloudflare Home takes a private `MEDIA_TURN` service binding. The SQLite Home takes a permission-restricted Unix socket
configured by absolute path in `OM_MEDIA_SOCKET`. Both pass only `POST /lease` with JSON `{"request_id":"<lowercase hex>"}` to the owner's backend.
A successful backend reply is JSON with `urls`, `username`, `credential` and `expires_at`.
`OM_MEDIA_STUN` optionally lists owner-selected STUN URLs, comma-separated. The public release configurations leave the binding
unset. Creating a media lease when the Home's usage state is hard, missing or unknown returns a hard `quota` failure.

The backend must durably reserve against the Home's media byte budget, refuse at the ceiling, issue unique credentials scoped to
the reserved allocation, and enforce expiry and the byte limit in the TURN data plane, including after restart. It returns
HTTP 429 for budget refusal and HTTP 410 for an expired reservation. The backend must refuse new allocations and refreshes once
their reservation is spent or expired and close the allocation at that limit. An issuance-count limit or a billing dashboard alone
cannot provide this guarantee. Compatibility with any particular backend needs evidence from the running data plane.
The Home implementations include the binding, validation and usage gate. A production byte-limiting TURN issuer is external to them.

## Endpoint behavior

Recommended gathers LAN and direct candidates, with STUN when advertised. When the direct check has failed or an initial call
has spent seven seconds connecting after key handoff, the original caller may add its Home's TURN lease and use the existing
authenticated ICE restart on the same call. The callee can add a lease from its own Home when responding to a relayed restart.
Subsequent recovery retains the route floor and the established SFrame keys. Configured non-draining own Homes are tried in
preference order when a lease cannot be obtained.

Hardened, Maximum Privacy and Custom without a typed routing decision require relay-only ICE in the web host.
The host creates the peer connection with the browser's relay-only candidate policy and rejects a local description with no
relay candidates or an unexpected candidate type. Related addresses are removed before signalling. The host refuses capture
and ICE when a private lease is unavailable. A stricter profile closes a direct-capable transport already in use.
Offers must pass Core admission and an explicit Answer before the recipient requests a lease or starts capture or ICE.
One-to-one calls remain limited to established contacts. Non-contact mesh routing remains in the group-call work.

Missed-call notices live in the encrypted device vault under the existing bound of 64. They are excluded from sync and history
export. Reopening restores notices and never restores a live call, signalling description or media key. An authenticated late
envelope can leave a notice if still delivered to the receiver. There is no source for a notice once the Home has purged the
envelope. A hidden tab uses the slow poll or opaque push nudge. The wake socket remains open only on a visible page under ADR-0185.

## Where did authority go?

Core owns call admission and call state. The person's policy sets the route floor. The configured backend can reserve or
deny relay resources and holds no call authority or media key.
