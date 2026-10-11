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

- where the user owns the origin/account, no universal OM origin can ship code to them (most T0/T1 users do not own one and use a shared ClientHost; its controls are in [`CLIENTHOST_UPDATE_LIFECYCLE.md`](CLIENTHOST_UPDATE_LIFECYCLE.md) §9);
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

Each profile gets an independent random `ProfileVaultKey` (PVK). Derive independent subkeys with a domain-separated KDF, e.g. (the labels below are **implementation-private local labels: not registered, not interoperable**; they never appear on the wire or in archives, are not part of [`om-domain-separation.json`](../../../spec/registries/om-domain-separation.json), and an implementation may use different ones, ADR-0129):

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
3. passphrase-derived KEK only as an **additional** wrapping factor that the user explicitly selects, layered over method 1, 2 or 4; use a reviewed memory-hard KDF such as Argon2id with versioned calibrated parameters. A passphrase is never the sole wrapper of a production DeviceIdentity (matching the 2026-09-23 resolution below). The web construction, parameters, floor and honest limits are fixed by ADR-0180 (R-409);
4. platform/native keystore when hosted in a native shell.

WebAuthn PRF/WebCrypto are vault wrappers, not OM identity roots.

**Vault wrapping is separate from the recovery custodian (ADR-0129).** ADR-0111 makes a synced passkey's PRF the default RecoveryRoot custodian. The vault must not rest on the same secret:

1. Where the platform offers a device-bound (non-synced) credential (a platform authenticator marked device-bound, a security key, or the native keystore), the vault is wrapped with it.
2. In every case the vault uses a WebAuthn credential or PRF salt and label distinct from the RecoveryRoot's, so one PRF output never serves both.
3. Where the only available credential is a synced passkey, the client uses a separate synced credential or at least a separate salt, and states the reduced property: someone who controls the passkey provider account and obtains a copy of the browser profile can open local state. The recovery delay protects account authority, not local plaintext.

V-006 records, per browser/platform, whether a device-bound credential is available and exercises the separate-salt path.

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

If the old device/origin is gone, use RecoveryPolicy, which is on by default (ADR-0111). If recovery was explicitly turned off, identity loss is a possible outcome.

## 7. Browser eviction

The PWA must detect/communicate whether durable browser storage is granted/available. Browser storage is a cache/device state, not the sole recovery mechanism for a recovery-enabled identity.

Never silently infer “cloud backup exists” from an installed PWA.

## 8. Native transition

PWA -> native uses the same device-link protocol. Native client obtains fresh DeviceIdentity and fresh platform vault keys, then transfers/rebuilds history. It does not inherit web-origin private-key files directly.
## 2026-09-22 update-boundary clarification

ClientHost software update integrity and DeviceVault identity continuity are separate. A compromised origin cannot bootstrap its own trust merely by running an in-origin signature verifier. Recovery may require a clean/new ClientHost or higher-assurance endpoint plus authenticated device enrollment. See [`CLIENTHOST_UPDATE_LIFECYCLE.md`](CLIENTHOST_UPDATE_LIFECYCLE.md).

## 2026-09-23 WebDeviceVault resolution

Production PWA identity storage uses WebAuthn PRF-derived wrapping where supported, otherwise non-extractable WebCrypto root material. A browser that cannot provide either durable cryptographic boundary is not silently treated as equivalent for long-lived production DeviceIdentity. Passwords may augment locking but are not the identity-key storage primitive. Feasibility matrix is `V-006`.

## 2026-10-01 V-006 browser-matrix resolution (ADR-0172)

Measured on macOS (Chrome, Safari, Firefox), iOS Safari and Android Chrome ([`PLATFORM_CAPABILITY_MATRIX.md`](PLATFORM_CAPABILITY_MATRIX.md) §3):

1. **Wrapper selection is tested, never assumed.** At enrollment the client creates the credential with a PRF salt, accepts method 1
   only if the authenticator reports `prf.enabled` and a later `get()` returns a PRF output for the vault salt, and otherwise uses
   method 2 (non-extractable WebCrypto key). Browser capability flags (`extension:prf`) are hints, not evidence: third-party
   providers advertise PRF yet return none.
2. **Chained prompts.** A ceremony started right after another prompt closes can fail with "The document is not focused" (Safari,
   about 1.4 s). Enrollment and unlock wait for document focus (bounded) before each ceremony and never depend on one user gesture
   covering two prompts.
3. **The reduced at-rest property is the normal PRF case, not an edge case.** No first-release platform offered a device-bound
   credential that can evaluate PRF. When the vault is wrapped with a synced passkey's PRF, the UI says the ADR-0129 rule 15
   property: someone who controls the passkey provider account and obtains a copy of the browser profile can open local state.
   Where a device-bound credential exists but cannot evaluate PRF (Android non-discoverable), it is not used for vault wrapping.
4. **Migration works as an origin-independent device add.** A recovery key (the RecoveryRoot custodian, not a passkey) at an
   unrelated origin restores account state, enrolls a fresh DeviceID and a fresh non-extractable device key, and wraps a new vault
   there; passkeys from the old origin are rejected at the new RP ID, so passkey-only recovery is distribution-bound.
5. **Windows Chrome/Edge is untested.** It is not in the first-release matrix until the probe is run there (pre-release).

