# Software Supply Chain and Updates

## 1. Principle

> **Move fast on features. Do not move fast on the trust root.**

Open source is necessary but not sufficient. A clean public repository does not protect users if CI, dependencies, signing keys, package registries, or app-store releases are compromised.

Software distribution is part of OM's effective security boundary because a malicious client update can bypass every protocol guarantee at the endpoint.

## 2. Source hosting is not the trust root

OM should use ordinary Git and mirror automatically across independent forges where practical:

- GitHub;
- GitLab/Codeberg/self-hosted Git;
- downloadable signed Git bundles/source archives.

Canonical release identity is a cryptographically signed release object/commit/tag plus its release metadata—not a particular hosting domain.

A decentralized-Git system may be added later, but it is not required to avoid forge lock-in.

## 3. Minimum foundation from the first public release

Do these early because retrofitting them later is painful:

1. protected main/release branches and mandatory review;
2. signed release tags/releases;
3. dependency lockfiles and cryptographic pinning where ecosystems permit;
4. CI actions/images pinned by digest, not floating tags;
5. least-privilege CI credentials and short-lived/OIDC credentials where possible;
6. isolated/ephemeral release builds;
7. SBOM and dependency inventory;
8. build provenance using SLSA/in-toto-compatible attestations;
9. update metadata with rollback/freeze protection;
10. offline/threshold root authority separate from ordinary online release automation.

Expensive formal audits and multiple independent build farms can follow later; the trust-root shape should not.

## 4. TUF-style update trust

OM should use The Update Framework concepts or a compatible implementation rather than one permanent online signing key.

Target shape:

```text
offline root role: threshold (for example 2-of-3)
       |
       +-- defines/rotates trusted online roles and expiry

online release/targets role
       +-- signs authorized artifacts only

snapshot/timestamp roles
       +-- freshness / rollback / freeze resistance
```

Trust expires and is compartmentalized. Compromising one online release secret must not permanently grant root authority.

## 5. Provenance and reproducibility

Produce provenance that binds an artifact digest to:

- exact source revision;
- build recipe/workflow;
- builder identity;
- resolved dependencies where practical;
- outputs.

SLSA provenance/in-toto are appropriate existing frameworks.

Make Android/desktop/server builds reproducible where technically feasible and publish a simple rebuild/compare procedure. Independent rebuilders should be able to attest that a release artifact corresponds to source.

Platform caveats must be honest:

- Android independent APK distribution can target strong reproducibility;
- app-store signing/repackaging can complicate byte-identical comparison;
- iOS distribution is constrained by Apple's signing/build ecosystem, so provenance/source mapping and independent source review remain important even when byte-identical App Store reproduction is not possible;
- a PWA receives executable code dynamically and therefore has a different/higher update-channel trust exposure than an installed independently verifiable native build.

High-assurance users may prefer independently verifiable installed clients.

## 6. Release transparency

Publish append-only release metadata/artifact hashes and permit independent monitors. A transparency service detects equivocation; it must not itself gain release-signing authority.

Clients should remember a minimum observed release/update metadata version to prevent rollback where platform constraints permit.

## 7. Emergency response

Pre-plan:

- signing-key rotation;
- compromised CI isolation;
- revocation of release metadata keys;
- emergency client update;
- public incident metadata;
- independent verification path.

Do not invent emergency bypass keys that silently skip normal authorization.

## 8. OM protocol governance independence

A compromised official client must not redefine protocol truth for everyone. Public protocol specs, CryptoProfile registry history, conformance tests, and third-party implementations provide an independent reference.
