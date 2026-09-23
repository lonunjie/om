# Production Crypto Integration Gate

## Status

**BLOCKING for any production-security claim.** The rest of OM may be prototyped with an unmistakable dev-only provider.

## 1. Current reviewed direction (September 2026)

- ML-KEM is standardized in NIST FIPS 203; NIST has published implementation/use guidance in SP 800-227.
- ML-DSA is standardized in FIPS 204; SLH-DSA is standardized in FIPS 205.
- NIST has selected HQC as an additional KEM for future standardization, but it is not an OM production primitive until a stable standard/profile review exists.
- Signal's PQXDH remains a valuable asynchronous PQ reference but its published revision still relies on classical discrete-log assumptions for mutual authentication.
- Signal's current Double Ratchet specification describes hybrid Triple Ratchet and recommends ML-KEM Braid as the SCKA; ML-KEM Braid has formal analysis and is a major reference for ongoing PQ forward secrecy/post-compromise recovery.
- Signal's `libsignal` repository explicitly states use outside Signal is unsupported, so OM must not treat its private API/serialization as a stable protocol dependency.
- MLS base protocol/architecture are RFC 9420/RFC 9750. PQ/hybrid MLS ciphersuites are still an active IETF Internet-Draft as of July 2026.
- OpenMLS 0.9.x is a strong Rust candidate and has undergone an external security audit, but library review/pinning and PQ feature maturity remain required.

## 2. Do not invent OM cryptography

Forbidden without a separate research/security-review project:

- new KEM/signature/ratchet primitive;
- ad-hoc hybrid combiner;
- ad-hoc transcript authentication;
- custom random-number generator;
- custom AEAD;
- undocumented key derivation/domain separation.

Protocol composition must follow reviewed specifications/papers and obtain independent review before production.

## 3. CryptoProfile production checklist

A profile cannot move from `experimental` to `allowed/preferred` until:

- all primitive standards/spec revisions are pinned;
- current errata are reviewed (notably NIST PQC publication errata notices);
- implementation libraries have an active maintenance/security story;
- license/use model is acceptable;
- known-answer tests and upstream vectors run in CI;
- OM composition/transcript/domain-separation spec is complete;
- downgrade floor/migration is specified;
- key sizes/serialization/parser bounds are specified;
- device vault/key-erasure behavior is documented honestly;
- independent cryptographic review is scheduled/completed for first production release.

## 4. Candidate 1:1 evaluation, not a frozen cipher list

Evaluate two implementation strategies:

### A. Reviewed reusable secure-messaging implementation

If a stable supported library emerges for PQXDH/Triple-Ratchet-class messaging, prefer wrapping it behind OM's session interface.

### B. OM-maintained implementation of public reviewed specs

Only if no maintainable library exists. This is protocol implementation work, not primitive invention, but it requires substantially stronger tests/audit and must not be rushed.

`libsignal` may be used as a behavioral/reference source/test oracle where licensing permits, but its unsupported external API is not the OM compatibility contract.

## 5. Durable signatures

Durable account/control/discovery objects need PQ-aware signature migration. A hybrid classical+PQ durable credential strategy is expected initially, but exact algorithms belong to CryptoProfile.

Ordinary conversation messages SHOULD remain authenticated by the secure session/group protocol without adding a separate durable identity signature to every message; see evidence/deniability policy.

## 6. Groups

Implement base MLS first behind `GroupCrypto`. PQ MLS draft suites remain experimental until their standard/profile identifiers and chosen library support are judged stable enough. Group state must support authenticated reinit/migration.

## 7. Symmetric/content encryption

Use established AEAD/KDFs through the profile. 256-bit symmetric keys are a reasonable conservative target for long-lived content/backups, but exact primitive choice remains profile-owned. Never reuse a nonce with the same key; tests must make nonce construction explicit.

## 8. Recovery/passphrases

Recovery roots are random high-entropy secrets, not passwords. If a user explicitly protects local/recovery material with a memorized passphrase, use a reviewed memory-hard password KDF (Argon2id is the current CFRG reference in RFC 9106) with versioned parameters calibrated to device class. Never reduce a random 256-bit recovery root to password entropy merely for convenience.

## 9. Security language

Engineering/release UI must distinguish:

- dev/test crypto;
- experimental CryptoProfile;
- reviewed/allowed CryptoProfile.

Never describe experimental/draft composition as finalized PQ security.
## 2026-09-22 hardening additions

Production CryptoProfile evaluation now includes two architecture layers that cannot be delegated vaguely to a library:

1. [`ASYNC_KEY_MATERIAL.md`](ASYNC_KEY_MATERIAL.md) — publication/fetch/depletion/rotation and safe multi-provider semantics for prekeys/KeyPackages;
2. [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](ONE_TO_ONE_SESSION_MANAGEMENT.md) — multi-device active-session selection, crossed initiation, device changes, backup/rollback and logical-message fanout.

A candidate crypto library is insufficient if it cannot expose/support these boundaries across native + WASM targets without relying on an irreplaceable central server model.

