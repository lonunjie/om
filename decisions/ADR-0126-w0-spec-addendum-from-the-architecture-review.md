# ADR-0126 — W0 spec addendum from the architecture review

- **Status:** Accepted
- **Date:** 2026-09-24
- **Decided by:** integrator under operator delegation 2026-09-24 ("do everything and complete it"), from the
  architecture review
- **Source:** [`2026-09-24-claude-opus-architecture-r1.md`](../verification/reviews/2026-09-24-claude-opus-architecture-r1.md)
  (F-R1-3, F-R1-5, F-R1-6, F-R1-9, F-R1-11, F-R1-12, and the out-of-scope note on ADR-0118 §6)
- **Amends:** ADR-0119 (the frozen W0 set: additions only, no assigned value changes), [`discovery.cddl`](../spec/cddl/discovery.cddl),
  [`account.cddl`](../spec/cddl/account.cddl), [`keys.cddl`](../spec/cddl/keys.cddl), [`om-object-types.json`](../spec/registries/om-object-types.json), [`om-numeric-fields.json`](../spec/registries/om-numeric-fields.json),
  [`om-domain-separation.json`](../spec/registries/om-domain-separation.json), [`om-crypto-profiles.json`](../spec/registries/om-crypto-profiles.json), [`om-time-fields.json`](../spec/registries/om-time-fields.json),
  [`w0-metadata-exposure-descriptors.json`](../spec/metadata/w0-metadata-exposure-descriptors.json)

## Context

The architecture review found W0 behaviour that the frozen CDDL cannot express: first contact had no encryption
object, a fork bridge could not reference both branch heads, the downgrade floor compared opaque profile ids, and the
RelaySet signer was undefined. ADR-0119 lets the frozen registries grow; it never lets an assigned value change. This
ADR collects every W0 wire addition from the review in one place.

## Decision

1. **Request key.** A recipient publishes a `request_key` (new struct `RequestKey {0: profile-id, 1: public key,
   ? 2: expires_at}`) in its `ContactDescriptor` (new key 15) and may repeat it in an `IntroductionTicket` (new key
   11). The key is a public key of the hybrid KEM of the named CryptoProfile (for OM-Crypto-v1: X25519 + ML-KEM-768).
   A sender uses a request key only from an object signed by the recipient's intro identity. Its digest uses the new
   label `OM-REQUEST-KEY-HASH-v1`.
2. **MessageRequest (object type 19).** The first-contact message is sealed using RFC 9180 base-mode semantics and
   the exact PQ/T HPKE profile pinned by ADR-0131 rule 9: `draft-ietf-hpke-pq-05` MLKEM768-X25519, HKDF-SHA256,
   ChaCha20Poly1305. OM does not invent a hybrid KEM. The draft version is part of OM-Crypto-v1; an incompatible later
   draft/RFC requires a new profile/version. The `info` input is `OM-MESSAGE-REQUEST-HPKE-INFO-v1` plus profile id,
   ticket id and request-key digest. The outer object carries only the ticket id, the request-key digest, the profile, the HPKE
   encapsulation, the ciphertext and an optional admission proof. It carries **no** sender AccountID, device id or
   sender intro key, so the IntroGate sees the ticket and admission token only. The IntroGate rejects a ciphertext
   longer than the ticket's `payload_limit_bytes` plus the AEAD tag.
3. **MessageRequestPayload (object type 20)** is the signed plaintext inside the HPKE ciphertext. The sender signs it
   with its own intro identity (role `intro_identity`). It repeats the outer ticket id and request-key digest, so a
   payload cannot be replayed into another request, and carries the sender's signed ContactDescriptor, an optional
   greeting, optional ReferralAttestations and a reply route (a `relay-entry`).
4. **IntroIdentityBinding (object type 21).** A signature, by a device that the subject AccountControlState lists as
   active and not quarantined (role `device`), over the subject AccountID, intro public key, request-key digest and the
   ACS generation/hash it was made under. It is disclosed only after acceptance. ADR-0131 rule 8 makes the exchange
   symmetric: recipient binding first, requester binding second. The relationship remains `ACCEPTED_UNBOUND` until
   both bindings verify; only then may normal PQXDH fanout use stable-account device bundles.
5. **Fork bridge extension `om.acs.fork_ack`.** A critical key-23 extension of AccountControlState whose value is
   `{0: [2*16 digest]}`, the set of observed branch-head digests (OM-ACS-HASH-v1). A bridge state's
   `previous_state_hash` is the common ancestor. Authorization rules are in ADR-0127.
6. **`security_generation`.** Each entry of [`om-crypto-profiles.json`](../spec/registries/om-crypto-profiles.json) gets `security_generation`, a map from
   conversation type (`one_to_one`, `group`) to an unsigned integer. The downgrade floor compares this field for the
   conversation type in question, never profile ids. The only floor movement that is not an increase: a
   SecurityAdvisory (ADR-0104) marks a profile `forbidden`; the floor then moves to a non-forbidden profile of
   **equal or higher** generation, never lower. If none exists, the conversation stops sending until one does. There
   is no other exception. ADR-0158 further pins `AccountControlState.minimum_crypto_profile` to the
   `one_to_one` axis only; group floors remain Room/group state and do not derive from that ACS field.
7. **`pq_signed_prekey` is mandatory** in AsyncKeyBundle (key 8), as ADR-0118 §6 requires. The CDDL is changed. The
   numeric-fields registry keeps its assigned `req: false` value (immutable) and instead records the rule as
   OM-Crypto-v1's `required_bundle_fields`; validators apply both.
8. **RelaySet semantics on the frozen fields.** No RelaySet extension is needed. REACHABILITY §2's semantics map as
   follows: `valid_from` is the moment a peer accepts the higher `version`; `overlap_until` is each old entry's own
   `expires_at`; emergency revocation is a new version that omits the entry, plus revoking its drain capability at
   the relay; the recovery rendezvous lives in the separate ReachabilityRecoveryDescriptor. RelaySet and
   BlobLocationRecord signer rules are normative text in the delivery docs (R-211).
9. **Disappearing-message timer.** `ConversationEvent.expiry` (key 12, absolute wall clock) is the v1 interoperable
   timer. `MessageCreate.expires_after` is a local hint; a receiver MUST NOT use it to keep content past `expiry`.
   The time registry row gets a note; no value changes.
10. **Metadata descriptor correction (superseded 2026-09-27 by ADR-0140).** This ADR originally declared `stable_account_id: sometimes` and `device_count: sometimes` because the pre-deployment bundle carried account-wide ACS fields. ADR-0140 removes those fields from AsyncKeyBundle v1 before release; current descriptors therefore use `no_by_design` for those two bundle-wire exposures while retaining `stable_device_correlator: always` for `subject_device_id`.

## Consequences

- New W0 object types 19-21, structs `RequestKey`, `MessageRequest`, `MessageRequestPayload`, `IntroIdentityBinding`
  and `ForkAck`, the extension `om.acs.fork_ack`, and four new domain-separation labels (one reserved) are
  registered. No assigned value changed, so
  `check-registries` stays clean.
- V-005 needs vectors and both codecs for the additions. The existing `AsyncKeyBundle/minimal` positive vector lacks
  key 8 and becomes a negative vector. V-015 and V-016 need MessageRequest and IntroIdentityBinding vectors.
- R-202, R-203, R-206, R-211, R-212 and R-216 carry these rules.
