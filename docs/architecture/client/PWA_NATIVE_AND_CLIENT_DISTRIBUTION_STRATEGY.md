# PWA, Native, and Client Distribution Strategy

## 1. Decision

OM is **PWA-first, not PWA-only**.

The first operatorless reference client should be an installable PWA because it can be distributed directly, can use standards-based Web Push, and does not require an app-store publisher merely to prove the protocol and product.

Android and iOS native clients are explicit V1 reference-family targets because they provide stronger OS integration and catch assumptions a browser cannot validate. The PWA remains the first vertical slice; this is not a simultaneous-launch requirement (ADR-0150).

## 2. Shared portable core

Security/state logic should live in one portable implementation rather than being independently reimplemented in JavaScript, Swift and Kotlin.

Preferred direction:

```text
OM Core (Rust or equivalently portable memory-safe core)
  identity
  crypto-profile integration
  AccountControlState
  events
  MLS/group state
  discovery/contact capsules
  sync/recovery
  canonical encodings
       |
       +-- WASM -> reference PWA
       +-- native FFI -> Android shell
       +-- native FFI -> iOS shell
       +-- native -> desktop/CLI
```

The shared `om-ui` owns the visible application experience. Platform shells own only platform-specific capabilities/system surfaces such as secure key handles, notifications, share sheets, system calling, local contacts permission, background execution and future SMS/RCS integration.

## 3. What the PWA can cover

The PWA is expected to support:

- OM-native 1:1 (hybrid-PQ) and group (classical MLS until the ADR-0107 triggers are met) messaging through OM Core;
- media/file transfer and encrypted storage;
- local encrypted state and offline UI;
- QR / `om:` invitations and ContactCapsule sharing;
- Web Push notifications;
- WebRTC voice/video while the runtime is active;
- LAN/P2P/TURN/SFU media paths where browser APIs allow;
- optional offline-local message/file delivery through manual/QR/known-endpoint/WebRTC-style local routing when browser capabilities permit it;
- backup/restore flows;
- multi-device linking;
- installable icon/standalone app presentation.

On iOS/iPadOS, standards-based Web Push is supported for Home Screen web apps and does not require Apple Developer Program membership for Web Push itself.

## 4. Known PWA limitations

Do not pretend the browser is identical to native.

Important gaps include:

- native-grade locked/background incoming VoIP UX (e.g. PushKit/CallKit-class integration);
- platform-specific background execution guarantees;
- direct access to all native hardware-backed key-management capabilities;
- Android default-SMS role and future SMS/MMS/RCS adapters;
- some OS-level file/share/contact integrations;
- platform-dependent local/LAN API restrictions.

Therefore a PWA-only policy would unnecessarily constrain the future product.

## 5. Native clients are convenience/assurance shells, not the protocol

V1 native reference shells add/validate:

### Android

- Android Keystore/StrongBox integration;
- native notification/background lifecycle;
- system calling integration;
- better media/file handling;
- future default-SMS/MMS and authorized RCS adapters.

### iOS

- Keychain/Secure Enclave-backed operations where compatible;
- native push/calling integration;
- better background behavior and OS integration.

### Desktop (V1 reference-family target)

- native secure-storage integration appropriate to macOS/Windows/Linux;
- multi-window and filesystem/share integration through `PlatformAdapter`;
- native notification/update lifecycle integration;
- keyboard/pointer density and large-window layouts without a separate protocol/UI semantic model.

Desktop OS support may be staged; V1 target status means the shared contracts are validated against desktop constraints before V1 architecture is considered complete, not that every desktop binary must launch simultaneously.

All native shells speak the exact same OM protocol and use the same user-owned infrastructure. Native-only APIs are capabilities, not native-only protocol semantics; automatic LAN discovery may be available here while the PWA uses a manual/QR/known-endpoint fallback.

## 6. Avoid the fragmented-client trap

Open protocol does not mean users should be forced to choose among uncoordinated third-party apps.

The project should maintain a coherent reference family:

```text
om-core/
om-web/
om-android/
om-ios/
om-desktop/
```

Third-party clients remain welcome and must be able to interoperate through public specifications/conformance suites.

The reference family may be socially prominent but receives no hidden protocol privilege.

## 7. Distribution-local obligations

A native App Store / Play Store distribution may have store-policy, signing, update, push and moderation obligations that do not belong to the OM protocol itself.

The architecture must allow:

- a PWA/self-host distribution;
- an F-Droid-like distribution;
- an App Store distribution;
- a Play Store distribution;
- enterprise/private distributions;
- third-party distributions.

Each distribution owns its own signing/update/push/store-policy boundary.

## 8. Push ownership

Web/PWA path:

```text
recipient PersonalRelay/Worker
   -> standards-based Web Push
   -> recipient PWA
```

No universal OM push gateway is required.

Native path:

A particular native distribution may operate or outsource a narrowly scoped APNs/FCM/UnifiedPush-compatible gateway, or use a bounded persistent encrypted wake/fetch connection where the OS permits. Every adapter is a property of that distribution/user configuration, not a mandatory protocol service.

## 9. Roadmap consequence

1. operatorless PWA proves the protocol and product;
2. polish it into a usable daily messenger;
3. build Android, iOS and desktop V1 reference shells over the same OM Core/shared UI, staged as practical;
4. provide replaceable Android wake adapters so FCM is convenience rather than protocol dependency;
5. add Android SMS/MMS/RCS adapters later;
5. optional providers/businesses compete on convenience without owning the network.

## 10. Shared UI architecture

The PWA is the first shell for the same shared `om-ui`, not a separate web product whose UX will later be rewritten natively.

```text
om-core
   ↕ typed state/commands
om-ui
   ↕ PlatformAdapter
web / iOS / Android / desktop shells
```

The native clients should therefore add better OS integration without surprising users with a different product. System permission prompts, lock-screen incoming calls, file pickers, share sheets and biometrics remain native; OM-owned screens remain shared.

This reduces semantic drift and makes the PWA a genuine first implementation of the long-term client rather than a disposable prototype.

## PWA-to-native migration

Native installation is a normal new-device link:

- fresh native DeviceIdentity and native DeviceVault;
- authenticated transfer/rebuild of profile state/history;
- current AccountControlState/head verification;
- fresh native push/system-call registrations;
- optional retirement of the PWA device.

Do not export web-origin private-key/database files as the cross-platform continuity mechanism.
## 2026-09-22 self-host lifecycle hardening

PWA-first independence includes lifecycle ownership, not only first deployment. [`CLIENTHOST_UPDATE_LIFECYCLE.md`](CLIENTHOST_UPDATE_LIFECYCLE.md) specifies the required update/deployment/rollback/compromised-origin migration model. A self-hosted origin that cannot be kept safely updated is not treated as durable independence.

