# PWA, Native, and Client Distribution Strategy

## 1. Decision

OM is **PWA-first, not PWA-only**.

The first operatorless reference client should be an installable PWA because it can be distributed directly, can use standards-based Web Push, and does not require an app-store publisher merely to prove the protocol and product.

Native clients remain part of the long-term architecture because they provide stronger OS integration and unlock capabilities that a browser cannot universally provide.

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

Platform shells own only platform-specific concerns such as UI, secure key handles, notifications, share sheets, calls, contacts and future SMS/RCS integration.

## 3. What the PWA can cover

The PWA is expected to support:

- OM-native 1:1 and group messaging;
- PQ/hybrid E2EE through OM Core;
- media/file transfer and encrypted storage;
- local encrypted state and offline UI;
- QR / `om:` invitations and ContactCapsule sharing;
- Web Push notifications;
- WebRTC voice/video while the runtime is active;
- LAN/P2P/TURN/SFU media paths where browser APIs allow;
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

Future native distributions may add:

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

They still speak the exact same OM protocol and use the same user-owned infrastructure.

## 6. Avoid the fragmented-client trap

Open protocol does not mean users should be forced to choose among uncoordinated third-party apps.

The project should maintain a coherent reference family:

```text
om-core/
om-web/
om-android/
om-ios/
om-desktop/   (later)
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

A particular native distribution may operate or outsource a narrowly scoped APNs/FCM gateway. That gateway is a property of that distribution, not a mandatory protocol service.

## 9. Roadmap consequence

1. operatorless PWA proves the protocol and product;
2. polish it into a usable daily messenger;
3. build native shells over the same OM Core;
4. add Android SMS/MMS/RCS adapters later;
5. optional providers/businesses compete on convenience without owning the network.
