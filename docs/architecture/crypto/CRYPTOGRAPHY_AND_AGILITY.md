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

Recommended logical form:

```text
AccountID = random 256-bit stable identifier

AccountControlState(epoch N):
  AccountID
  CredentialSet
  authorized devices
  recovery policy commitment
  supported CryptoProfiles
  previous-state commitment / continuity proof
  authenticated transition
```

The exact canonical encoding/transition signature construction remains a Gate-C/A detail, but the invariant is fixed: replacing cryptography does not replace AccountID.

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
  minimum_client_generation?
}
```

Peers MUST NOT assemble arbitrary combinations in production.

## PQ-first, hybrid-secure default

Where mature and interoperable, OM should combine a classical and post-quantum assumption instead of betting the entire system on one new primitive family.

Conceptual profile:

```text
OM-PQHYBRID-1
  account/device authentication: classical + PQ-capable authenticated credential transition
  async 1:1 key establishment: PQXDH-class or equivalent hybrid-reviewed design
  ongoing 1:1: Triple-Ratchet-class hybrid continuous key evolution
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

Each relationship/device/group records the strongest accepted security generation/profile family according to policy.

Conceptually:

```text
security_floor(peer) = generation G

if proposed_generation < G and no explicit authenticated migration exception:
    reject
```

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

Official client update metadata should eventually support hybrid/classical+PQ verification so a future quantum break of a legacy signing algorithm does not become a software-supply-chain break. Release-key migration needs the same continuity/downgrade discipline as user credentials.

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

Recovery mechanisms use versioned `RecoveryPolicy` methods rather than embedding one mnemonic/guardian algorithm forever. A 256-bit random offline recovery root is the baseline candidate. Shamir mnemonic shares are feasible prior art; future standardized threshold/MPC authorization can replace them without changing AccountID.

NIST's 2026 crypto-agility guidance reinforces the architectural requirement that algorithms be replaceable while operations continue. NIST's 2026 threshold call is a reason to reserve the future threshold interface rather than invent one prematurely.

## 2026-09-22 engineering re-audit

Production integration is now explicitly gated by `PRODUCTION_CRYPTO_INTEGRATION_GATE.md` and `research/SECURITY_CRYPTO_REAUDIT_20260922.md`.

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

CryptoProfile v1 must specify not only primitives/handshake/ratchet but also the public logical shape and lifecycle of asynchronous per-device handshake material. `ASYNC_KEY_MATERIAL.md` is canonical architecture; runtime implementation proof is tracked separately.

Multi-device session selection is outside the ratchet itself and is defined by `ONE_TO_ONE_SESSION_MANAGEMENT.md`. No production security claim is complete until both layers have conformance vectors and stale/replay/depletion/device-change tests.

## 2026-09-23 OM-Crypto-v1 architecture resolution

Q-001/Q-044 are no longer open design choices. OM-Crypto-v1 targets the published PQXDH + Triple-Ratchet/SPQR-class construction with X25519 + ML-KEM-768 for asynchronous hybrid establishment, classical PQXDH authentication semantics as currently specified, HKDF/SHA-256 family derivation, and misuse-resistant authenticated encryption. Durable constitutional authentication uses versioned dual classical/PQ signatures where the AccountControl/CryptoProfile requires it. The remaining production-library/runtime work is `V-001`, a proof gate—not permission to substitute a different wire protocol during implementation.
