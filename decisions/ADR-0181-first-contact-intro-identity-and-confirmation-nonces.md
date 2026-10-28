# ADR-0181 — First contact in `core_kernel`: per-invite intro identity, hidden requester account, first-contact limits, Core-issued confirmation nonces

- **Status:** Accepted by operator delegation 2026-10-28
- **Date:** 2026-10-28
- **Resolves:** audit finding H2 and the seven open questions in [`2026-10-28-h2-first-contact-open-questions.md`](../docs/reports/2026-10-28-h2-first-contact-open-questions.md); audit finding M2 (rest) in [`2026-10-28-core-kernel-opus-audit.md`](../docs/reports/2026-10-28-core-kernel-opus-audit.md)
- **Amends (implementation profile, no wire change):** [`INVITES_INTROGATE_AND_SPAM.md`](../docs/architecture/discovery/INVITES_INTROGATE_AND_SPAM.md) sections 5, 5.1, 6 and 11 as built by `core_kernel`; [`UI_CORE_PLATFORM_CONTRACT.md`](../docs/architecture/ux/UI_CORE_PLATFORM_CONTRACT.md) (UI-11 confirmation, `IntroductionRequest`)
- **Does not amend:** frozen W0 CDDL, signature roles, domain-separation labels, the PQ/T HPKE construction (ADR-0131), the sealed delivery profile (ADR-0142 to ADR-0147) or AccountControlState

## Context

The operator delegated the H2 and M2 decisions to the engineering agents on 2026-10-28. Until now first contact trusted a request key through a link digest, carried the requester's whole AccountControlState chain in the sealed request, verified up to 65 chain links with composite signatures before any admission decision, and accepted any non-null token as a destructive-intent confirmation. This ADR fixes the construction so `core_kernel` can build it and the UI can render it. Where the specification was ambiguous the conservative reading was taken and is recorded below.

## Decision

### 1. Intro identity lifecycle (Q1)

1. The **intro identity** is a per-invite composite signing key: Ed25519 AND ML-DSA-65, the same construction as the device signing key (`DeviceSigner`), used with signature role 4 (`intro_identity`).
2. Core derives it; nothing is stored per invite. The profile holds one random 32-byte `intro_root` (zeroized, created on first use). The two seeds are `HKDF-SHA256(salt = "om-core:intro-identity/v0", ikm = intro_root, info = invite_id, L = 64)`. The inviter derives it from the invite id it minted; the requester derives its own from the invite id in the link it accepts. Different invites therefore have unrelated intro keys, and no intro key is a device or account key.
3. **Single use, expires with the invite.** The inviter's intro identity signs exactly one ContactDescriptor and one IntroductionTicket, both expiring at the invite's `expires_at`. The requester's signs exactly one MessageRequestPayload and one sender ContactDescriptor for the one invite it accepts (`accepted_invites` already makes a second use a `Conflict`). Core never signs a second object with an intro identity. An unbound relationship that has not completed binding within `BOOTSTRAP_TTL_S` (7 days, the bootstrap window) is dropped.
4. Clock class of every intro time field: wall-clock seconds, display-and-expiry hint only, with the existing 1-hour bootstrap skew allowance. No intro field is authority.

### 2. Link commits to the intro identity; request key only from a signed descriptor (Q2)

