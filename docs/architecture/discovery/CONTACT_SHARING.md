# Contact Sharing and Introductions

## 1. Goal

Carol should be able to connect Alice and Bob with one simple action without becoming a certificate authority and without learning Alice/Bob private delivery capabilities.

User experience:

```text
Carol opens Alice -> Share contact -> Bob
Bob taps Alice -> Connect
Alice sees "Bob wants to connect" (optionally "Introduced by Carol")
Alice accepts
```

## 2. ContactCapsule

OM defines a signed, expiring `ContactCapsule` suitable for forwarding.

Conceptually:

```text
ContactCapsule {
  capsule_version
  share_id / share_epoch
  subject_intro_identity
  contact_descriptor_or_authenticated_reference
  protocol_versions
  expiry
  share_policy_hint
  signature_by_subject_intro_identity
}
```

The capsule contains **introduction/rendezvous information only**. It MUST NOT contain Alice's private pairwise delivery capabilities used by accepted contacts.

Where privacy policy wants AccountID hidden before acceptance, the capsule uses Alice's alias/share-specific introduction identity; Alice binds/proves stable AccountID inside the accepted E2EE relationship with an `IntroIdentityBinding` ([`INVITES_INTROGATE_AND_SPAM.md`](INVITES_INTROGATE_AND_SPAM.md) §5.1).

## 3. Why Bob trusts Alice, not Carol

Bob verifies Alice's signature on the capsule. Carol merely transports it.

Carol cannot alter Alice's endpoint/key/profile claims without invalidating the capsule.

A referral has two separate parts (ADR-0126):

1. **Admission.** Alice's client pre-issues **referral tokens** to her own contacts (Privacy Pass-style, bound to her
   IntroGate). Carol hands one to Bob with the capsule, and Bob's request presents it as its `admission_proof`. The
   IntroGate verifies only the token, never who Carol is, and admits the request into the reserved referral capacity
   (R-195). Alice never uploads her contacts to the IntroGate. Without a valid token, the request uses the public queue.
2. **Label.** Carol may also give Bob a `ReferralAttestation` signed with her device key in the Carol/Alice
   relationship context:

   ```text
   "I, Carol, intentionally introduced Bob to this Alice capsule."
   ```

   Bob puts it inside the sealed MessageRequest payload. Alice's client verifies it and shows **Introduced by Carol**
   only when Carol is Alice's contact and the signature checks against Carol's current devices. The IntroGate never
   sees it.

A referral is local context, not a global Web-of-Trust score and not transitive reputation.

## 4. Revocation/share control

Alice controls whether her contact capsule is freely shareable.

Possible policies:

- shareable until expiry;
- shareable only N times through IntroGate tickets;
- shareable by accepted contacts;
- referral required;
- disabled.

A `share_epoch`/`share_id` can be checked by Alice's IntroGate. Rotating/revoking the share epoch invalidates old forwardable capsules without breaking already accepted relationships.

Purely offline capsules cannot have instantaneous remote revocation unless the receiving peer later checks Alice's current state; therefore offline share capsules should be short-lived when revocability matters.

## 5. Organic distributed discovery

A signed ContactCapsule can be cached/forwarded like a cryptographically authenticated discovery answer:

```text
Alice signs -> Carol caches -> Carol forwards -> Bob verifies -> Alice IntroGate
```

No global directory query is required.

This is one of the useful DNS-like lessons for OM: authenticated bounded-lifetime discovery data can move through caches/people while authority remains with the subject.
