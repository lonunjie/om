# ADR-0131 — Core-kernel final pre-engineering hardening

- **Status:** Accepted
- **Date:** 2026-09-25
- **Decided by:** integrator under operator delegation 2026-09-25 ("do what you need to do"), after independent architecture review
- **Resolves:** A-024, A-025, A-026, A-027, A-028 and newly identified recovery/bootstrap/reduction gaps
- **Amends:** ADR-0107, ADR-0118, ADR-0122, ADR-0126, ADR-0127, ADR-0128

## Context

The verification work had reduced the crypto and authority design to a small set of implementation-defining choices, but leaving those choices to engineering would create incompatible wire/state implementations. A further review also found three cross-cutting gaps: a recovery request could be withheld until its delay had allegedly elapsed; rebasable reductions could target a later reincarnation of the same semantic object; and first-contact stable-identity binding was specified only recipient-to-sender.

The constitution favors user-owned authority, fail-closed behavior, reviewed constructions, no silent central authority, and explicit residual risk. This ADR therefore chooses simple, auditable rules even where they cost some availability or metadata privacy.

## Decision

### 1. PQXDH identity and prekey binding is dual-signed

For OM-Crypto-v1 every PQXDH public object that can redirect a session is bound to the OM DeviceIdentity by **both** of the DeviceIdentity's required signature families: Ed25519 and ML-DSA-65.

A `PQXDHDeviceBinding` certificate commits to:

- AccountID;
- DeviceID and DeviceCredential digest;
- AccountControlState generation and state hash;
- PQXDH X25519 identity public key;
- signed-prekey public key and signed-prekey epoch;
- ML-KEM-768 signed prekey/ciphertext-key public material used by the profile;
- profile id and object version;
- validity/rotation epoch, not wall-clock authority.

The binding is accepted only when both signatures verify and the DeviceCredential is active and non-quarantined at the referenced ACS head. A server cannot substitute either the classical identity key or PQ prekey material by relying on only a classical signature. This does **not** claim fully post-quantum mutual authentication for PQXDH; it protects OM's durable credential binding against future classical-signature forgery.

### 2. Last-resort PQXDH replays use an exact, durable per-prekey ledger

A no-one-time-key PQXDH initiation has `handshake_id = DS-HASH("OM-PQXDH-HANDSHAKE-ID-v1", canonical transcript fields)` including sender ephemeral key, PQ ciphertext, recipient DeviceID, signed-prekey epoch and profile.

The recipient stores an exact set of accepted `handshake_id`s **before** releasing application plaintext or acknowledging the initiation. The set is namespaced by recipient DeviceID and signed-prekey epoch and persists in DeviceVault for the whole acceptance lifetime of that signed-prekey epoch plus its configured overlap. It is not a Bloom filter and has no LRU eviction.

Each signed-prekey epoch has a hard `last_resort_accept_limit` of **4096** initiations. At the limit the device rejects further last-resort handshakes and publishes fresh key material; availability fails closed rather than evicting replay evidence. Normal one-time-key initiations do not consume this allowance.

Backups never restore ratchet state, prekey secrets or replay ledgers onto another DeviceIdentity. A restored installation enrolls as a fresh DeviceID with fresh PQXDH material; the old DeviceID's material is revoked/quarantined through AccountControlState. Therefore rollback of the replay ledger cannot resurrect the old cryptographic endpoint.

### 3. OM-Crypto-v1 payload AEAD is AES-256-GCM-SIV with deterministic derived nonces

The v1 Double Ratchet message encryption profile uses **AES-256-GCM-SIV (RFC 8452)**.

For each ratchet message key `MK`, HKDF-SHA-256 derives disjoint values:

- `K_aead = HKDF-Expand(MK, "OM-DR-AEAD-KEY-v1", 32)`;
- `N_aead = first_12_bytes(HKDF-Expand(MK, "OM-DR-AEAD-NONCE-v1" || header_hash, 32))`.

The canonical authenticated ratchet header hash and immutable envelope context are AEAD associated data. Implementations MUST still guarantee one message key per message; GCM-SIV is defense in depth, not permission to reuse ratchet keys. V-001 owns KATs, native/wasm feasibility and dependency review.

### 4. Double Ratchet header encryption is not part of OM-Crypto-v1

OM-Crypto-v1 leaves the ordinary Double Ratchet header needed for session processing visible inside the already sender/recipient-routed transport envelope. Relays may therefore observe header length and ratchet-shape metadata available at their layer. The metadata descriptor MUST say so.

Header encryption may be added only by a new CryptoProfile/version with independent interop and metadata analysis. V-015 measures and documents the exposure; it does not silently change v1 crypto.

