# Media (TURN) adapter contract

Status: engineering reference for the optional Home feature `om.media.turn.v1`. It restates what
[ADR-0221](../decisions/ADR-0221-own-home-media-leases-and-call-continuity.md) and
[ADR-0228](../decisions/ADR-0228-calls-per-device-media-secrets-ordered-signalling-answer-retry-watchdog-and-turn-lease-lookup.md)
section 4 decide; it adds no behaviour. If this file and an ADR disagree, the ADR wins.

A Home that advertises the feature reserves short-lived TURN credentials for its owner by asking a private
media backend. The Cloudflare Home and the SQLite Home share the same Worker code for this path; they differ only
in how the backend is reached. This file is what a backend author or a new Home adapter must satisfy.

## Parties and where authority sits

| Party | Role | Holds |
|---|---|---|
| Client (owner's device) | Asks its own Home for a lease, one `request_id` per lease attempt | The lease, in memory only, for one peer connection attempt |
| Home | Authenticates the owner, gates the request on its budget, remembers recent attempts, validates and relays the backend's answer | A table of at most 64 recent `request_id` outcomes for 600 seconds plus the envelope skew |
| Media backend | Reserves relay capacity against a byte budget and mints credentials | The reservation. It holds no call identifier, account, destination, SDP or media key and never receives any |

The Home never sees a call identifier, and the backend never sees the owner's admission value.

## Advertising

The Home advertises `om.media.turn.v1` and a `media` block in its provider capabilities only when all of these hold:
a media backend is bound, the hard free-tier mode is on, the usage meter is on, and an owner admission is configured.
`OM_MEDIA_STUN` optionally lists owner-chosen STUN URLs (comma separated, up to eight). With any condition missing the
operation answers `not_found`, exactly as an unknown path would.

## Binding the backend

| Home | How the backend is reached | Configuration |
|---|---|---|
| Cloudflare Home | A private service binding named `MEDIA_TURN`; one internal `fetch` per lease | The binding is left unset in public release configurations |
| SQLite Home | A permission-restricted Unix domain socket | `OM_MEDIA_SOCKET`, an absolute path. A relative path refuses to start |

Both adapters send the same single request and read the same single reply. No DNS lookup or public fetch is involved.

## Backend request

`POST /lease`, `content-type: application/json`, body exactly `{"request_id":"<32 lowercase hex characters>"}`.
No other field, no query string, no cookie, no admission or authorization header.

## Backend reply and how the Home maps it

The Home waits at most 5 seconds, follows no redirect and reads at most 4096 bytes of reply.

| Backend answers | Client sees | Remembered by the Home |
|---|---|---|
| 200 and a valid lease document (below) | The lease as private canonical CBOR (`media-turn-lease`), `cache-control: no-store` | Outcome `issued`, with the expiry |
| 429 | `quota` (hard) | Outcome `quota` |
| 410 | `expired` | Outcome `expired` |
| Any other status, including every 3xx, 4xx and 5xx | `retryable` (HTTP 503) | Nothing but the pending attempt |
| 200 with a body that is not JSON, is empty, or is larger than 4096 bytes | `retryable` | Nothing but the pending attempt |
| 200 with a lease document that fails validation | `retryable` | Nothing but the pending attempt |
| No answer within 5 seconds, or a transport error | `retryable` | Nothing but the pending attempt |

A failure that is not a verdict is never turned into a refusal: the owner's exact retry (same `request_id`) reaches the
backend again.

### Lease document

```json
{ "urls": ["turn:host:3478?transport=udp"], "username": "...", "credential": "...", "expires_at": 1790000000 }
```

- `urls`: 1 to 8 entries, each at most 256 characters, each a `turn:` or `turns:` URI with an optional port and an
  optional `?transport=udp|tcp`. No other scheme.
- `username` and `credential`: 1 to 256 printable ASCII characters, no spaces.
- `expires_at`: POSIX seconds, in the future and at most 600 seconds ahead of the Home's clock.
- Unknown extra fields are ignored and never forwarded.

## What the backend must guarantee

These are the behaviours the Home relies on; the Home cannot verify them.

1. **Idempotent by `request_id`.** The same id returns the same reservation while it is live, and never a second one.
2. **Durable budget.** A reservation is counted against the media byte budget before it is returned, the budget is
   refused at its ceiling (429), and a reservation made before a restart still counts after it.
3. **Expiry is final.** An expired id answers 410 and never creates a fresh reservation.
4. **Scoped credentials.** Each reservation gets credentials valid only for that allocation, unique per reservation.
5. **Data-plane enforcement.** The relay refuses new allocations and refreshes once the reservation is spent or expired and
   closes the allocation at the byte limit, including after a restart. An issuance count or a billing dashboard alone does
   not meet this. Evidence for it comes from the running data plane of the backend that claims it, not from a Home test.

## What the Home guarantees to the client

1. One reservation per lease attempt: a repeated `request_id` is answered from the Home's table without asking the budget
   gate again, so a reservation made before the usage meter turned hard stays retrievable and costs nothing more.
2. A `request_id` that is not in the table is gated by the usage meter first. With the meter hard, missing or unknown the
   answer is `quota` and the backend is not asked.
3. The table holds at most 64 attempts, oldest first out, and survives a restart of the Home.
4. The same `request_id` with a different body shape is `invalid_request`. The body is exactly one 16-byte id.
5. The reply is private (`no-store`) and the request is refused without the owner's admission value.

## Parity between the two Homes

Both Homes run the same cases. The suite is `infra/cloudflare-home/test/media.test.ts`, and it is part of the shared
provider conformance run, so a difference between the two adapters fails it:

```sh
pnpm conformance:provider worker sqlite sqlite-process
```

The `sqlite-process` target runs the SQLite Home as its own operating-system process and reaches the backend over a real
Unix socket, which is the path a deployment uses. The cases cover the request the backend receives, every row of the
mapping table above, the five-second limit, the exact-retry behaviour and the 64-attempt bound.

What these tests do not establish: that any particular production backend enforces its byte and expiry limits in its data
plane. There is no production backend in the repository.
