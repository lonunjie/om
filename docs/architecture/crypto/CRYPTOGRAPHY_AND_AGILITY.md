# Cryptography and Crypto Agility

## Goals

1. Strong E2EE using reviewed protocols and primitives.
2. Post-quantum protection where mature enough, with conservative hybrid hedging.
3. Complete algorithm replacement without changing human/account identity.
4. Invisible migrations for normal users.
5. No downgrade attacks disguised as compatibility.
6. No arbitrary “crypto buffet” negotiated from independent primitives.

## AccountID must be algorithm-independent

`AccountID` represents an account lineage, not a specific Ed25519/X25519/ML-DSA/ML-KEM key.

`AccountID` is a random 256-bit stable identifier. Account authority is the single `AccountControlState` object
([`account.cddl`](../../../spec/cddl/account.cddl), [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../identity/ACCOUNT_CONTINUITY_AND_CONTROL.md)); per-device keys live in `DeviceCredential` objects bound by
the ACS `device_credentials` digests, and the ACS carries a `minimum_crypto_profile` floor. There is no separate
credential-set or account-authority object. The invariant is fixed: replacing cryptography does not replace AccountID.

## CryptoProfiles, not arbitrary primitive negotiation

Implementation code may expose narrow interfaces such as:

```text
Kem
Signature
Aead
Hash
Kdf
Ratchet
GroupCrypto
```

but the wire protocol negotiates only reviewed, versioned profiles:

```text
CryptoProfile {
  profile_id
  status: experimental | allowed | preferred | deprecated | forbidden
  identity_auth
  async_handshake
  one_to_one_ratchet
  group_suite
  media_keying
  aead
  hash_kdf
  security_generation: { one_to_one?, group? }   // ADR-0126; the floor compares this
  minimum_client_generation?
}
```

The registry form is [`om-crypto-profiles.json`](../../../spec/registries/om-crypto-profiles.json). A profile id is an allocation number, not a strength; profiles are
ordered only by `security_generation`, separately per conversation type (ADR-0107: a 1:1 hybrid profile and a
classical MLS profile are not comparable).

Peers MUST NOT assemble arbitrary combinations in production.

## PQ-first, hybrid-secure default

Where mature and interoperable, OM should combine a classical and post-quantum assumption instead of betting the entire system on one new primitive family.

Conceptual profile:

```text
OM-Crypto-v1 (registry id 1)
  account/device authentication: classical + PQ-capable authenticated credential transition
  async 1:1 key establishment: PQXDH-class or equivalent hybrid-reviewed design
  ongoing 1:1: Double Ratchet in v1; Triple-Ratchet-class hybrid continuous key evolution in v2 (ADR-0118)
  groups: MLS with approved hybrid/PQ ciphersuite when standard/implementation maturity is sufficient
  content: strong symmetric AEAD
  attachments/backups: random 256-bit content keys + strong AEAD
```

Specific algorithms and libraries are deliberately not frozen into this document. They belong to a profile ADR and can change without changing the higher-level protocol.

## 1:1 reference direction

Signal's published PQXDH and current Double Ratchet specification (which includes Sparse Post-Quantum Ratchet and Triple Ratchet) are strong reference designs for asynchronous PQ-capable messaging.

OM must not equate “uses PQXDH” with “every authentication property is post-quantum.” The published PQXDH revision explicitly retains classical assumptions for mutual authentication. OM's credential layer therefore needs its own PQ-aware authentication/migration plan.

## Groups

Private groups remain based on MLS. Base MLS is RFC 9420. PQ/hybrid MLS ciphersuites are still evolving as an IETF work item in September 2026, so OM must:

- keep group ciphersuite behind CryptoProfile;
- allow experimental implementation for testing;
- not permanently assign OM semantics to transient draft identifiers;
- support controlled group reinitialization/migration to a finalized profile;
- bind group profile/epoch changes into authenticated state;
- forbid silent profile downgrade.

