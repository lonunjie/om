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

The ticket issuer may be the recipient's chosen IntroGate or a delegated anti-abuse service.

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

Normal users should never see these mechanisms during ordinary traffic.

## 7. Privacy Pass direction

Privacy Pass-style unlinkable tokens are a candidate for proving that a requester passed an anti-abuse step without creating a permanent global user identifier or reputation score.

OM should not require a specific Privacy Pass issuer in the core protocol. Define an opaque `AdmissionProof` interface and standardize concrete schemes only after review.

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

