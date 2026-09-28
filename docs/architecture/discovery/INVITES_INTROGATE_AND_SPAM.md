# OM Invitations, IntroGate, Revocation, and First-Contact Abuse Control

- **Status:** Canonical architecture checkpoint
- **Checkpoint:** 2026-09-21 10:08 PHT

## 1. Principle

A public way to find a person must not become a permanent reusable write capability into that person's private mailbox.

OM therefore separates:

```text
DISCOVERY -> INTRODUCTION PERMISSION -> MESSAGE REQUEST -> ACCEPTED RELATIONSHIP
```

## 2. Invitation object

An invitation is a scoped rendezvous capability, not an identity and not a normal message mailbox.

Logical fields:

```text
Invite
  invite_id           random
  secret/capability   high entropy; never guessable
  intro_services[]
  issued_at
  expires_at
  max_uses?           optional
  scopes              e.g. request-only, LAN-rendezvous
  revocation_epoch?   optional
  authentication
```

IntroGate stores only what it needs to validate the invite, preferably a one-way representation of the secret where possible.

## 3. Revocation

The user can revoke a link/QR independently of all accepted relationships.

```text
Invite A -> REVOKED
Invite B -> ACTIVE
existing Alice/Bob relationship -> unchanged
```

The client should make revocation a normal UX operation:

- `Revoke link`
- `Create new link`
- optionally `Expire after one use`

**`max_uses` is per IntroGate.** An invite that lists several IntroGates carries a distinct capability for each, and
no two independent gates share an atomic counter (the same problem as one-time prekeys, [`ASYNC_KEY_MATERIAL.md`](../crypto/ASYNC_KEY_MATERIAL.md) §4).
So when an invite lists more than one gate, the recipient's client either splits the use budget disjointly across the
gates, or, on the first accepted request, revokes the invite's capabilities at every sibling gate. The UI then shows
"one use per server" rather than promising a single global use.

A revocable invite requires a revocation point such as an IntroGate/IntroductionService. A completely self-contained offline invite cannot know that it was revoked while disconnected, so offline invites should use short expiry and explicit semantics.

## 4. Public directory entries do not publish reusable invite secrets

A public directory entry—especially for a public figure—should publish an `IntroductionServiceDescriptor`, not a static mailbox capability.

```text
Directory
   ↓
public IntroductionServiceDescriptor
   ↓
request IntroductionTicket
   ↓
short-lived/single-use ticket
   ↓
submit constrained MessageRequest
```

If the directory record leaks, that is not equivalent to leaking a private mailbox secret; the record was intended to be public.

## 5. IntroductionTicket

An `IntroductionTicket` is intentionally narrow:

```text
recipient/service binding
single-use or tiny use-count
short expiry
small payload limit
no arbitrary attachment upload
no unsolicited call permission
anti-replay nonce
optional anonymous anti-abuse entitlement
```

"No unsolicited call permission" is defined in [`CALLS_AND_REALTIME_MEDIA.md`](../calls/CALLS_AND_REALTIME_MEDIA.md) "Who may ring": a caller without an
accepted relationship causes no ringing, ICE or TURN allocation (ADR-0128 rules 14-15).

The ticket issuer may be the recipient's chosen IntroGate or a delegated anti-abuse service.

### 5.1 MessageRequest cryptography (ADR-0126)

W0 first contact uses three objects ([`discovery.cddl`](../../../spec/cddl/discovery.cddl)):

1. **Request key.** The recipient publishes a `request_key` (a public key of the hybrid KEM of the active
   CryptoProfile) in its ContactDescriptor, signed by the alias-specific intro identity, and may repeat it in an
   IntroductionTicket that the intro identity also signs. A sender uses a request key only from such a signed object.
