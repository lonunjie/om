# Security / Cryptography Re-audit — 2026-09-22

This is an engineering-time re-audit of the major standards/references OM relies on. External standards evolve; recheck official sources before a production profile/release.

## Post-quantum standards

### NIST FIPS 203 — ML-KEM

Final since 2024-08-13. NIST currently lists an errata/potential-update note dated 2025-11-17. Engineering MUST consume current errata/current official vectors, not implement from a stale PDF transcription.

Source: https://csrc.nist.gov/pubs/fips/203/final

### NIST FIPS 204 — ML-DSA

Final since 2024-08-13. NIST lists minor potential updates/errata as of 2026-07-31. Same implementation rule: current standard + errata + KATs.

Source: https://csrc.nist.gov/pubs/fips/204/final

### NIST FIPS 205 — SLH-DSA

Final hash-based PQ signature standard. It is valuable algorithm diversity/fallback material but has very different size/performance properties; not assumed as OM's default messenger signature.

Source: https://csrc.nist.gov/pubs/fips/205/final

### NIST SP 800-227 — KEM recommendations

Final 2025-09-18. Use as implementation/use guidance around KEM composition and validation.

Source: https://csrc.nist.gov/pubs/sp/800/227/final

### HQC

NIST selected HQC in 2025 as an additional KEM for standardization; the final FIPS is still future work at this checkpoint. Reserve crypto-agility path; do not ship it as an OM standard merely because it was selected.

Source: https://csrc.nist.gov/projects/post-quantum-cryptography/post-quantum-cryptography-standardization/selected-algorithms

## 1:1 secure messaging references

### Signal PQXDH

Useful, analyzed asynchronous PQ-capable initial agreement. Current published revision explicitly provides PQ forward secrecy while mutual authentication still relies on classical discrete-log assumptions. OM credential/control authentication therefore cannot say “PQXDH makes the whole identity stack post-quantum.”

Source: https://signal.org/docs/specifications/pqxdh/

### Signal Double Ratchet / Triple Ratchet

Current Signal specification includes Sparse Post-Quantum Ratchet / Triple Ratchet constructions and recommends ML-KEM Braid for SCKA in the hybrid direction.

Source: https://signal.org/docs/specifications/doubleratchet/

### ML-KEM Braid

Published by Signal, built on standardized ML-KEM, with formal analysis/models. Important reference for PQ PCS/forward-security engineering, including bandwidth and “vulnerable message set” considerations.

Source: https://signal.org/docs/specifications/mlkembraid/

### libsignal maintenance warning

Signal's own README says use outside Signal is unsupported and APIs/bridge layers can change. OM may learn from/test against it but must not silently make `libsignal` private APIs the stable OM protocol boundary.

Source: https://github.com/signalapp/libsignal

## Groups

### MLS

RFC 9420 remains the base MLS protocol. RFC 9750 is the architecture reference.

Sources:
- https://www.rfc-editor.org/rfc/rfc9420
- https://www.rfc-editor.org/rfc/rfc9750

### PQ MLS

`draft-ietf-mls-pq-ciphersuites-06` (2026-07-21) is active work and includes ML-KEM/hybrid suites. It is still an Internet-Draft, so OM must not fossilize draft identifiers as permanent protocol truth.

Source: https://datatracker.ietf.org/doc/draft-ietf-mls-pq-ciphersuites/

### OpenMLS

0.9.0 was released in August 2026 and the project reports an external SRLabs security audit. Strong candidate for base MLS; still wrap storage/serialization and crypto profile boundaries so library state formats are not OM wire formats.

Source: https://blog.openmls.tech/posts/2026-08-25-0.9.0-release/

## Media

SFrame is standardized in RFC 9605 and remains the correct architectural direction for media E2EE independent of P2P/TURN/SFU routing.

Source: https://www.rfc-editor.org/rfc/rfc9605

## Crypto agility

NIST CSWP 39upd1 is final (2026-06-29) and explicitly treats replaceability/adaptation of cryptography while preserving operation as a core requirement. OM's algorithm-independent AccountID + versioned CryptoProfiles + downgrade floors align with this direction.

Source: https://csrc.nist.gov/pubs/cswp/39/upd1/considerations-for-achieving-crypto-agility/final

## Encoding

RFC 8949 defines deterministic CBOR requirements and explicitly warns that decoders must treat even signed/encrypted input as hostile. OM engineering adopts a narrower deterministic-CBOR profile, parser limits and canonical rejection before signatures/wire formats freeze.

Source: https://www.rfc-editor.org/rfc/rfc8949

## Password-derived material

RFC 9106 defines Argon2 and requires Argon2id support. Use only for user-memorized passphrase-derived keys with calibrated/versioned parameters; do not replace random recovery entropy with a password.

Source: https://www.rfc-editor.org/rfc/rfc9106

## Web endpoint security

WebAuthn Level 3 became a W3C Recommendation on 2026-08-25. Use WebAuthn capabilities (including PRF where implemented) as a local vault/unlock building block, not OM account identity.

Source: https://www.w3.org/TR/webauthn-3/

Trusted Types remains a W3C Working Draft (2026-06-23 at this checkpoint). It is worthwhile PWA defense-in-depth but MUST NOT be treated as the sole XSS/content security boundary.

Source: https://www.w3.org/TR/trusted-types/

## Software supply chain

TUF remains relevant for rollback/freeze/key-compromise-aware update metadata. SLSA 1.2 is current and its v1 provenance predicate is approved. in-toto stable v1.0 remains suitable for signed supply-chain statements.

Sources:
- https://theupdateframework.io/docs/security/
- https://slsa.dev/spec/v1.2/
- https://slsa.dev/spec/v1.2/build-provenance
- https://in-toto.io/docs/specs/

## Hostile content / Unicode

OWASP file-upload guidance reinforces allowlisting, actual type validation, size/decompression limits, parser isolation and treating files as malicious input.

Source: https://cheatsheetseries.owasp.org/cheatsheets/File_Upload_Cheat_Sheet.html

Unicode UTS #39 Version 18.0.0 / Revision 34 (2026-08-27) is the current stable security-mechanisms reference for confusable/mixed-script identifier detection.

Source: https://unicode.org/reports/tr39/

Signal's link-preview design remains useful prior art: a privacy-enhancing proxy can hide the user's IP from the destination while preventing the proxy from learning the plaintext URL through end-to-end TLS tunneling semantics.

Source: https://signal.org/blog/i-link-therefore-i-am/

## Re-audit conclusion

No reviewed source justifies weakening OM's architecture. The key adjustments for engineering are:

1. standards/profile agility must remain real because PQ standards and errata continue to evolve;
2. don't build OM compatibility around unsupported `libsignal` APIs;
3. base MLS can proceed, but PQ MLS remains experimental;
4. formal/control-state verification and hostile-input hardening deserve equal priority with crypto;
5. production claims wait for independent review and vectors, not merely “we used strong algorithms.”