### 5. PQ wording is exact

For a v1 1:1 conversation the UI may say **"Post-quantum handshake protection"** or equivalent. It MUST NOT claim post-quantum post-compromise healing, "per-message PQ", or a generic unexplained "PQ protection: on". OM-Crypto-v1 mitigates harvest-now-decrypt-later for initial asynchronous key establishment through hybrid PQXDH; post-compromise healing remains classical until a later reviewed ratchet profile is negotiated.

### 6. Recovery-alone authority is valid only after the verifier's own observation interval

There is no trusted global recovery clock.

When recovery acts without a current non-quarantined controller co-signature, every verifier records the first valid observation of the `RecoveryRequest` digest in durable local security state. That verifier MUST NOT accept a recovery-alone successor as final until its own monotonic elapsed observation reaches the RecoveryPolicy activation delay and it has observed no valid RecoveryVeto.

A verifier coming online late starts its own observation interval. A Home timestamp, wall clock in the request, or assertion by the recovery custodian cannot shorten it. Current controllers may make the transition immediate by co-signing under ADR-0127. Optional future witness acceleration requires a separate reviewed protocol and cannot be assumed by v1.

This rule intentionally trades recovery speed after a long partition for resistance to a RecoveryRoot holder who withholds the request from live controllers. Homes still fan out requests promptly, but are availability helpers, never recovery-time authorities.

### 7. Every rebasable reduction targets an immutable incarnation

The ADR-0122/0128 rule that reductions remain live is amended:

> a rebasable reduction targets a unique authority/membership/grant **instance**, never only a semantic name.

Each reducible object has an immutable instance identifier or equivalent creation digest committed at creation. Examples include `grant_id`, `membership_instance_id`, `service_installation_id`, endpoint-credential digest, invite instance and Space-binding instance.

On rebase, if that exact instance is absent, the reduction is an idempotent no-op. It MUST NOT match a later re-add/regrant/reinstallation with the same AccountID, ServiceID, role name or capability tuple. A revoked DeviceID is never re-used; returning hardware enrolls with a fresh DeviceID and credential. This closes stale-reduction/ABA attacks while preserving monotone revocation liveness.

### 8. First-contact stable identity binding is symmetric

`IntroIdentityBinding` is not recipient-only. After a MessageRequest is accepted:

1. the accepting recipient sends its binding from intro identity to stable AccountID/ACS;
2. the requester verifies it and sends its own `IntroIdentityBinding` covering the intro identity used in the request and its stable AccountID/ACS;
3. the recipient verifies the requester's binding;
4. only then is the RelationshipBinding `ACTIVE` and normal PQXDH fanout to stable-account device bundles allowed.

Until both directions verify, the relationship is `ACCEPTED_UNBOUND`: UI may show the accepted intro profile, but clients MUST NOT attribute stable AccountID history, auto-merge contacts or grant relationship-scoped key-material fetch authority.

This preserves sender AccountID privacy before acceptance while preventing an asymmetric identity transition.

### 9. MessageRequest HPKE pins the current IETF PQ/T HPKE construction; OM invents none

The W0 MessageRequest construction uses RFC 9180 base mode semantics with the **MLKEM768-X25519** hybrid KEM as specified by `draft-ietf-hpke-pq-05` (July 2026) and its published test-vector construction, with:

- KEM: MLKEM768-X25519 (`kem_id` 0x647a in that draft);
- KDF: HKDF-SHA256;
- AEAD: ChaCha20Poly1305;
- mode: base;
- `info`: the registered `OM-MESSAGE-REQUEST-HPKE-INFO-v1` context plus profile id, ticket id and request-key digest.

Because the HPKE PQ/T document is still an Internet-Draft at this checkpoint, OM records the exact draft version as part of CryptoProfile v1. A later RFC or incompatible draft revision is a **new profile version/security-generation-compatible migration**, never a silent reinterpretation of v1 bytes. RFC 10024's X25519MLKEM768 TLS mechanism is informative evidence for the component hybrid, not the HPKE wire definition.

V-001/V-005 must consume the pinned draft's vectors and OM envelope vectors before `core_kernel` can unlock.

## Consequences

- A-024..A-028 are resolved as architecture questions; remaining work is verification evidence.
- V-001 gains dual-binding, exact replay-ledger, AES-256-GCM-SIV and pinned PQ/T-HPKE criteria.
- V-002 gains withheld-request/late-verifier traces.
- V-004/V-008/V-011/V-016 gain stale-incarnation/ABA negative cases where relevant.
- V-005/V-015/V-016 gain symmetric IntroIdentityBinding vectors/state transitions.
- No new online authority or trusted clock is introduced.
