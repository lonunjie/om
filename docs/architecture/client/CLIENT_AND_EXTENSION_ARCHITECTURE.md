# OM Client, UI, Extension, and Execution Architecture

- **Status:** Phase 0 selected direction
- **Timestamp:** 2026-08-18 13:53 PHT (UTC+08:00)
- **Scope:** Reference clients, web/PWA, native clients, third-party apps/UIs, SDK boundaries, extension safety, and execution order

## Decision summary

OM is an **open protocol + reusable secure core + conformance suite**, not one UI.

The official project will ship a polished reference app, but independent developers may build:

- alternative clients;
- accessibility-focused clients;
- skins/themes;
- business clients;
- desktop/mobile/web clients;
- specialist high-assurance clients;
- integrations and adapters.

The protocol must never depend on behavior available only in the official UI.

## Layering

```text
┌──────────────────────────────────────────────┐
│              UI / Product Layer              │
│ official OM UI / third-party UI / business   │
└──────────────────────┬───────────────────────┘
                       │ stable client API
┌──────────────────────▼───────────────────────┐
│             OM Client Application            │
│ conversations / contacts / groups / search   │
└──────────────────────┬───────────────────────┘
                       │
┌──────────────────────▼───────────────────────┐
│                 OM Core                      │
│ canonical events / identity / group logic    │
│ protocol state / provider interfaces         │
└───────┬────────────┬────────────┬────────────┘
        │            │            │
  CryptoProvider  StoragePool  RelayProvider   ...
        │
┌───────▼──────────────────────────────────────┐
│          Platform / Provider Adapters         │
│ Web / Android / iOS / S3 / Drive / NAS ...  │
└──────────────────────────────────────────────┘
```

## Official app responsibility

Open source does not mean the core team can ignore UX.

The official app remains responsible for:

- a clear reference experience;
- accessibility;
- safe defaults;
- security-warning semantics;
- migration/restore UX;
- conformance with the protocol;
- proving the protocol is practical for normal people.

Third-party clients are welcome, but the protocol must remain usable without them.

## Third-party clients

Anyone may implement the public protocol.

OM should publish:

1. protocol specifications;
2. canonical schemas;
3. golden test vectors;
4. conformance tests;
5. reusable SDK/core libraries where practical;
6. provider/transport extension interfaces;
7. compatibility/capability negotiation rules.

Avoid a centralized permission gate for third-party clients.

### Security labeling

A third-party client can be protocol-compatible without having identical security properties.

Conformance should be testable and descriptive:

```text
OM Protocol Compatible
OM Identity Tests: PASS
OM Event Tests: PASS
OM Group Tests: PASS
OM Provider Tests: PASS
```

Do not imply that open source alone makes every client equally trustworthy.

## UI customization

Two different extension categories:

### Safe presentation customization

Official client may support:

- themes;
- skins;
- layout preferences;
- icon packs;
- accessibility presets;
- renderer extensions for known safe content types.

These should not receive plaintext from unrelated conversations unless required.

### Independent/full clients

A developer who wants arbitrary UI/application logic should build an independent OM client against the public core/SDK.

Do **not** add arbitrary in-process code plugins to the official client in v1. That would expand the plaintext/key attack surface and complicate mobile-store security.

## Web as a first-class client

The browser is a first-class OM `DeviceIdentity`.

Web is valuable because it offers:

- one URL;
- immediate deployment;
- fast protocol iteration;
- mobile + desktop browser access;
- no app-store wait for early prototypes;
- easy interoperability testing.

The browser client must still be treated as a distinct security/runtime environment.

### Web device model

```text
Browser install/profile
 -> DeviceIdentity
 -> independently authorized by AccountControlState
 -> independently revocable
```

A web session is never merely a password-authenticated mirror of the phone.

### Web/PWA execution role

**Selected execution direction:**

1. build the first usable OM-native reference implementation as a web/PWA protocol lab;
2. use it to validate identity, canonical events, relay/provider interfaces, 1:1 delivery, and groups;
3. build Android native early enough to prevent browser assumptions from becoming protocol assumptions;
4. add iOS after core protocol behavior and push/device lifecycle are stable;
5. Android carrier/SMS integration remains native-only work after the OM-native core is stable.

Web-first does **not** mean web-only final product.

## Why native remains required

The final daily-driver product needs OS capabilities that a browser cannot universally replace:

