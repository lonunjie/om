# OM System Architecture — 2026-09-21

## System intent

A local-first, provider-independent secure messenger where identity and conversation continuity survive infrastructure and cryptographic evolution.

## Layer map

```text
┌────────────────────────────────────────────────────────────┐
│                    Product / UI layer                      │
│ conversations · contacts · groups · calls · search       │
└────────────────────────────┬───────────────────────────────┘
                             │
┌────────────────────────────▼───────────────────────────────┐
│                       OM application                       │
│ routing · policy · materialized views · safety UX        │
└────────────────────────────┬───────────────────────────────┘
                             │
┌────────────────────────────▼───────────────────────────────┐
│                          OM Core                           │
│ AccountID/device state · canonical events · relationships│
│ MLS groups · CryptoProfiles · call state · sync/restore  │
└───────┬────────┬────────┬────────┬────────┬───────────────┘
        │        │        │        │        │
   Crypto   Relay     Storage   Media    Directory/Push/Transport
 Provider   Provider  Pool      Provider  providers/adapters
        │        │        │        │        │
┌───────▼────────▼────────▼────────▼────────▼───────────────┐
│              replaceable implementations                  │
│ local/VPS/CF/R2/S3/WebDAV/APNs/FCM/WebRTC/etc.           │
└────────────────────────────────────────────────────────────┘
```

## Identity

- `AccountID`: stable random/provider-independent account lineage identifier.
- `DeviceIdentity`: independently authorized/revocable device credential.
- `PersonRecord`: local/private human contact grouping.
- aliases/phone/email/domain/QR: claims and discovery references, not identity roots.
- AccountID is never a relay queue/routing address.
- cryptographic credential epochs can migrate across algorithms without changing AccountID.

## Relationship

Unknown contact:

```text
Discovery/Invite -> IntroductionEndpoint -> MessageRequest
```

Accepted contact:

```text
RelationshipBinding
  - authenticated peer AccountID/device state
  - private delivery capabilities
  - current CryptoProfile/security floor
  - route/provider hints
  - local PersonRecord binding
```

## Messaging

```text
semantic event
  -> E2EE/session/group encryption
  -> fresh transport wrapper
  -> opaque relay capability
  -> receiving device
  -> authenticated event
  -> local materialized view
```

Relays do not assign EventID or canonical order.

## Groups

Human membership is AccountID-level; cryptographic MLS membership is DeviceIdentity-level. Group roles/admin state is application state bound to authenticated MLS epoch transitions. History sharing remains separate from live membership.

## Storage

Local state is primary for the client. Delivery queue, device sync, blobs, archive, backup, and recovery are separate provider capabilities. `StoragePool` can mirror/migrate encrypted data.

## Calls

Encrypted OM signaling authenticates participants. SFrame protects media independently of WebRTC route. `RoutePolicy` chooses LAN/direct P2P/TURN/SFU. `MediaProvider` is replaceable.

## Transport interoperability

OM-native messaging is first. `TransportAdapter` remains stable so SMS/MMS/RCS/bridges can be added later without changing the canonical event model.

## Push

Push providers wake a device with opaque metadata. They are not correctness authorities. Opening the app or polling the relay must recover missed state.

## Key transparency/discovery

Directories may publish aliases/device/account key material, but cannot become root identity. OM should use append-only transparency commitments plus independent witnesses/auditors/client gossip where practical to detect equivocation.

## Provider policy engine

Provider selection considers:

- capability compatibility;
- security/trust requirements;
- user privacy preference;
- latency/health;
- quota and cost state;
- failure-domain diversity;
- jurisdiction/operator preference where user configures it.

The default UX is “Automatic.”

## Local-first resilience

Long-term target: established peers on the same LAN should be able to message, transfer files, and call without WAN media transit; discovery must avoid broadcasting stable identities.

## Authority boundary

No provider is allowed to combine roles into protocol-level super-authority. Even if one vendor implements multiple interfaces, identifiers/keys/data exposure remain minimized per interface.

## Discovery/resilience overlay (2026-09-21)

```text
Human alias / QR / domain / LAN
            ↓
        OM Resolver
            ↓
     ContactDescriptor
            ↓
         IntroGate
            ↓
     MessageRequest
            ↓ accept
 private relationship state
            ↓
         RelaySet
   ┌────────┼────────┐
 relay A  relay B  relay C
```

The resolver, directory, IntroGate and relays are separate replaceable roles. Network transports beneath them are also replaceable. Existing relationships do not require a directory to continue operating.

## Control / policy layers added at 21:32 checkpoint

```text
                         Human UX
                            |
                 Security/Privacy Presets
                            |
       +--------------------+--------------------+
       |                    |                    |
 RecoveryPolicy       NotificationPolicy     Route/Metadata Policy
       |                    |                    |
       +---------- AccountControlState ---------+
                            |
                    AccountID / devices
                            |
       discovery -> IntroGate -> relationships -> relay/storage/media
```

The Cloudflare provisioning layer sits outside this graph as one replaceable way to instantiate provider adapters. Software-update trust sits around the client itself and is treated as part of the effective security boundary.

## 2026-09-22 ecosystem layer

Above protocol semantics, distinguish:

```text
OM Protocol/Core
    |
    +-- independent user/organization deployments
    +-- reference and third-party client distributions
    +-- optional convenience providers
```

There is no mandatory centrally operated OM network service. User deployments own their rendezvous/relay state; distributions own only packaging/update/platform integration; providers are replaceable capabilities.

## Shared presentation layer — 2026-09-22 13:42

Reference clients now share a first-class presentation layer:

```text
Protocol / canonical semantics
          ↓
        OM Core
          ↓
         OM UI
          ↓
   PlatformAdapter
    ↙   ↓   ↓   ↘
  PWA  iOS Android Desktop
```

This layer does not alter the rule that independent/third-party clients can implement OM. It ensures the official/reference family remains one coherent product and that security-sensitive action semantics do not drift across platform-specific UI rewrites.

## Client installation / profile layers — 2026-09-22 14:34

```text
ClientHost (PWA/native executable/update boundary)
      ↓
DeviceVault (device-local secret wrapping)
      ↓
IdentityProfile A / B / ...
      ↓
AccountID + local state + RecoveryPolicy
      ↓
IdentityHome (discovery/IntroGate/relay/blob providers)
```

These are distinct. One client installation can later host several IdentityProfiles; each profile has independent keys/state/home bindings. PWA ClientHost/origin migration is authenticated device migration, not browser-database copying.

Rich user content also has an explicit trust boundary: E2EE authentication tells OM who delivered bytes, not whether those bytes are safe to parse. RichTextV1 is inert semantic data and attachments remain untrusted until validated/decoded through safe platform paths.
## Architecture hardening overlay — 2026-09-22 23:02

The component map now includes explicit layers that were previously implicit:

```text
AccountControlState / DeviceIdentity
        |
        +--> KeyMaterialProvider --> per-device async handshake material / MLS KeyPackages
        |
        +--> OneToOneSessionManager --> device-to-device ratchet/session fanout
        |
        +--> GroupCommitOrdering + GroupControlTransition --> MLS epoch convergence

Relationship state
        +--> RelaySet normal routing
        +--> ReachabilityRepair (only when known private routes are lost)

Canonical message attachment
        +--> immutable BlobID
        +--> authenticated mutable BlobLocationRecord

ClientHost
        +--> release/update/deployment lifecycle separate from DeviceVault identity
```

Cross-cutting `TIME_AND_EXPIRY_SEMANTICS.md` classifies all TTL/clock fields. None of these new layers grants infrastructure identity authority.