OpenMLS is a leading Rust implementation candidate, not a protocol dependency.

## Downgrade floor

Each relationship/device/group records the highest `security_generation` it has accepted for its conversation type
(ADR-0126). Restored records and the ACS `minimum_crypto_profile` combine by taking the maximum (ADR-0120).

```text
security_floor(conversation) = G   // security_generation[conversation_type] of the strongest accepted profile

if generation_of(proposed_profile, conversation_type) < G:
    reject
```

There is exactly one case where the floor moves without increasing: a signed SecurityAdvisory (ADR-0104) marks a
profile `forbidden`. The floor then moves to a non-forbidden profile of **equal or higher** generation; if none exists,
the conversation stops sending and says so until one does. The floor never moves lower. ~~"no explicit authenticated
migration exception"~~ and any "emergency migration procedure" are deleted: there is no other exception. V-003 carries
the negative vector.

A server or active network attacker cannot make two upgraded peers silently fall back to an old profile.

## Migration behavior

Crypto migration must normally be invisible:

1. client update learns new allowed/preferred profile;
2. device advertises authenticated support;
3. peer/group state detects mutual support;
4. a new session/epoch is established in background;
5. new traffic moves to the new profile;
6. old session remains only for bounded in-flight compatibility;
7. downgrade floor advances;
8. deprecated secrets/state are erased when safe.

No ordinary user prompt is required unless identity continuity cannot be proven or the migration weakens a security property.

## Provider independence

Crypto libraries are replaceable implementations. `libsignal`, OpenMLS, platform key stores, WASM crypto libraries, or future audited libraries must sit behind OM interfaces. A library API/serialization format must never become the only canonical wire/storage format without an explicit ADR.

## Symmetric cryptography and PQ

PQ migration primarily changes public-key/KEM/signature assumptions. Strong symmetric primitives with adequate key sizes remain suitable in a post-quantum design. OM should use conservative key sizes and KDF/hash output lengths through CryptoProfiles.

## Update/release signatures

V1 client update metadata is algorithm-agile and can carry multiple algorithm-tagged signatures under the owner-pinned release-root policy (ADR-0148). ADR-0149 makes the acceptance unit explicit: metadata role + authorized key ID/public key + algorithm + root-policy generation + exact canonical metadata bytes. Thresholds count distinct authorized keys, never algorithm tags or duplicate signatures; root versions transition sequentially under old-root and new-root threshold checks. A maintained reference distribution should add an ML-DSA-65 co-signature for long-lived root/release/targets metadata once a reviewed implementation is suitable; the co-signature remains optional until an authenticated root-policy transition makes its authorized role/key/algorithm contribution required. Required-signature stripping, algorithm/key/role substitution, rollback and release-key migration use the same continuity/fail-closed discipline as user credentials.

## Required tests

- profile negotiation vectors;
- downgrade rejection;
- cross-version session migration;
- group profile migration/reinit;
- device added before/after profile upgrade;
- restore from backup created under older profile;
- account credential transition across algorithm families;
- unsupported/forbidden profile behavior;
- compromised provider cannot force weaker crypto;
- implementation/library replacement preserves wire interoperability.

## Recovery crypto is independently agile

Recovery mechanisms use versioned `RecoveryPolicy` methods rather than embedding one mnemonic/guardian algorithm forever. A 256-bit random offline recovery root is the W0 baseline. Shamir mnemonic shares are feasible prior art; future standardized threshold/MPC authorization can replace them without changing AccountID.

NIST's 2026 crypto-agility guidance reinforces the architectural requirement that algorithms be replaceable while operations continue. NIST's 2026 threshold call is a reason to reserve the future threshold interface rather than invent one prematurely.

## 2026-09-22 engineering re-audit