2. **`MessageRequest`.** The sender seals a signed `MessageRequestPayload` using the exact PQ/T HPKE construction
   pinned by ADR-0131: RFC 9180 base-mode semantics with `draft-ietf-hpke-pq-05` MLKEM768-X25519, HKDF-SHA256 and
   ChaCha20Poly1305. The draft version is part of the profile; OM defines no ad-hoc hybrid KEM.
   The payload is signed by the **sender's** intro identity and carries the sender's signed ContactDescriptor, an
   optional greeting, optional referral attestations, a one-logical-response reply route with bounded transport use, and a required request-scoped
   `reply_delivery_key` bound to that route (ADR-0142/0144/0146/0147). It repeats the ticket id and request-key digest, so it cannot
   be replayed into another request. The requester generates a fresh reply key pair and `delivery_key_id` for every
   MessageRequest and durably binds them to `(ticket_id, request_key_digest, crypto_profile, reply_route)`. The key's
   profile equals the MessageRequest profile, its route-binding digest must match the exact reply route, and its expiry
   cannot exceed the route expiry. The bootstrap acceptance deadline is the authenticated minimum of the reply-route and reply-key expiries; v1 has no separate hidden request-expiry value. The same key/id is not reused across requests or promoted into established state.
   The reply delivery key is not a stable identity credential; it exists solely so the first post-acceptance response
   can be sealed before established relationship delivery keys have been exchanged.
   - **Sender hiding.** The outer object carries only the ticket id, request-key digest, profile, HPKE encapsulation,
     ciphertext and an optional admission proof. The IntroGate sees the ticket and admission token, the size, the
     timing and the network address; never the sender's AccountID, device id or intro key. It MUST NOT require any.
   - **Size.** The IntroGate rejects a ciphertext longer than the ticket's `payload_limit_bytes` plus the AEAD tag,
     before any other work.
3. **`IntroIdentityBinding`.** After the recipient accepts, one of its devices that the recipient's
   AccountControlState lists as active and not quarantined signs the recipient's AccountID, the intro public key, the
   request-key digest and the ACS generation and hash. It is sent only inside the accepted relationship. The sender
   verifies it against the recipient's ACS (device active, credential matching ACS `device_credentials`, intro key
   and request key matching what it used) before treating the relationship as bound to that AccountID.

Then both sides establish PQXDH sessions to each other's device bundles (ADR-0118, ADR-0120), fetched with the
per-relationship capability of [`ASYNC_KEY_MATERIAL.md`](../crypto/ASYNC_KEY_MATERIAL.md) §2.1, and exchange private relationship capabilities (§9).
V-015 and V-016 carry the vectors.

## 6. Adaptive abuse controls

The IntroGate can progressively increase admission cost under attack without weakening E2EE:

1. cheap validation and replay rejection;
2. endpoint/ticket/source-rate limits;
3. per-recipient quotas;
4. short ticket expiry;
5. privacy-preserving entitlement tokens;
6. lightweight proof-of-work under automated attack;
7. CAPTCHA/device attestation only as a last-resort policy option;
8. contacts/referral-only policy for high-risk recipients.

**Referral capacity is issued by the recipient (R-195, ADR-0126).** R-195's reserved queue admits two kinds of
tickets: invite-bound tickets and **referral tokens**. Alice's client pre-issues referral tokens to her own contacts
(Privacy Pass-style, RFC 9576-9578, bound to her IntroGate). Carol attaches one to Bob's request as the
`admission_proof`. The IntroGate verifies only the token, never Carol's identity, so it learns nothing about Alice's
contacts. "Introduced by Carol" is a separate label that Alice's client verifies from the ReferralAttestation inside
the sealed payload, and shows only when Carol is Alice's contact. A request without a valid token uses the public
queue. A-032 was resolved by ADR-0132: W0 uses the RFC 9578 publicly verifiable Blind-RSA referral-token profile.

Normal users should never see these mechanisms during ordinary traffic.

## 7. Privacy Pass direction

W0 uses the recipient-issued RFC 9578 publicly verifiable Blind-RSA referral-token profile to prove limited referral admission without a permanent global user identifier or reputation score. The cryptographic property and its metadata limits are defined by ADR-0132/ADR-0137.

