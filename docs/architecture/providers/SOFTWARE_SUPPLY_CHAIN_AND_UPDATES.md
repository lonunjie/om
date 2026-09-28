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

### V1 signature-algorithm agility (ADR-0148, hardened by ADR-0149)

The first-public-release metadata/verifier format supports **algorithm-tagged multiple signatures**, but algorithms are not authority by themselves. The already-trusted owner-pinned root policy binds each metadata role to authorized key IDs/public keys, algorithms and distinct-key thresholds. Verification is over the exact canonical metadata bytes plus the declared distribution/role/root-policy version. A right algorithm from an unauthorized/wrong-role key, a duplicate signature from one key, or the same key ID paired with a different public key/algorithm does not satisfy policy.

An optional additional co-signature may be ignored only while no current trusted threshold counts its key/algorithm/role; once an authenticated root-policy transition requires it, stripping it or rolling back to metadata that predates the requirement fails under the same rollback/freeze rules. Unknown required algorithms fail closed. Root transition uses TUF-style continuity: the next root is accepted through the previously trusted root threshold and its own new-root threshold, and long-neglected verifiers authenticate intermediate root versions rather than skipping to an attacker-selected later version.

The reference distribution is ML-DSA-65 co-signature-ready for long-lived root and release/targets metadata and SHOULD emit such co-signatures once a maintained reviewed implementation fits the release toolchain. This does not make ML-DSA a universal OM distribution requirement, does not couple software-signing keys to AccountID/protocol identity, and does not require every online freshness role to use PQ signatures initially.

**The SecurityAdvisory key is a delegated role (ADR-0130 rule 3).** A distribution's advisory signing key is a TUF
role delegated from the same root, with its own expiry, rotation and revocation, and it is listed in the custody map
(PROJECT_STEWARDSHIP §5). It can sign advisories and a `min_release` floor for the distribution's own clients. It
cannot on its own sign a **peer-affecting** floor (a minimum CryptoProfile or protocol generation that would put
relationships with other distributions' users into "Needs update"): that needs the root threshold
([`VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md`](VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md) §6).

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

Roles (ADR-0130 rule 5, R-274; A-046 was resolved by ADR-0133 with the frozen release-transparency profile):

- **Release-transparency log.** Each distribution appends every release's metadata digest and artifact digests to at
  least one public append-only log with signed checkpoints. Anyone may run a log; a distribution may use several. A
  log is a witness to what was published, never a signer of releases.
- **Monitor.** Any independent party may run one. A monitor follows one or more logs, checks checkpoint consistency
  (append-only, no split view), and publishes signed observations `(release digest, log id, index, checkpoint)`.
  Monitors are **optional and non-authoritative**: a monitor can report "seen" or "inconsistent"; it can never
  approve, block or sign a release. Distributions list known monitors as replaceable data, never as protocol state.
- **Consumers.** A ClientHost deployment controller requires observations from at least two independent monitors
  before automatic deploy (CLIENTHOST §4 rule 2). A running client compares its release digest with at least two
  observations when online (R-164).
- **Failure behavior.** Fewer than two observations: the controller does not auto-deploy and waits or falls back to
  approval-required mode; the client shows a non-blocking "couldn't check this app version yet" state, not an alarm.
  Conflicting observations or a digest no monitor has seen: a security event ("The app you are running does not
  match any published release") with a path to move to another origin or a native client. A monitor outage never
  stops messaging.

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

## 2026-09-22 distribution separation

Update trust belongs to a **client distribution**, not to the OM protocol as a universal authority.

The reference distribution should still use the strongest practical provenance/reproducibility/threshold-update design. However, another compatible distribution may have its own independently auditable signing/update root. Protocol peers authenticate protocol state, not the brand of the software binary.

Creator-disappearance requirement: loss of the reference project's update keys must not make existing independent deployments unable to communicate or prevent a fork from continuing under new distribution keys.

## PWA executable-origin limitation

TUF/provenance/reproducible assets improve auditability and update discipline, but a web origin that is fully compromised can serve a malicious bootstrap/service worker. Do not describe application-level signature verification as eliminating this browser execution trust root.

Reference PWA therefore also requires a ClientHost the user can trust (their own origin for T2/T3, a shared ClientHost under [`CLIENTHOST_UPDATE_LIFECYCLE.md`](../client/CLIENTHOST_UPDATE_LIFECYCLE.md) §9, and for T0/T1 users never the sponsor's or host's origin by default: [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md) §3.5, §4.2), no remote runtime dependency/CDN, strict CSP, and a documented native higher-assurance option. SLSA 1.2/in-toto provenance should be generated from the start of public builds rather than retrofitted after release.
## 2026-09-22 hardening — self-owned ClientHost deployment lifecycle

[`CLIENTHOST_UPDATE_LIFECYCLE.md`](../client/CLIENTHOST_UPDATE_LIFECYCLE.md) is canonical. Signed release metadata is necessary but not sufficient for a fleet of independently owned PWA origins: OM must also define deployment/staging/service-worker/rollback/emergency-update and compromised-origin migration behavior.

Same-origin JavaScript is not treated as an independent verifier after total origin compromise. Recovery to a trusted ClientHost/native endpoint uses authenticated device enrollment/migration.

## 2026-09-23 S0 release-root resolution

The founder may remain the only human steward during S0 while still using a physically separated 2-of-3 offline root-key arrangement under founder custody. This improves key-loss/online-compromise resilience without pretending there are already independent trusted people. Later custodians can take separate shares without changing the protocol trust model.

**One statement of S0 custody (ADR-0130 rule 7):** 2-of-3 root shares, one human custodian. Against key loss and
online compromise that is a threshold; against coercion of that one person it is equivalent to 1-of-1. Public wording
says exactly this and never calls S0 custody multi-party.

## 2026-09-23 08:21 protocol-continuity artifact

Long-term protocol continuity now adds a mirrorable **Protocol Continuity Bundle** for each published generation/profile. The bundle commits to the normative specifications/schemas, canonical encoding rules, conformance vectors, registry/CryptoProfile snapshots, licenses, release authentication/integrity data and migration guidance required to interpret that generation without a live project website or forge.

This does not make one bundle-signing key a protocol-global root. It authenticates a particular published project release/lineage. Independent implementations may preserve/mirror published bundles, and an incompatible successor profile/fork uses a new lineage identifier rather than rewriting the meaning of old bytes.