Production integration is now explicitly gated by [`PRODUCTION_CRYPTO_INTEGRATION_GATE.md`](PRODUCTION_CRYPTO_INTEGRATION_GATE.md) and [`docs/research/SECURITY_CRYPTO_REAUDIT_20260922.md`](../../research/SECURITY_CRYPTO_REAUDIT_20260922.md).

Key engineering updates:

- check current NIST FIPS 203/204 errata and official vectors before pinning an ML-KEM/ML-DSA implementation;
- follow NIST SP 800-227 KEM-use guidance;
- keep HQC reserved for future profile diversity until a stable standard/review exists;
- treat Signal PQXDH/Triple Ratchet/ML-KEM Braid as reviewed reference designs, not permission to improvise a hybrid construction;
- do not make `libsignal` private APIs/storage formats OM protocol dependencies because upstream explicitly marks external use unsupported;
- base MLS can use a reviewed library candidate such as OpenMLS behind OM storage/profile boundaries, while current PQ MLS draft suites remain `experimental`;
- signed/control/event objects use bounded OM deterministic CBOR rather than JSON canonicalization;
- ordinary messages do not gain a separate durable AccountID signature merely for evidence export.
## 2026-09-22 architecture hardening

CryptoProfile v1 must specify not only primitives/handshake/ratchet but also the public logical shape and lifecycle of asynchronous per-device handshake material. [`ASYNC_KEY_MATERIAL.md`](ASYNC_KEY_MATERIAL.md) is canonical architecture; runtime implementation proof is tracked separately.

Multi-device session selection is outside the ratchet itself and is defined by [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](ONE_TO_ONE_SESSION_MANAGEMENT.md). No production security claim is complete until both layers have conformance vectors and stale/replay/depletion/device-change tests.

## 2026-09-23 OM-Crypto-v1 architecture resolution

Q-001/Q-044 are no longer open design choices. OM-Crypto-v1 targets the published PQXDH + ~~Triple-Ratchet/SPQR-class~~ Double Ratchet construction (ADR-0118; SPQR is the v2 path) with X25519 + ML-KEM-768 for asynchronous hybrid establishment, classical PQXDH authentication semantics as currently specified, HKDF/SHA-256 family derivation, and misuse-resistant authenticated encryption. Durable constitutional authentication uses versioned dual classical/PQ signatures where the AccountControl/CryptoProfile requires it. The remaining production-library/runtime work is `V-001`, a proof gate—not permission to substitute a different wire protocol during implementation.

### OM-Crypto-v1 sealed-delivery binding (ADR-0142)

OM-Crypto-v1 also pins mandatory sealed relay delivery for established 1:1 traffic. It reuses the profile's reviewed
`draft-ietf-hpke-pq-05` MLKEM768-X25519 / HKDF-SHA256 / ChaCha20Poly1305 suite under distinct sealed-delivery
domain-separation labels and route-bound `info`/AAD. Reuse of the suite does not reuse request keys or HPKE contexts.
The outer open is transport confidentiality only; sender authentication remains entirely in the inner
relationship-pinned PQXDH/Double-Ratchet path. OM-Crypto-v1 has no clear-sender compatibility fallback.

## 2026-09-24 post-quantum status is per conversation type (ADR-0107)

OM-Crypto-v1 provides different confidentiality properties to different conversation types, and OM must say so precisely:

| Conversation type | v1 key establishment | Harvest-now-decrypt-later exposure |
|---|---|---|
| 1:1 | PQXDH (X25519 + ML-KEM-768) + Double Ratchet (ADR-0118; SPQR in v2) | mitigated (hybrid handshake); no PQ post-compromise healing until v2 |
| Private group / Room (MLS) | RFC 9420 X25519 suite | **not mitigated in v1** (classical) |
| 1:1 calls | derived from the underlying 1:1 secure session | inherits the 1:1 session property |
| Group calls | per-sender SFrame media keys distributed only to joined participant DeviceIDs; MLS authenticates call-control membership but is not the media-key source (ADR-0132) | **no PQ confidentiality claim in v1** (R-173) |
| Durable control signatures | Ed25519 + ML-DSA-65 | n/a (authentication, dual) |