The generic `AdmissionProof` interface remains extensible, but W0 standardizes this profile so independent clients and IntroGates interoperate. Other admission mechanisms remain optional and explicitly profiled.

## 8. DDoS responsibility

The system has three separate protection surfaces:

- `DirectoryProvider`: protects search/lookup infrastructure;
- `IntroGate`: protects a recipient from unsolicited first contact;
- `Relay`: protects normal delivery queues/resources.

Each provider can use its own operational defenses such as anycast/CDN/WAF, caching, quotas, admission control, or upstream DDoS filtering without gaining plaintext or identity authority.

## 9. Accepted relationships

After acceptance, peers exchange fresh private relationship capabilities. These are:

- separate from the public invitation;
- separate per peer/relationship;
- rotatable;
- revocable;
- potentially distinct per relay;
- not published in public directories.

## 10. Failure and privacy rules

- Reusing an expired/revoked ticket MUST fail closed.
- A sender MUST NOT be silently promoted from MessageRequest privileges to normal relationship privileges.
- An IntroGate MUST NOT require plaintext conversation access after acceptance.
- Abuse controls MUST be enforceable server-side because hostile clients can remove local limits.

## 2026-09-23 invite resolution

IntroductionTicket/invite capabilities are random, scoped, authenticated, replay-bounded and provider-scoped for single-use consumption by default. Multi-provider invitations carry distinct capabilities. Revocation is distributed to relevant IntroGates; OM does not claim impossible instantaneous revocation at disconnected providers. ZK/Privacy-Pass-style admission remains optional extension work.

## 11. 2026-09-24 resource model result (V-016)

[`verification/bounded/introgate_sybil_model.py`](../../../verification/bounded/introgate_sybil_model.py) simulates Sybil floods (up to 500 fresh identities per tick from
1-1000 sources) against the controls in §6:

- The recipient's expensive work stays at or below the per-recipient quota at every attack level. Identity is cheap,
  but opening a request is not.
- **Invite-bound tickets keep their reserved capacity**, so people holding a personal invite always get through
  (100% in every run).
- **Public, no-invite first contact is best-effort under attack.** It falls to about 1% under a large multi-source
  flood. PoW helps only while the attacker's compute per tick stays below the recipient quota.

Normative consequences:

1. Every IntroGate reserves capacity for invite-bound and referral tickets that the public queue cannot consume (R-195).
2. Under pressure, the client tells the recipient "you're receiving a lot of requests from strangers; people you
   invited can still reach you". It never shows a silent empty inbox.
3. Onboarding and discovery steer first contact through invites, QR codes and contact sharing. The public handle is
   a convenience, not a guaranteed channel.

### Symmetric stable-identity binding

Acceptance does not immediately create a stable-account relationship. The recipient first sends its
`IntroIdentityBinding`; after verifying it, the requester sends an `IntroIdentityBinding` for the intro identity used
in the original request. The relationship is `ACCEPTED_UNBOUND` until both verify against active, non-quarantined
DeviceCredentials and their referenced AccountControlState heads. Only then does it become `ACTIVE`, expose
relationship-scoped key-material fetch authority, exchange per-device/per-route `SealedDeliveryKeyRecord`s, and
establish normal PQXDH sessions to stable-account device bundles. Exactly one logical recipient bootstrap response may
win each MessageRequest under the request-scoped `reply_delivery_key`. The `reply_route` is one-logical-response rather
than one-physical-deposit: exact retries remain admissible and at most 8 distinct candidate wrappers may be admitted over the
request lifetime while it is `OPEN`. If recipient devices race, the first valid requester-side crash-consistent commit wins;
another different wrapper cannot create a second branch. Established records
then carry the reciprocal response while the relationship remains `ACCEPTED_UNBOUND` until both bindings verify.