1. The invite link is `om:invite/` + base64url of `invite_id (16) || SHA-256(intro public key) (32)`: 48 bytes. It no longer carries an ACS head hash or the request-key digest.
2. The introduction service stores one opaque record `{1: signed-ContactDescriptor, 2: signed-IntroductionTicket}`. The descriptor follows the frozen W0 shape (`descriptor_id = invite_id`, `subject_hint` = the inviter's label, `protocol_versions`, a Core stand-in `introduction_services` entry, `expires_at`, `provenance`, `request_key`) and carries **no ACS, credentials or chain**. It is signed by the intro identity (role 4, label `OM-CONTACT-DESCRIPTOR-SIG-v1`).
3. The invitee accepts the request key **only** from such a descriptor: it checks the signer's key digest against the link, the signature, `descriptor_id`, expiry, and that the ticket (same signer, `OM-INTRO-TICKET-SIG-v1`) names the same invite, request key and a payload limit it can meet. A substituted descriptor, a descriptor signed by another key, or a missing ticket fails closed as `IntegrityFailure`.
4. The invitee does not learn the inviter's AccountID from the link or the introduction service. It learns it from the inviter's `IntroIdentityBinding` after acceptance (section 4). The link authenticates the intro identity, not the account; the binding ties the two.

### 3. Hidden requester account; `ACCEPTED_UNBOUND` (Q3)

1. The sealed `MessageRequestPayload` follows the frozen shape: ticket id, request-key digest, the requester's intro public key, its signed ContactDescriptor (`subject_hint` = display label, **unverified**), reply route and the fresh request-scoped `reply_delivery_key`. It is signed by the requester's intro identity (role 4, `OM-MESSAGE-REQUEST-PAYLOAD-SIG-v1`). It carries no AccountID, ACS, credentials or chain, so the inviter sees an intro key and a self-chosen label only.
2. `RelState` gains `ACCEPTED_UNBOUND`. The order is:
   1. Inviter accepts: relationship `ACCEPTED_UNBOUND`. It sends one **Accept** (sealed to the requester's reply key, the bootstrap response) carrying its mailbox, its signed `SealedDeliveryKeyRecord`, the conversation id, its `IntroIdentityBinding` (device-signed, `OM-INTRO-IDENTITY-BINDING-SIG-v1`) and its ACS chain and credentials.
   2. Requester verifies the binding: the chain verifies, the signer is an active, non-quarantined device the head commits to, `intro_identity` equals the key the link committed to, `request_key_digest` equals the request it sent, the ACS generation and hash equal the head. It pins the inviter's ACS head, installs the inviter's established delivery key, destroys the bootstrap key (the winning commit, ADR-0145), becomes `ACCEPTED_UNBOUND`, and sends its own **Bind** (its binding for the intro identity it used, its chain and credentials, its signed key record), sealed to the inviter's established key.
   3. Inviter verifies the Bind by the same rules (intro identity equals the one in the request it accepted), pins the requester's ACS head, installs its key, and moves to `ESTABLISHING`.
   4. Both bindings are now verified. Inviter fetches the requester's device bundle, starts the PQXDH session and sends **Hello** (the 1:1 session message, sealed to the requester's established key); the inviter becomes `ACTIVE` when the Hello is deposited. The requester becomes `ACTIVE` when the Hello authenticates in its PQXDH session and carries the conversation id from the Accept.
3. No ordinary message is sent or accepted before `ACTIVE`; a message composed by the inviter while `ESTABLISHING` is queued as before. The requester cannot send in `ACCEPTED_UNBOUND`.
4. The Accept is HPKE-sealed to a request-scoped key only the requester holds and is built only by a holder of the request key; the binding is device-signed and bound to the intro key and request digest, so an observer who sees the public link cannot forge it. The intro identity itself signs no binding (the spec has the device sign it); this is recorded as the conservative reading of "same strength both ways" rather than adding a label.
5. **Migration:** `AppState.version` 2 profiles load. The loader drops state that cannot complete under the new flow (pending introductions, unused invites, relationships that are not `ACTIVE`, and their outbox entries). `ACTIVE` relationships are unchanged.

### 4. Core issues the IntroductionTicket (Q4)

1. For v1 the inviter's Core issues and signs the `IntroductionTicket` itself with the invite's intro identity: `ticket_id = invite_id`, `service_binding` = SHA-256 of the intro public key, `expires_at` = invite expiry, `max_uses = 1`, `payload_limit_bytes = 32768`, attachments and calls disallowed, a random 16-byte `nonce`, `request_key`.
2. **`admission_proof` is empty.** RFC 9578 blind-RSA referral tokens (ADR-0132) need an IntroGate adapter that does not exist yet. Until it does, a Core-issued ticket authorizes nothing beyond "this invite id and request key"; abuse control for it is the local limiter below. When an adapter lands, the ticket moves behind it with no change to the Core objects.
3. Where did authority go? Nowhere new: the recipient's own Core is the only issuer, and the ticket gives the sender no right the invite did not already give.

### 5. Order of checks and the local limiter (Q5)

On a message arriving at an invite mailbox, in this order and **before any HPKE decapsulation, chain or signature verification**:

1. the invite exists, is unused, unrevoked and unexpired;
2. the whole message is at most 40,000 bytes and the ciphertext at most `payload_limit_bytes` plus the AEAD tag;
3. the W0 `MessageRequest` shape is valid;
4. the ticket id and request-key digest equal the invite's;
5. the **limiter** admits it.

The limiter is Core-local and persisted with the profile: at most **20** requests admitted per rolling hour and **100** per rolling day. Its clock is the wall clock guarded by a persisted non-decreasing high-water mark (a clock set backwards cannot reopen a window), and survives restart. A refused request is **silently dropped** (acknowledged to the relay, no state, no reply). Drops are aggregated into one counter, exposed as the query `GetIntroductionLoad`, which the UI renders as one notice: "you are receiving a lot of requests from strangers; people you invited can still reach you". Limitations, recorded: the budget is per profile, not per invite, so a flood on one link also uses the budget for others (reserved invite-bound capacity belongs to an IntroGate, section 11 point 1); an attacker holding a link can exhaust the hourly budget but cannot make the device do more than 20 decapsulations an hour for it. Section 6 of the architecture is unchanged: PoW, attestation and referral tokens remain IntroGate-side.

### 6. First-contact chain cap (Q6)

An ACS chain received from a peer that is not yet bound (the Accept at the requester, the Bind at the inviter) is refused when it has more than **16 entries**, before any signature is verified. This is **local resource policy, not a protocol rule**: the protocol cap (64 links) is unchanged, and the cap is a named Core constant that a client may raise. Cost: an account with a longer chain cannot complete first contact with a Core using this cap until chain compaction exists; the refusal is clean (nothing pinned, no key installed, counted as an integrity failure on that relationship).

### 7. Verification field (Q7)

`IntroductionRequest` gains `verification: "unverified" | "bound"`. A pending request is always `unverified` (the intro key is not bound to any account). The conversation list carries a `relationship` state code (`awaiting_acceptance`, `accepted_unbound`, `establishing`, `active`) and an `unverified_contact` notice while unbound. The UI shows "Unverified" next to the requester's name and must not present the label as a verified name.

### 8. Core-issued confirmation nonces (M2)

1. A destructive intent (`DESTRUCTIVE_INTENT_KINDS`) carries `confirmation = {nonce}`. The nonce is minted by Core: the intent `IssueConfirmation {intentKind, target}` stores a 16-byte random nonce bound to `(intent kind, target, profile)` with a **5-minute** expiry, and `GetConfirmation {operationId}` returns `{nonce, expiresAt}` once.
2. When the destructive intent arrives Core removes the nonce **whether or not it matches** (single use), and answers `Rejected(unauthorized)` unless the nonce exists in this profile, has not expired, and matches the intent's kind and target. A bare object, a missing or non-string nonce, a reused, expired, foreign-profile or mismatched nonce is `unauthorized`. At most 16 nonces are held; the oldest is dropped.
3. `target` is: `personId` (BlockPerson), `deviceId` (SuspendDevice, RevokeDevice), `requestId` (ApproveDeviceLink, VetoRecovery), the policy code (ChangePrivacyPolicy), `scope:custodian` (StartRecovery).
4. The Action Registry still checks that the click is a direct action and binds the token to the action and parameters (UI-side guard); Core is now the authority that the confirmation was real. A learned or suggested action cannot obtain a nonce because the registry issues one only for `origin: "direct"`.

## Consequences

- R-411 to R-417 and T-164 to T-166 record the rules; D-202 records the decision. All are verified by `conformance:discovery`, `conformance:ui_intents` and the new regression tests (`crates/om-core/tests/first_contact.rs`, `first_contact_limits.rs`, `confirmations.rs`, the harness and registry tests).
- The first-contact wire inside Core is now the frozen W0 object set for the descriptor, ticket, payload and binding (signed envelopes, labels, role 4). `ReferralAttestation`, `greeting` and a real `relay_uri` for `reply_route` are not built; the Core stand-in route URI from the audit's M1 remains and must be replaced before a provider adapter ships.
- Existing dev profiles migrate as described in section 3.5; unfinished first contacts must be restarted.
- **Signature entry shape.** The frozen `signature.signer` field is a 16..32 byte key id, so an intro-identity signature carries the SHA-256 key digest (the same value the link commits to) and the verifier supplies the public key: the record key 3 for the descriptor and ticket, `sender_intro_identity` for the payload.
- **Codec correction (not a spec change).** The Accept and Bind carry an ACS chain (up to 16 entries) and credentials inside a sealed delivery, so they use the 64 KiB and 128 KiB buckets that [`delivery.cddl`](../spec/cddl/delivery.cddl) allows (`ciphertext bstr .size (17..131088)`). The decoder applied the profile-wide `max_string_bytes` (65536) before the per-field bounds and therefore rejected those buckets and `MessageRequest` ciphertexts above 65536 bytes. `om-codec` now raises the string limit to the object's own registered `max_bytes` (the per-field schema bounds still apply). Registries and vectors are unchanged and the W0 corpus replays identically; a regression test seals 20 KB and 70 KB inner payloads and checks them against the registry.
- Residual risks: single-vendor design review (ADR-0167); the per-profile limiter budget; an unbound relationship holds a mailbox and key for up to 7 days; the account the inviter's binding names is learned at acceptance and trusted on first use, as every other first contact.

## Where did authority go?

Nowhere new. The requester's account gains no authority before acceptance and loses no secrecy: the inviter's Core holds only an intro key until both devices have signed bindings. The recipient's Core remains the only ticket issuer and the only confirmation authority for the user's own destructive actions.
