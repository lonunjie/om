# Security and Privacy Profiles

## 1. One cryptographic floor, different operational privacy policies

OM is a messenger for everyone, not a niche "privacy messenger." The product should be easy by default while allowing users with stronger threat models to increase protection.

Profiles MUST NOT mean "weak crypto versus strong crypto." All users get the strongest approved compatible CryptoProfile, authenticated devices, forward secrecy/post-compromise mechanisms, and automatic crypto migration.

Profiles change **metadata, routing, notification, recovery, retention, and provider** policy.

## 2. Policy decomposition

Internally OM uses independent policies rather than one magic `security_level`:

```text
CryptoPolicy          // always strongest approved compatible profile
RecoveryPolicy
MetadataPolicy
RoutePolicy
NotificationPolicy
DeviceControlPolicy
StoragePolicy
RetentionPolicy
CensorshipTransportPolicy
```

Friendly profiles simply install recommended combinations. If the user changes one component, UI shows **Custom**.

## 3. Recommended presets

### Recommended

Goal: excellent everyday usability with privacy materially stronger than ordinary server-readable messaging.

- strongest approved E2EE/PQ-hybrid profile;
- opaque/generic vendor push wakeups;
- local notification rendering after decrypt;
- basic size bucketing/padding where cheap;
- accepted-contact P2P/LAN calls allowed;
- TURN fallback;
- ordinary directory lookup with optional privacy relay when inexpensive;
- receipts/presence enabled by normal user choice;
- automatic relay failover;
- no recovery unless user explicitly enables it;
- normal encrypted local storage.

### Hardened

Goal: reduce metadata exposure without making the app frustrating.

- same crypto floor;
- oblivious/private discovery where available;
- stronger message size buckets/padding;
- generic lock-screen notifications by default;
- reduced/off presence and receipts;
- prefer relayed calls where peer-IP exposure matters;
- provider diversity preferred;
- encrypted backups only when explicitly configured;
- stricter first-contact policy.

### Maximum Privacy

Goal: users with unusually strong metadata/network threats; accepts cost/latency/battery/usability tradeoffs.

- same crypto floor;
- no direct peer-IP exposure unless explicitly overridden;
- multi-hop/anonymity/pluggable transports when available;
- stronger traffic padding/batching where practical;
- presence/typing/read receipts off by default;
- private/direct invitation preferred over searchable directories;
- vendor push may be disabled, with explicit warning that background delivery can become delayed, especially on iOS;
- backup normally off/local unless explicitly configured;
- no recovery unless explicitly configured;
- aggressive provider diversity.

### Local only (future specialty mode)

Not a joke architecturally: existing/QR-linked peers on a LAN can communicate without WAN where supported. This is an availability/privacy mode, not a universal replacement for Internet messaging.

## 4. UX

First run should default to **Recommended settings**. Do not force every user to understand profiles.

A secondary path may say:

```text
Protection
✓ Recommended
  Hardened
  Maximum Privacy
```

Every setting must explain the human effect, for example:

- "Hide your IP address from people you call — calls may use more relay bandwidth."
- "Do not use Apple/Google push — messages may arrive later while the app is closed."
- "No account recovery — losing all your devices permanently loses this identity."

## 5. Security monotonicity

Changing a convenience/privacy profile must not silently lower the authenticated cryptographic security floor established with a peer/group. Security-critical downgrades require explicit protocol policy and visible user action where allowed at all.

## 2026-09-23 metadata-threat-model resolution

`METADATA_THREAT_MODEL.md` is canonical for A-013.

The profiles above are policy bundles, not privacy badges. In particular:

- all profiles retain the same cryptographic floor;
- normal private provider routing uses scoped opaque capabilities rather than canonical AccountID/ConversationID;
- inner session/MLS payloads are protected by independently randomized provider-facing transport envelopes where required to avoid exposing canonical group/session identifiers to relays;
- Recommended may use cheap size bucketing/padding but does not claim traffic anonymity;
- Hardened may add private discovery, stronger padding/provider separation and relayed calls;
- Maximum Privacy may add anonymity/pluggable transports, stronger batching/padding and optional bounded cover traffic with explicit bandwidth/battery cost;
- opaque push payloads still leak wake timing to the push provider;
- no preset may claim to defeat a global observer/provider collusion unless a separately reviewed mechanism actually supplies that property.

## 2026-09-24 correction — post-quantum wording (ADR-0107)

"Strongest approved E2EE/PQ-hybrid profile" above means the strongest approved profile **for each conversation type**. In OM-Crypto-v1, 1:1 conversations are hybrid-PQ. Private groups and group calls use the classical RFC 9420 X25519 MLS suite until the ADR-0107 adoption triggers are met. No preset may claim PQ confidentiality for groups. Presets never change this; it is a property of the CryptoProfile, not of privacy policy.

Coercion, device-access and platform-leakage controls (app lock, secure surfaces, keyboard/spellcheck hints, backup exclusion) are defined in `ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md` and are part of the Hardened and Maximum Privacy bundles.
