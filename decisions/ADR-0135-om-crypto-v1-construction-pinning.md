# ADR-0135 — OM-Crypto-v1 construction pinning (closes the V-001 constructions)

- **Status:** Accepted for verification; independent review pending (the executor wrote it, so ADR-0117 requires a non-Anthropic reviewer before any gate can count it as reviewed)
- **Date:** 2026-09-25
- **Decided by:** verification executor (Claude Opus 5.5) under ADR-0134. It fills the constructions that [`om-domain-separation.json`](../spec/registries/om-domain-separation.json) leaves to "the CryptoProfile ADR that closes V-001" and does not change any rule of ADR-0131.
- **Amends:** ADR-0118 (dependency set only), ADR-0131 (encodings only)
- **Evidence:** [`verification/prototypes/v001-crypto/`](../verification/prototypes/v001-crypto/README.md), [`verification/results/v001-crypto-latest.json`](../verification/results/v001-crypto-latest.json)

## Context

ADR-0131 froze the OM-Crypto-v1 rules: a dual Ed25519 + ML-DSA-65 device binding, an exact last-resort replay ledger, the AES-256-GCM-SIV payload derivation, no header encryption and the draft-ietf-hpke-pq-05 MessageRequest suite. It did not fix the bytes. The frozen W0 CDDL (ADR-0119) has one `signing_public_key` field per DeviceCredential and one `signature` bstr per signature entry, and the domain-separation registry leaves four constructions `reserved` for V-001. Two independent implementations cannot interoperate until these bytes are fixed. This ADR fixes them inside the frozen CDDL and the existing labels, and adds one label.

## Decision

### 1. Composite DeviceIdentity key and signature (profile 1)

- `DeviceCredential.signing_public_key` (key 5) = Ed25519 public key (32 bytes) ‖ ML-DSA-65 verification key (1952 bytes): exactly 1984 bytes.
- `signature.3` = Ed25519 signature (64) ‖ ML-DSA-65 signature (3309): exactly 3373 bytes.
- Both halves sign the same DS-SIG bytes. Ed25519 follows RFC 8032 with strict verification (canonical S, and no small-order A or R). ML-DSA-65 is FIPS 204 pure `ML-DSA.Sign` with an empty context string. Production signing uses hedged randomness; test vectors may fix `rnd`.
- A verifier accepts only if both halves verify. Any other length is rejected. A signed object whose verifier expects a device signature must carry **exactly one** signature entry for that (signer, profile, role). Two partial entries are rejected, so neither half can be stripped, split off or substituted.
- Both keys are stored as 32-byte seeds (the Ed25519 secret key; the ML-DSA-65 `KeyGen` seed ξ).

### 2. PQXDH binding over the frozen objects

The `PQXDHDeviceBinding` of ADR-0131 rule 1 is the following chain. It uses only frozen W0 objects and adds no new signed-object type:

1. The verifier holds a trusted `AccountControlState` head (from V-002 processing): the ACS body whose `OM-ACS-HASH-v1` digest it already trusts.
2. The peer's `DeviceCredential` is accepted only if its `OM-DEVICE-CREDENTIAL-HASH-v1` digest appears with its DeviceID in ACS key 13, its AccountID equals the ACS AccountID, and the DeviceID is in ACS key 5 or 7 and in neither key 6 nor key 8. The credential's key 6 (`key_agreement_public_key`) is the PQXDH X25519 identity key IK. Key 5 is the composite key of §1.
3. The `signed-AsyncKeyBundle` must carry exactly one composite device signature under `OM-PREKEY-BUNDLE-SIG-v1`. Its body must name the same DeviceID, profile 1, the trusted ACS `generation` and `state_hash` (keys 3 and 4), and an `expires_at` in the future. Key 6 (`generation`) is the **signed-prekey epoch**. Key 7 is the EC signed prekey (kind 1). Key 8 is the ML-KEM-768 signed prekey (kind 2), which is also the PQXDH last-resort key. A bundle without key 8 is refused.
4. One-time prekeys (kinds 3 and 4) travel as individually signed `prekey-item`s. Key 3 holds a composite signature under `OM-PREKEY-SIG-v1` over the item without key 3. A Home fetch (`KeyBundleFetchResponse`, `[* bstr]`) returns `[signed bundle, EC one-time item?, PQ one-time item?]`, so a provider can hand out each item once without re-signing anything.

This replaces PQXDH §3.2's XEdDSA prekey signatures by IK. IK is still the PQXDH identity key and is authenticated through the ACS-committed credential. Every PQXDH public object that can redirect a session is covered by both signature families, as ADR-0131 rule 1 requires.

### 3. `handshake_id` and ledger applicability

`handshake_id = DS-HASH("OM-PQXDH-HANDSHAKE-ID-v1", OM-CBOR [profile, recipient_device_id, signed_prekey_epoch, spk_id, pq_id, IK_A, EK_A, CT])`.

