# ClientHost Update Lifecycle

- **Status:** Canonical architecture direction; deployment mechanisms remain provider-specific
- **Checkpoint:** 2026-09-23 05:45 PHT

## 1. Problem

A self-owned PWA ClientHost gives users independence from one OM web origin, but thousands of independent origins also need a safe way to receive, verify, deploy, roll back and recover from client security updates.

Release trust and deployment are separate problems.

## 2. Release artifacts

Canonical project releases publish signed update metadata/artifacts through the software-supply-chain model in `SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md` and independent mirrors.

A ClientHost deployment mechanism MUST verify the expected release identity before deploying when the hosting platform permits an out-of-band deployment controller.

## 3. Origin compromise limitation

A PWA already served by a malicious/compromised origin cannot securely rely on JavaScript from that same origin to prove that the origin's JavaScript is honest. An in-app verifier is therefore not treated as a magic recovery boundary after complete origin compromise.

Higher-assurance recovery uses an independent trusted path: a trusted native client, clean/new ClientHost deployment, authenticated new-device enrollment, or an external deployment verifier/controller.

## 4. Reference update flow

For supported self-host targets, OM should provide provider-native templates or declarative deployment tooling that can:

1. discover signed canonical release metadata from independent locations;
2. verify allowed release/update root metadata outside the newly downloaded application bundle where practical;
3. stage a new immutable build;
4. atomically switch the ClientHost to that build;
5. retain bounded rollback material;
6. expose health/version status locally;
7. preserve DeviceVault data only through the documented origin/security model;
8. fail closed on invalid signatures/rollback metadata.

## 5. Service worker

The PWA service worker is a deployment/cache mechanism, not an independent root of trust against the controlling origin. Update design must specify cache versioning, old-client compatibility windows, staged activation and recovery from a broken worker.

## 6. Abandoned host

If a personal ClientHost is abandoned, unsupported, compromised, or cannot meet a minimum security version, the supported recovery path is migration/new-device enrollment to a trustworthy host/client. OM does not require preserving a compromised origin merely to preserve identity.

## 7. Provider credentials

Cloud/provider deployment credentials stay outside OM protocol state and should use least privilege. Moving to another hosting provider is a ClientHost deployment change, not AccountID migration.

## 8. Freeze gate

Before the reference PWA is called sustainably self-hostable, specify and test:

- release discovery/mirror rules;
- signed metadata verification path;
- provider-template update flow;
- service-worker lifecycle;
- rollback/freeze protection;
- emergency minimum-version policy;
- broken/compromised-origin migration;
- credential scoping;
- offline/long-neglected host upgrade behavior.

## 2026-09-23 updater resolution

Reference deployment prefers an external release/deployment controller that verifies signed/TUF-style metadata before replacing a ClientHost origin. Service-worker verification is defense-in-depth only. Suspected origin compromise is handled as authenticated new-origin/new-device enrollment followed by old-device revocation, not raw browser-state migration.