Rules:

1. No UI, profile, marketing or documentation may claim PQ confidentiality for v1 groups or group calls (R-173). Security details may show a per-conversation "Post-quantum protection: on/off" fact derived from the active profile.
2. The v1 group wire MUST carry an authenticated, downgrade-safe **ReInit/migration** path from the v1 suite to a hybrid-PQ MLS profile. The path preserves `ConversationID`, GroupControlState authority and roster, and it must be exercised against an experimental hybrid suite before group wire freeze (R-174, V-019).
3. **Adoption triggers** for a hybrid-PQ MLS CryptoProfile. All are required:
   - an IETF-published (RFC or WG-last-call-stable) ciphersuite/combiner definition with assigned codepoints;
   - ≥2 independent interoperable implementations passing that suite's vectors;
   - OM review of bandwidth/commit-size impact on target group sizes and mobile networks;
   - a reviewed OM CryptoProfile entry (new immutable ID, Q-013).
   IETF MLS WG drafts for ML-KEM/X-Wing-based suites and an amortized PQ combiner exist, and at least one candidate library exposes experimental hybrid suites. Verify current status before relying on this.
4. When the triggers are met, the new profile becomes the default for **new** groups, and existing groups migrate automatically through ReInit without user action (R-011). An established group's floor never silently drops back.
5. Until then, users with HNDL-sensitive content are pointed, in Security details, to 1:1 conversations, which are hybrid-PQ in v1. OM does not build a parallel pairwise-fanout group mode to paper over this.

## 2026-09-24 verification rulings

Normative, from the V-series models. The ADR is authoritative where this summary is shorter.

- [ADR-0120](../../../decisions/ADR-0120-one-to-one-session-manager-rules.md) (downgrade floor): per-peer floor records are included in backups. After a restore the floor is the maximum of the restored record and the AccountControlState minimum profile. A receiver that rejects a session below the floor sends an authenticated reject naming the message, and the sender retries at the floor.
- [ADR-0118](../../../decisions/ADR-0118-production-crypto-dependency-licensing-and-update-strategy.md): OM-Crypto-v1 is PQXDH + Double Ratchet, because the only SPQR implementation is AGPL. v1 is hybrid-PQ against harvest-now-decrypt-later but has no PQ post-compromise healing. `OM-Crypto-v2` adds a clean-room SPQR/ML-KEM Braid implementation.

## OM-Crypto-v1 final profile details (ADR-0131)

- PQXDH redirecting identity/prekey material is bound to AccountID/DeviceID/DeviceCredential/ACS and requires both
  Ed25519 and ML-DSA-65 signatures.
- ADR-0140 re-freezes AsyncKeyBundle v1 without provider-visible ACS generation/hash, adds a durable per-peer/device/profile signed-prekey generation floor, rejects equal-generation core equivocation, and context-binds one-time prekeys to their DeviceID/profile/generation/provider-pool.
- Last-resort (no one-time-key) handshakes use an exact durable per-signed-prekey replay ledger. The default hard cap
  is 4096 accepted last-resort transcript ids per signed-prekey epoch; exhaustion fails closed and triggers refresh.
- Double Ratchet payload encryption uses AES-256-GCM-SIV (RFC 8452). HKDF-SHA256 derives disjoint AEAD key and
  deterministic nonce material; immutable header/envelope context is associated data.
- OM-Crypto-v1 does not encrypt Double Ratchet headers. Metadata documentation must state the resulting exposure.
- `MessageRequest` HPKE pins `draft-ietf-hpke-pq-05` MLKEM768-X25519 + HKDF-SHA256 + ChaCha20Poly1305 in base mode.
  Later incompatible standardization creates a new profile/version rather than silently changing v1.
- v1 claims **post-quantum handshake / harvest-now-decrypt-later protection**, not PQ post-compromise healing.
