# ADR-0118 — Dependency, licensing and update strategy for the production crypto path

- **Status:** Accepted. §2 resolved as option (d): OM-Crypto-v1 ships without SPQR; a clean-room SPQR (b) is the planned
  v2 path.
- **Decided by:** operator, 2026-09-24 ("go with v1 without SPQR ... (d) for OM-Crypto-v1, with (b) explicitly planned
  as the v2 path"). Option (a), asking Signal to relicense, was ruled out as not realistic.
- **Date:** 2026-09-24
- **Source:** V-001 feasibility prototype ([`verification/prototypes/v001-crypto/`](../verification/prototypes/v001-crypto/README.md)),
  drafted by a Claude agent and filed by the integrator.
- **Blocks:** design-queue items A-023..A-028; V-001 criteria 4-5.

**Context.** V-001 needs the OM-Crypto-v1 1:1 path (PQXDH + Triple Ratchet/SPQR) to rest on maintained libraries whose licences fit the policy in [`LICENSING_AND_CONTRIBUTION_POLICY.md`](../docs/constitution/LICENSING_AND_CONTRIBUTION_POLICY.md) (om-core MPL-2.0). It must also not rely on an unsupported external API contract. This prototype shows the path works on native and wasm32 using the libraries below.

**Decision (proposed).**

1. **Primitive libraries** (all permissive, all build for native, wasm32-wasip1 and wasm32-unknown-unknown):
   - X25519 and XEdDSA group operations: `x25519-dalek` / `curve25519-dalek` (BSD-3-Clause).
   - ML-KEM-768: `libcrux-ml-kem` (Apache-2.0, formally verified with hax/F*). RustCrypto `ml-kem` (Apache-2.0 OR MIT) is the named fallback and cross-check.
   - HKDF, HMAC, SHA-2 and AES-256-GCM-SIV: RustCrypto (MIT OR Apache-2.0).
   - Deterministic CBOR: OM's own strict codec (V-005).
   - Protocol composition (PQXDH, EC Double Ratchet, Triple Ratchet framing, prekey store): OM-maintained code implementing Signal's public-domain specifications (both PQXDH and the Double Ratchet document place themselves in the public domain). This is strategy B of [`PRODUCTION_CRYPTO_INTEGRATION_GATE.md`](../docs/architecture/crypto/PRODUCTION_CRYPTO_INTEGRATION_GATE.md) §4, because no supported library exists (see strategy A below).
2. **SCKA / SPQR licence conflict: must be resolved before engineering.** The only SPQR implementation is Signal's `spqr`, which is AGPL-3.0-only. Linking it into om-core would make every distributed client binary an AGPL work, which contradicts the MPL-2.0 core policy (and has app-store distribution implications). Options, in order of preference:
   - (a) Ask Signal for an MPL/Apache relicence or a linking exception for `spqr`.
   - (b) Write an OM-maintained **clean-room** implementation of ML-KEM Braid + SPQR from Signal's published specifications. Implementers must not read the `spqr` source. `spqr` then serves only as an interoperability **test oracle** in a separate, non-distributed AGPL test harness. This needs its own independent review (a new high-risk component).
   - (c) Change om-core's licence (constitutional change; not recommended).
   - (d) Ship OM-Crypto-v1 without SPQR, as PQXDH (X25519 + ML-KEM-768) + the classic Double Ratchet. Signal shipped this
     construction from 2023 to 2025.

   **Decision: (d) for v1, with (b) as the planned v2 path.**
   - v1 keeps post-quantum protection against harvest-now-decrypt-later for the whole session, because every ratchet
     key descends from the hybrid PQXDH secret. v1 does **not** give post-quantum post-compromise healing: after a
     device's state is stolen, the classical DH ratchet heals against classical attackers only. A new session
     (a fresh PQXDH) restores post-quantum secrecy.
   - v2 (`OM-Crypto-v2`) adds SPQR/ML-KEM Braid as an OM-maintained clean-room implementation from Signal's published
     specifications. The implementers never read the `spqr` source, and `spqr` is used only as a test oracle in a
     separate, non-distributed AGPL harness. The v2 work item includes specifying OM's SCKA wire format, an independent
     crypto review, and the v1→v2 upgrade through the downgrade-floor and profile-migration rules.
   - Option (a) is ruled out. Option (c) is rejected.
3. **Stable contract is OM's, not a library's.**
   - Library types never cross the `CryptoProvider`/`Ratchet` interface.
   - Persisted state is OM CBOR. Library state is embedded only as a versioned opaque field with a migration story.
   - Every wire byte (including the SCKA message) must be specified in an OM spec document with vectors, so a replacement implementation can be verified against it.
   - libsignal is not a dependency; its README declares external use unsupported. It may be a behavioural reference.
4. **Update and CVE process.**
   - Pin exact versions (`Cargo.lock`, [`rust-toolchain.toml`](../verification/prototypes/v001-crypto/rust/rust-toolchain.toml), `--locked`), and review every bump.
   - Watch RustSec/GitHub advisories and the upstream security contacts (Signal, Cryspen/libcrux, RustCrypto, dalek), with `cargo-audit` and `cargo-deny` in CI. `cargo-deny` gets a licence allow-list that rejects GPL/AGPL in core crates, plus bans on duplicate crypto implementations.
   - Every update must pass: the NIST ACVP KATs, OM's own vectors, the cross-implementation (second-language) vectors, and the downgrade/one-time-key negative tests.
   - A critical advisory is answered with a patched release and, where needed, a signed SecurityAdvisory that raises peer-local floors. There is no remote kill switch.
5. **Production exclusion of test providers (R-072).**
   - Deterministic and test RNGs and providers live only behind an `insecure-*` cargo feature.
   - Enabling that feature together with `production` is a compile error.
   - Release CI builds with `--locked`, checks that no test-provider symbol is present in release artifacts, and publishes reproducible build hashes.
6. **Downgrade invariants in the profile:** a PQ prekey is mandatory in every bundle; the ML-KEM ciphertext is mandatory in every initial message; one-time keys are consumed only after the initial AEAD authenticates.

**Consequences.** V-001 criteria 4 and 5 are no longer blocked by licensing: the v1 path uses only permissive crates plus OM-maintained code. The clean-room SPQR and its SCKA wire format are a v2 work item, not a v1 gate. ADR-0107 stays true (1:1 is hybrid-PQ against harvest-now-decrypt-later); only the construction name changes.
