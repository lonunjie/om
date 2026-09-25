# ClientHost Update Lifecycle

- **Status:** Canonical architecture direction; deployment mechanisms remain provider-specific
- **Checkpoint:** 2026-09-23 05:45 PHT

## 1. Problem

A self-owned PWA ClientHost gives users independence from one OM web origin, but thousands of independent origins also need a safe way to receive, verify, deploy, roll back and recover from client security updates. Most early PWA users (T0 sponsored and T1 hosted, ADR-0102/ADR-0112) will not own an origin at all; they use a **shared ClientHost** (§9), which needs stronger rules because its operator can ship code to many endpoints at once.

Release trust and deployment are separate problems.

## 2. Release artifacts

Canonical project releases publish signed update metadata/artifacts through the software-supply-chain model in [`SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md`](../providers/SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md) and independent mirrors.

A ClientHost deployment mechanism MUST verify the expected release identity before deploying when the hosting platform permits an out-of-band deployment controller.

## 3. Origin compromise limitation

A PWA already served by a malicious/compromised origin cannot securely rely on JavaScript from that same origin to prove that the origin's JavaScript is honest. An in-app verifier is therefore not treated as a magic recovery boundary after complete origin compromise.

Higher-assurance recovery uses an independent trusted path: a trusted native client, clean/new ClientHost deployment, authenticated new-device enrollment, or an external deployment verifier/controller.

## 4. Reference update flow

For supported self-host targets, OM should provide provider-native templates or declarative deployment tooling that can:

1. discover signed release metadata, for the owner-pinned release root (below), from independent locations;
2. verify allowed release/update root metadata outside the newly downloaded application bundle where practical;
3. stage a new immutable build;
4. atomically switch the ClientHost to that build;
5. retain bounded rollback material;
6. expose health/version status locally;
7. preserve DeviceVault data only through the documented origin/security model;
8. fail closed on invalid signatures/rollback metadata.

Rules for every deployment controller (ADR-0129):

1. **The owner pins the release root.** At deploy time the ClientHost owner chooses the release root: the canonical project root, a fork's, or their own. The controller trusts only that root. Changing it is an owner action authenticated with the owner's deploy credentials; release metadata can never change or add a root, and the canonical root has no standing to push into a deployment pinned elsewhere.
2. **Transparency before deploy.** The controller deploys a release automatically only after observing its digest in **at least two independent transparency monitors**, and only after an owner-configurable delay (default 24 hours). R-164's client-side digest check still runs afterwards; this rule makes transparency a precondition, not only a detector.
3. **Emergency path.** A signed SecurityAdvisory (ADR-0104) from the pinned root may shorten the delay (never below 1 hour). It never skips the transparency precondition, never changes the root and never deploys a release the root did not sign.
4. **Owner chooses the mode.** Updates are either automatic (rules 2–3) or approval-required: the controller stages the verified release and waits for the owner. The owner can switch between them at any time.

## 5. Service worker

The PWA service worker is a deployment/cache mechanism, not an independent root of trust against the controlling origin. Update design must specify cache versioning, old-client compatibility windows, staged activation and recovery from a broken worker.

## 6. Abandoned host

If a personal ClientHost is abandoned, unsupported, compromised, or cannot meet a minimum security version, the supported recovery path is migration/new-device enrollment to a trustworthy host/client. OM does not require preserving a compromised origin merely to preserve identity.

## 7. Provider credentials

Cloud/provider deployment credentials stay outside OM protocol state and should use least privilege. Moving to another hosting provider is a ClientHost deployment change, not AccountID migration.

## 8. Freeze gate

Before the reference PWA is called sustainably self-hostable, specify and test the list below. Since ADR-0130 these
items are the pre-engineering criteria of gate **V-023** (`clienthost_update_lifecycle`), which blocks
`reference_client_ui`; a live update and rollback drill on the reference PWA is its pre-release criterion. The
release-transparency log and monitor roles the controller and client rely on are specified in
[`SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md`](../providers/SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md) §6.

- release discovery/mirror rules;
- signed metadata verification path;
- provider-template update flow;
- service-worker lifecycle;
- rollback/freeze protection;
- ~~emergency minimum-version policy;~~ owner-local minimum-version floor: the ClientHost owner may refuse to serve releases below a floor they set, informed by SecurityAdvisories. Clients apply ADR-0104 peer-local floors ("Needs update"). There is no remote disable and no floor that anyone other than the owner can set (ADR-0104);
- broken/compromised-origin migration;
- credential scoping;
- offline/long-neglected host upgrade behavior.

## 9. Shared ClientHost (ADR-0129)

A **shared ClientHost** is an origin that serves the OM PWA to users who do not own it: a community's app server, a volunteer mirror, or one the project or a distribution runs. Whoever operates it can ship executable code to every user it serves; that is T-042 at larger scale (threat T-074). It is a **distribution role** under ADR-0049 (convenience providers are non-privileged) and ADR-0082 (a capability does not imply a project-operated service). No shared ClientHost is technically privileged, including one the project runs.

Obligations:

1. **Plural and selectable.** Onboarding that offers a shared ClientHost offers a choice among at least two operated by independent parties, or names the single one and says that others exist. Nothing in the protocol, the client or a Home depends on a particular shared ClientHost.
2. **Transparency-logged releases only.** It serves only unmodified releases, deployed under §4 rules 1–3, whose digests appear in at least two independent transparency monitors. It never serves locally modified code under a release's identity.
3. **Client-side digest check.** R-164 applies: the client records the digest it runs and compares it with independent transparency observations when online. A mismatch is a security event ("The app you are running does not match any published release") with a path to move.
4. **Named origin.** Onboarding states which origin serves the app ("This app is served by app.example.org"), and Settings → About shows it.
5. **Separate from the Home.** It is not operated from the same hosting account as the user's mailbox Home (extending R-188), so one phished account is not both endpoint and relay.
6. **Leaving is enrollment.** Moving to another shared ClientHost, one's own origin or a native client is new-device enrollment (§6), never browser-state copying.

## 2026-09-23 updater resolution

Reference deployment prefers an external release/deployment controller that verifies signed/TUF-style metadata from the owner-pinned release root (§4, ADR-0129) before replacing a ClientHost origin. Service-worker verification is defense-in-depth only. Suspected origin compromise is handled as authenticated new-origin/new-device enrollment followed by old-device revocation, not raw browser-state migration.

## Rekor-compatible release transparency profile (ADR-0133)

The reference release log uses Rekor-v2/C2SP signed-checkpoint compatible append-only semantics. A release entry
commits to distribution id, release digest, TUF-target metadata digest, provenance/source digest when available and
release generation. Inclusion and append-only consistency are independently verifiable. The transparency log is
**evidence**, never update authority; the owner's pinned TUF/root remains authority.

At least two separately operated monitors verify consistency/inclusion and publish signed `ReleaseObservation`s over
release digest + checkpoint. Automatic deployment requires two valid observations from distinct configured monitor
operators/failure domains plus the owner's update delay. Monitor snapshots are batch/epoch addressed and mirrorable to
avoid per-user/per-version lookup privacy. Missing observations pause automatic deployment; they never disable the
installed client or messaging.

A hostile ClientHost origin can replace its own JavaScript/service worker and therefore cannot be assumed to enforce
these checks on itself. Transparency reduces supply-chain and equivocation risk before compromise; recovery from a
compromised origin is authenticated new-device enrollment at another origin/native client.
