# Reference Home: what a malicious operator can and cannot do

- **Date:** 2026-10-28
- **Scope:** `provider_reference` (ADR-0171). Subject: the Cloudflare reference Home in `infra/cloudflare-home`, used through `packages/om-provider-http` by the real Core.
- **Question:** if the party running the Home is hostile, or the Home is breached, what do they learn and what can they break?
- **Basis:** the code and tests in this repository, the Home API ([`home-api.md`](../../spec/protocol/home-api.md)), [`SEALED_SENDER_DELIVERY_PROFILE.md`](../architecture/delivery/SEALED_SENDER_DELIVERY_PROFILE.md) section 13 and [`METADATA_OBSERVER_MATRIX.md`](../architecture/providers/METADATA_OBSERVER_MATRIX.md). "Operator" means whoever controls the Worker and its Durable Object storage. A user who deploys their own Home is their own operator; a sponsor, a T1 host or a community relay is a third party.

An operator can change the deployed code, so a conforming Worker's behaviour (no logging, no address reads) is a property of the **verified release** (R-261), not of the protocol. This note therefore separates what the protocol and Core make impossible even for modified code from what only the honest reference code declines to do.

## 1. What the operator can see

| Observation | Source | Notes |
|---|---|---|
| Ciphertext of every deposit, padded to one of five buckets (1024 / 4096 / 16384 / 65536 / 131072 plaintext bytes) | `SealedDeliveryEnvelope` | Size reveals only the bucket. |
| Outer fields: crypto profile, opaque `delivery_key_id`, `delivery_id`, `hpke_enc`, authenticated `expires_at` | the wrapper | None identifies a person. |
| The deposit, receive and admin capabilities in clear, as presented in each request | request bodies / `Authorization` | TLS terminates at the operator. They are bearer secrets for **this** Home only. |
| Queue ids and the mailbox id inside the deposit capability | `queueId \|\| mailbox` | Random per relationship; not an AccountID (absolute rule 2). |
| Network address and timing of every request | transport | The reference code reads and stores neither (`rules.test.ts` fails the build if source touches an address header), but modified code could log them. Sealed delivery is not anonymity (profile section 13). |
| Volume, burst pattern, which queues are active, when a queue drains | behaviour | Enough for traffic analysis. |
| That a `MessageRequest` (first contact) arrived at a queue, and its `ticket_id` / invite id | object type 19 | Visible by design: IntroGate admission. |
| Device ids, signed bundles and one-time prekeys; which addresses fetch them and when | `KeyPool` | Public key material by design. `provider_pool_id` is visible. |
| The full invite descriptor and, in it, the **inviter's mailbox, display label, ACS body and credentials** | `put_invite` | **Core pass-1 shape, not a Home choice.** The descriptor is stored in clear under an unguessable id, so an intro-service operator links an invite id, a display name and a mailbox. See section 4, item 1. |

## 2. What the operator cannot see

- Any plaintext, attachment, or conversation structure: the inner envelope is HPKE-sealed to a key only the recipient holds.
- Sender AccountID, DeviceID, identity key, Double Ratchet header or ConversationID for established 1:1 traffic: they are absent from the outer object, and the Home rejects a wrapper that carries any extra field (`wire.test.ts`, "no sender field"). The Core test `sealed_delivery` and the end-to-end test "what the Home receives" check the same on real traffic.
- The recipient's AccountID: queues are addressed by random mailbox capabilities, and the adapter derives every secret capability from a per-install secret, so two queues of one person share nothing the Home can see except network origin.
- Who is on the other end of a relationship, whether two queues belong together, or whether a queue is a reply route: no relationship id exists on the wire.
- Message content from the one-time prekey or descriptor stores: they hold public material only.

## 3. What the operator can do