The winning commit atomically installs the authenticated recipient binding/established route+delivery-key state, marks
the exact wrapper `COMPLETE`, marks every losing `RESERVED` sibling `REJECTED`, and closes the MessageRequest to new
logical bootstrap responses. Cancellation, decline, expiry and terminal invalidation compete on that same `OPEN`
transition and also terminalize every outstanding `RESERVED` candidate before key destruction. The completed bootstrap
private key may then be destroyed: a minimal non-secret replay/request tombstone recognizes the exact winning wrapper
before key lookup so a lost-ack byte-identical retry remains idempotently acknowledgeable. Wrapper expiry cannot exceed
the derived request acceptance deadline, reply-route or reply-delivery-key acceptance window. Candidate rejection or tombstone pruning while `OPEN` never frees one of the eight lifetime slots. Historical backup/restore never reactivates the bootstrap private key, `OPEN`/`RESERVED` state or replay-only request state; a needed introduction is re-issued fresh. Cross-request reuse is a protocol error
(ADR-0144/0145/0146/0147).

This keeps the requester's AccountID hidden before acceptance while ensuring the accepting recipient authenticates the
requester just as strongly as the requester authenticates the recipient.

## Concrete referral-token profile

W0 referral admission uses RFC 9578 **token type 0x0002**, publicly verifiable Blind RSA (2048-bit). The recipient
client controls the issuer private key; its selected IntroGate receives only the issuer public key/key id, so the gate
can verify but cannot mint referral capacity. Issuance happens only to already accepted contacts and is locally
budgeted per relationship.

Type 0x0002 is exactly RFC 9474 `RSABSSA-SHA384-PSS-Deterministic`: the IntroGate verifies the token authenticator
with RSASSA-PSS, SHA-384, MGF1-SHA-384 and a **48-byte salt** (RFC 9578 §6.4), and `token_key_id` is SHA-256 of the
id-RSASSA-PSS SPKI (§6.5). The zero-salt `RSABSSA-SHA384-PSSZERO-*` variants are a different profile; an
implementation must not accept one in place of the other.

**W0 issuance-unlinkability property (ADR-0137).** Given issuance transcripts and later redeemed token bytes, the
issuer — and an IntroGate or other party that obtains/colludes with those issuance transcripts — must not be able,
from the Blind-RSA protocol values themselves, to determine which issuance transaction produced a particular redeemed
token except with negligible advantage. Every issuance therefore samples a fresh, independently generated, uniformly
distributed non-zero blinding scalar from a CSPRNG, resampling values that are not invertible modulo `n`. Reusing a
nontrivial blinding scalar across issuances violates W0 even when the value is not `1`.

This is a **cryptographic/protocol** unlinkability claim. It does not hide from the issuer which contact participated
in an issuance; it does not hide redemption IP address, timing, token use or ticket metadata from the IntroGate; it
does not defeat timing/cardinality correlation when observations are combined; and it does not hide an optional
ReferralAttestation from the recipient after the sealed request is decrypted.

Evidence boundary: the IntroGate redemption model in [`verification/bounded/abuse_w0_rules.py`](../../../verification/bounded/abuse_w0_rules.py) covers only
redemption-side signature verification, challenge binding, issuer-epoch rotation/overlap and single-use (replay)
enforcement. Issuance unlinkability/freshness is evidenced separately by
[`verification/bounded/privacy_pass_issuance.py`](../../../verification/bounded/privacy_pass_issuance.py): RFC 9474
Blind/BlindSign/Finalize reproduces the RFC 9474/RFC 9578 vectors; a deterministic positive control uses distinct
known invertible blinding factors; and a negative control reuses the same nontrivial factor and must expose an
issuer-side repeated candidate marker. This remains executable regression evidence rather than a new proof of RFC
9474 blindness.

The Privacy Pass challenge commits to the IntroGate identity, recipient request-key digest, issuer epoch and admission
class under an OM domain-separation label. The IntroGate treats tokens as single-use and keeps the spent nonce through
the issuer epoch. Revocation is issuer-epoch rotation with bounded overlap; individual unlinkable tokens are not
individually revocable. `admission_proof` remains opaque framing on the W0 wire. ReferralAttestation is separate
client-side provenance and never admission authority or global reputation.