- Android default-SMS role;
- robust platform notification/background integration;
- platform-native contacts/sharing/media integrations;
- stronger platform key-storage options;
- better lifecycle control;
- future carrier/RCS integrations where available.

The core must expose these as platform adapters rather than baking them into the protocol.

## CryptoProvider boundary

OM should avoid assuming every private key is an exportable byte array owned by the core.

Define a platform cryptographic-provider boundary capable of operations such as:

```text
generate_key(purpose, policy)
public_key(key_ref)
sign(key_ref, message)
derive/agreement(key_ref, peer)
delete_key(key_ref)
attestation?(key_ref)
```

A native implementation may bind keys to Android Keystore / Apple Keychain or Secure Enclave where algorithms permit.

A browser implementation may use browser/WASM-backed mechanisms appropriate to the selected protocol, with its different threat model explicitly documented.

The wire protocol must remain interoperable across providers.

## Web code-delivery threat

A native signed binary and a browser-delivered application do not have identical trust properties.

The web deployment plan must therefore include:

- strict CSP;
- dependency pinning;
- reproducible/versioned static builds where practical;
- subresource integrity where applicable;
- minimal third-party JavaScript;
- no advertising/analytics scripts in the crypto origin;
- isolated origin for the messenger;
- release transparency/version visibility;
- strong XSS prevention;
- explicit linked-device/revocation UI.

High-assurance users may choose to disable web devices.

## PWA notifications

Web Push is a useful wake/notification path where the platform supports it.

Push notifications contain no message plaintext by architectural policy; they wake/notify a client that encrypted state may be available.

Do not make correctness depend on silent background execution.

## Execution milestone

A successful web-first prototype proves:

- two browsers can create AccountIDs;
- devices can link/revoke;
- message requests work;
- accepted relationships exchange canonical OM events;
- groups interoperate;
- relay/storage providers can be swapped;
- browser refresh/restart restores encrypted local state safely enough for prototype use;
- conformance vectors are shared with the future native clients.

It does **not** prove:

- Android SMS replacement;
- final mobile background reliability;
- final native key protection;
- carrier/RCS support;
- production high-assurance browser security.

## Architecture rule

> The official OM application is a reference implementation of an open system, not the definition of the system.


## 2026-09-21 canonical addendum — invisible complexity and new provider boundaries

Add `MediaProvider`, `RoutePolicy`, and `CryptoProfileRegistry` to the core capability boundaries.

Normal UI must not expose cipher selection, TURN/SFU selection, provider quota arithmetic, or routine crypto migration. Advanced infrastructure controls are optional policy settings. SMS/MMS/RCS remain future TransportAdapters and are not first-release blockers.

## 2026-09-22 canonical addendum — coherent reference family

The official/reference client family may be socially prominent but never protocol-privileged.

Preferred repository/product family:

```text
om-core
om-web
om-android
om-ios
om-desktop (later)
```

Third-party clients remain first-class interoperability targets. The project should nevertheless maintain a coherent reference family so non-technical users are not forced to choose among incompatible/unvetted forks.

A `ClientDistribution` owns only distribution-local concerns: signing/update channel, store listing, platform push gateway (if any), and distribution-specific report/compliance integrations. None of those may become required by the OM wire protocol.

## 2026-09-22 13:42 canonical addendum — one shared OM UI

The coherent reference family now has an explicit shared UI layer:

```text
OM Protocol
   ↓
om-core
   ↓
om-ui
   ↓
PlatformAdapter
   ↓
PWA / iOS / Android / Desktop
```

`om-ui` owns navigation, screens, components, accessibility and visual behavior. Platform shells provide OS capabilities/system surfaces only. Do not reimplement security-sensitive product flows independently in SwiftUI/Compose/web.

Safe themes are declarative bundles interpreted by `om-ui`; arbitrary in-process UI code plugins remain out of scope. Third-party full clients remain free to implement the public protocol/core contracts independently.

## 2026-09-22 engineering layering refinement

The official client application is now decomposed further:

```text
ClientHost -> DeviceVault -> IdentityProfile(s) -> om-core -> om-ui -> PlatformAdapter
```

`IdentityProfile` is a local isolation namespace around a real AccountID. It is not a new network identity type. Core/storage must support several profiles before the account-switching UI ships.

Rich-text rendering is part of shared `om-ui` but consumes only validated RichTextV1 semantic nodes. Arbitrary renderer extensions do not receive DOM/script execution privileges.