| Action | Effect | Detectable / bounded by |
|---|---|---|
| Drop, delay, reorder or duplicate deposits; refuse service to chosen queues | Availability loss, selective denial. | Duplicates are absorbed by Core (cursor plus EventID dedupe). A silent drop is **not** visible to the sender: there is no relay receipt that proves delivery, only E2E receipts. Multi-relay delivery (RelaySet, not built) is the answer. |
| Drain, ack or delete a queue (it holds the receive capability whenever the owner drains) | Message loss. It reads only ciphertext it cannot open. | Same as above. |
| Deposit junk wrappers (it knows deposit capabilities) | Fills the queue (`quota`), burns recipient CPU on failed HPKE opens. | Per-capability rate and queue quota limit the volume; Core rejects what does not open. |
| Withhold, replay or roll back a key bundle; serve an old generation | Blocks or delays first contact. | Clients verify signature and ACS binding and keep a durable generation floor (PC-K05); the Home's own generation check is an aid only. |
| Drain the one-time prekey pool | Handshakes fall back to signed prekeys (visible as inventory state). | Fetch is throttled (8 burst, 1 per 10 minutes) so a stranger drains slowly; the operator itself can still empty the pool. |
| Serve `not_found`, a stale or another invite's descriptor | Blocks or misdirects an invitation. | The invite link commits to the invite id, the ACS head hash and the request-key digest, so a substituted descriptor fails verification. |
| Squat a mailbox or device id it knows before the owner first uses it | Owner cannot claim the queue or publish keys. | The owner's adapter reports `lostMailboxes()`; not yet surfaced in the UI. |
| Refuse to forward or revoke | A revoked invite or capability may keep working at a hostile Home. | Revocation is only as good as the Home; the spec states no instant global revocation. |
| Correlate queues and devices by network address and timing | Partial relationship graph. | Use different Homes per relationship and a privacy transport; both are policy, not built. |

## 4. Where this Home or Core falls short of the spec's privacy goals (recorded, not hidden)

1. **Invite descriptors expose a mailbox and a display label to the intro service.** Core publishes `{ACS body, credentials, request key, mailbox, label, chain}` unencrypted under the invite id. The spec's public descriptor carries only safe introduction hints and no stable relationship mailbox. This needs the first-contact work (H2): a signed `ContactDescriptor` with intro services, and a reply route carried inside the sealed `MessageRequest` only.
2. **The first-contact reply route is a reusable write capability into a private mailbox.** Because the descriptor names the inviter's mailbox, anyone who reads the invite can deposit there until it is rotated. The Home limits this with per-capability rate and queue quota, but cannot fix it; the IntroGate endpoints (tickets, `spent_token`, payload limit) exist and are tested, and Core does not call them yet.
3. **One-time prekeys are handed to anonymous callers.** Core has no relationship fetch capability, so the Home accepts a placeholder and applies the throttle instead of the spec's "anonymous fetch returns request-key material only". A relationship capability needs a Core change.
4. **First poll or first publish claims a queue or device id (trust on first use).** Core has no provider-administration concept, so the adapter derives a capability from its install secret and the Home binds it on first use. A peer who learns a mailbox before its owner polls could claim it. The window is the gap between creating a mailbox and the owner's first drain pass.
5. **The route binding still uses `om-core:mailbox/v0`.** The relay is indifferent (it never sees a binding), but until Core adopts `RelayEntry` and `RelaySet`, keys are bound to the stand-in route. `packages/om-home-wire` implements the spec's `RelayEntry` and `route_binding` with independently computed vectors so Core can adopt them.
6. **No cover traffic, no padding of timing, no transport privacy.** Out of scope for this role.

## 5. Abuse and cost controls an honest operator keeps, and what each costs in privacy

| Control | How | Privacy cost |
|---|---|---|
| Per-capability deposit rate | token bucket per deposit capability, in the queue's Durable Object | None: keyed by capability, not address. |
| Queue items and bytes, retention, replay set | typed `quota` / `expired` | None. |
| Ticket uses, payload limit, per-service request rate | IntroGate | None. |
| One-time prekey fetch throttle | per device | None. |
| Flood protection against unknown or forged capabilities | **not in the Worker**: it would need a network address. Use the edge: Cloudflare WAF rate-limit rules or Workers Free hard limits. | Operator-chosen; the address is then visible to Cloudflare, which already terminates the connection. |

Free plan: a Durable Object limit that is exceeded fails further operations of that type (`OM_HARD_FREE_TIER=1` marks `quota` as a hard stop). A flood of forged capabilities creates empty Durable Objects and spends request budget; the typed failure is an availability loss, never a data loss.

## 6. Residual risks outside this component

Endpoint compromise, a malicious or compromised release of the Worker (R-261 verified-release path), Cloudflare itself as a global observer, and legal compulsion are covered by [`METADATA_OBSERVER_MATRIX.md`](../architecture/providers/METADATA_OBSERVER_MATRIX.md), [`SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md`](../architecture/providers/SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md) and the T1 hosted-Home analysis, not here.
