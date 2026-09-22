# ClientHost, DeviceVault and Client Migration

## 1. Why this is a security boundary

For a PWA, the web origin controls execution and browser storage. IndexedDB/service workers/Web Push/WebAuthn are origin or relying-party scoped. Therefore the PWA origin is not merely hosting; it is part of endpoint trust and continuity.

## 2. Separate ClientHost from IdentityHome

```text
ClientHost
  serves installed OM application code
  owns origin-scoped browser state

IdentityHome
  public discovery/IntroGate/relay/storage services for one IdentityProfile
```

An implementation MAY colocate them operationally, but Core must model them separately.

The reference personal Cloudflare deployment should prefer a distinct application origin from the public-facing introduction/relay endpoint so hostile public content is not served from the same origin that holds the application vault.

## 3. PWA origin compromise

PWA has an unavoidable endpoint limitation: a malicious origin can eventually deliver malicious executable application code. Application-level signatures inside that code cannot fully defend an already-compromised origin/bootstrap.

Mitigations:

- user owns the origin/account rather than one universal OM origin;
- separate public service origin from app origin;
- strong provider account security/2FA;
- minimal origin contents and strict CSP;
- TUF/provenance/reproducible app bundles for audit and safer updates;
- no remote script dependencies;
- native clients remain the higher-assurance path for users who need a stronger executable/key boundary.

Do not claim PWA origin compromise resistance that the browser platform cannot provide.

## 4. DeviceVault

Logical interface:

```text
DeviceVault
  create_profile_vault(profile_id)
  seal(profile_id, purpose, bytes)
  open(profile_id, purpose, sealed)
  delete(profile_id, handle)
  rotate_wrapping(profile_id)
  capabilities()
```

Each profile gets an independent random `ProfileVaultKey` (PVK). Derive independent subkeys with a domain-separated KDF, e.g.:

```text
PVK
 ├─ om:v1:local-db
 ├─ om:v1:search-index
 ├─ om:v1:draft-store
 ├─ om:v1:attachment-cache
 └─ om:v1:diagnostics-seal
```

Long-lived protocol private keys are stored/sealed behind the vault and are not exposed to UI code unless an approved crypto implementation absolutely requires it.

## 5. Web unlock mechanisms

`WebDeviceVault` supports capability-ranked wrapping methods rather than assuming one API everywhere:

1. WebAuthn PRF-derived wrapping secret when supported and explicitly enrolled;
2. persisted non-extractable WebCrypto wrapping key where support/behavior passes platform tests;
3. passphrase-derived KEK only when the user explicitly selects a passphrase unlock/recovery mode; use a reviewed memory-hard KDF such as Argon2id with versioned calibrated parameters;
4. platform/native keystore when hosted in a native shell.

WebAuthn PRF/WebCrypto are vault wrappers, not OM identity roots.

## 6. Migration between PWA origins/providers

Treat migration as adding a new device:

```text
old client
  -> authenticate new client/device
  -> establish encrypted transfer channel
  -> transfer current AccountControlState + profile state/history according to policy
  -> create new Web Push subscription
  -> verify newest control head
  -> mark new device active
  -> optionally quarantine/retire old web device
```

Never define “copy browser database files to the new origin” as the identity migration protocol.

If the old device/origin is gone, use RecoveryPolicy. If RecoveryPolicy is none, identity loss is an intentional possible outcome.

## 7. Browser eviction

The PWA must detect/communicate whether durable browser storage is granted/available. Browser storage is a cache/device state, not the sole recovery mechanism for a recovery-enabled identity.

Never silently infer “cloud backup exists” from an installed PWA.

## 8. Native transition

PWA -> native uses the same device-link protocol. Native client obtains fresh DeviceIdentity and fresh platform vault keys, then transfers/rebuilds history. It does not inherit web-origin private-key files directly.