The ledger applies exactly when an initiation consumes **no** one-time key of either kind. If an initiation consumes a one-time key, deleting that key already prevents replay, so it does not use the 4096 allowance. The recipient checks the ledger before any expensive work. It records the id only after the initial AEAD authenticates. It then commits the whole device state (ledger included) to DeviceVault before plaintext is released. A failed commit releases nothing. An epoch's ledger is deleted only together with that epoch's signed-prekey secrets.

### 4. Double Ratchet profile

- PQXDH `info` = `OM-Crypto-v1_PQXDH_X25519_SHA-256_ML-KEM-768`; EncodeEC type byte 0x05; `AD = EncodeEC(IK_A) ‖ EncodeEC(IK_B)`.
- The PQXDH `SK` is the initial Double Ratchet root key (DR rev 4 §3.3). Bob's signed prekey is his first ratchet key pair (§7.1). There is no SPQR split in v1.
- `KDF_RK(rk, dh) = HKDF-SHA256(salt = rk, ikm = dh, info = "OM-1TO1-RATCHET-KDF-v1", 64)`. `KDF_CK` is DR §5.2 (HMAC constants 0x01 and 0x02; no info string). `MAX_SKIP` is 1000 per chain, with at most 2000 stored skipped keys per session.
- `header = OM-CBOR [1, dh_pub: bstr .size 32, pn, n]`, not encrypted (ADR-0131 rule 4).
- `header_hash = DS-HASH("OM-DR-HEADER-HASH-v1", header)` (new label).
- `K`, `N` exactly as in ADR-0131 rule 3, with `header_hash` as above.
- `AD = OM-CBOR ["OM-1TO1-EVENT-AEAD-AD-v1", 1, session_AD, header_hash, envelope_context]`. `envelope_context` is the immutable transport context of the message: the canonical bytes of the envelope fields that precede the ciphertext. For the V-001 envelope that is `[sender EncodeEC(IK), PQXDH prefix or null]`.

### 5. MessageRequest HPKE binding

`info = OM-CBOR ["OM-MESSAGE-REQUEST-HPKE-INFO-v1", crypto_profile, ticket_id, request_key_digest]`. The digest uses the common `digest` struct. `aad` is empty. There is one `Seal` per MessageRequest, at sequence 0. The suite is exactly the draft-ietf-hpke-pq-05 construction pinned by ADR-0131 rule 9. The implementation must pass that draft's published vectors from hpkewg/hpke-pq tag `draft-ietf-hpke-pq-05` (commit `6433c8f`, `test-vectors.json` SHA-256 `35c59f4a…613c`).

### 6. Dependency set (amends ADR-0118)

The v1 path uses crates.io releases through their public APIs only: `libcrux-ml-kem` and `libcrux-ml-dsa` (formally verified, Apache-2.0); `x25519-dalek`, `curve25519-dalek` and `ed25519-dalek` (BSD-3-Clause); and RustCrypto `aes-gcm-siv`, `chacha20poly1305`, `hkdf`, `hmac`, `sha2`, `sha3` and `shake` (MIT/Apache-2.0). The graph contains no copyleft crate and no git dependency. Signal's `spqr` and `libsignal` are no longer used by any v1 artifact. Update strategy stays as in ADR-0118: pinned lockfile, and upstream KATs rerun on every bump.

### 7. Responder-side initiator binding and the composed authentication argument (2026-09-26, ADR-0136)

Review F-9 found §2 asymmetric: it pinned the chain that authenticates the recipient's `IK_B`, while the responder
accepted any `IK_A` that matched the PQXDH header. ADR-0136 rule 4 closes it. Before `accept_initial` performs any
secret-key operation, `IK_A` must be the `key_agreement_public_key` of exactly one DeviceCredential committed by the
trusted ACS head of the relationship that delivered the envelope, and that device must be active and not quarantined.
Otherwise the initiation fails closed and nothing is consumed.

The resulting party-authentication argument is a composition, not XEdDSA-PQXDH's original proof:

1. **ACS/credential authenticity.** Each peer's trusted ACS head (V-002) commits the peer's DeviceCredential by
   digest, and the credential carries both the composite signing key and the X25519 identity key.
2. **Mandatory dual signatures.** The recipient's signed prekeys verify under the committed composite key, and both
   Ed25519 and ML-DSA-65 halves are required (§1). This replaces the XEdDSA prekey signature.
3. **PQXDH key establishment** over identity keys that (1) binds on *both* sides: the initiator binds `IK_B` through
   §2, and the responder binds `IK_A` through rule 4.

PQXDH's authentication guarantee applies to the OM identities only when all three hold. Removing any one of them is a
conformance failure.

## Consequences

- [`om-domain-separation.json`](../spec/registries/om-domain-separation.json) fills the four reserved constructions and adds `OM-DR-HEADER-HASH-v1`.
- V-001 criteria 4–9 have executable evidence. V-005 and V-015 consume the same byte layouts.
- A later profile (for example OM-Crypto-v2 with SPQR, or header encryption) gets a new profile id and never reinterprets these bytes.
- Review question for the independent reviewer: does §1–§4 faithfully implement ADR-0131 rules 1–3 and 9 without weakening PQXDH's security argument, given that §2 replaces XEdDSA by a separately certified composite key?
