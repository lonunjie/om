# V-006 WebDeviceVault browser-matrix probe

Feasibility prototype for gate V-006 ([`DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`](../../../docs/architecture/client/DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md) §4–§6).
Not product code. `index.html` is a single static page that tests, on whatever browser opens it:

| Section | V-006 criterion |
|---|---|
| 1. Capabilities | documented behavior per browser (PRF, `signalCurrentUserDetails`, platform authenticator, persistent storage) |
| 2. Non-extractable WebCrypto key | wrapping method 2; key survives reload through IndexedDB |
| 3. WebAuthn PRF | wrapping method 1; PRF at create/get; device-bound vs synced credential (backup-eligible flag); vault and RecoveryRoot salts give different outputs; separate RecoveryRoot credential |
| 4. Passphrase layer | an additional factor over the device wrapper (PBKDF2 stand-in; production uses Argon2id) |
| 5. Origin binding | a credential from another origin fails here, so passkey-only recovery is distribution-bound |

## Browser matrix

Run it on each browser/platform the first reference PWA intends to support: Chrome, Safari and Firefox on macOS,
Chrome/Edge on Windows, Safari on iOS, and Chrome on Android. WebAuthn needs a secure context: `http://localhost` works
on the same machine; phones need an HTTPS origin.

On a Mac, from the `om` repository root:

```bash
python3 -m http.server 8806 -d verification/prototypes/v006-web-vault
```

Then open `http://localhost:8806` in each desktop browser. For origin binding, also open `http://om-probe.localhost:8806` (a
different RP ID; an IP address such as 127.0.0.1 is rejected outright as an RP ID, so it cannot test this) and paste the vault credential ID from the first report.

## Recording results

On each browser, press every button in order (1 → 5), then **Download report**. Save each report as
`results/<platform>-<browser>.json` in this folder. A run with a failed or unsupported capability is still a result:
V-006 needs the documented behavior, not only successes. The gate passes when the matrix covers the intended first-release
browsers and each criterion has a recorded outcome.

## Status

Recorded 2026-10-01 on macOS (iCloud Keychain, Touch ID), run through a driver (CDP / AppleScript / Marionette) with the operator
touching the sensor. Chrome and Firefox ran in fresh temporary profiles; Safari in the default profile. Sections 1-5 per browser:

| Capability | Chrome 154 | Safari 27.0.1 | Firefox 157 |
|---|---|---|---|
| WebAuthn PRF at create and get | yes | yes | yes |
| Vault and RecoveryRoot salts give different outputs | yes | yes | yes |
| Separate RecoveryRoot credential with its own PRF | yes | yes | yes |
| Passkey is device-bound | **no**: synced (BE=1 BS=1) | **no**: synced | **no**: synced |
| `signalCurrentUserDetails` | yes | yes | **no** |
| Non-extractable WebCrypto key survives reload | yes | yes | yes |
| `storage.persist()` granted | no | no | yes |
| Passphrase layer over the device wrapper | yes | yes | yes |
| Credential rejected at a different RP ID (`om-probe.localhost`) | yes | yes | yes |

Findings the design must absorb (ADR-0129 rule 15 reduced-at-rest case): on macOS all three browsers store the platform passkey
in iCloud Keychain, so a device-bound vault credential is not available from the platform authenticator on this setup; the
vault credential is synced. Safari rejects a ceremony that starts right after a previous prompt closes ("The document is not
focused") for about 1.4 s, so chained prompts need a focus wait or a fresh user gesture (the probe now waits).

A CDP virtual authenticator run on 2026-10-01 only logic-checked the page and is not evidence.

### Device-bound credential and new-origin migration (sections 6-7)

macOS (Chrome, Safari, Firefox): every platform-authenticator request variant returned a synced credential (`BE=1 BS=1`); the
security-key and phone variants only time out without a key present (Safari's security-key hint and Firefox/Chrome's hybrid hint
fall back to a synced platform passkey). Migration passes everywhere: a recovery key at an unrelated origin restores the state,
enrolls a fresh DeviceID and non-extractable device key, wraps a new vault with a new PRF credential, and the vault reopens.

### Phones (2026-10-01; HTTPS via temporary trycloudflare tunnels, two hostnames as old/new origin)

| Capability | iPhone, Safari 26.6.1 | Android 16, Chrome 154 + Google Password Manager | Android + Bitwarden |
|---|---|---|---|
| PRF at create / get | **not returned** (Bitwarden on; iCloud-only retry inconclusive, see below) | yes | **not returned** |
| Passkey device-bound (default request) | no: synced | no: synced | no: synced |
| Non-discoverable platform request | synced | **device-bound (`BE=0`)**, but PRF not enabled at create | device-bound (`BE=0`) |
| Non-extractable WebCrypto key survives reload | yes | yes | yes |
| `signalCurrentUserDetails` | yes | yes | yes |
| New-origin migration (recovery key at second hostname) | yes (fell back to WebCrypto wrapper, no PRF) | yes (fell back; run with Bitwarden) | yes |
| Credential rejected at a different RP ID | yes | not re-run | not re-run |

Findings: PRF availability depends on the passkey provider the user has selected, not only on the browser. Third-party providers
(Bitwarden on both phones) returned no PRF, so the capability-ranked fallback (non-extractable WebCrypto wrapper) is required and
works. On Android a device-bound credential exists only for the non-discoverable request, and it did not enable PRF at create, so
it cannot yet serve as a PRF vault wrapper. The first Android pass over plain-HTTP localhost gave intermittent `NotReadableError`
from the credential manager; the HTTPS runs did not. Safari on iPhone with the iCloud provider alone did not demonstrate PRF in
the one retry (the operator could not confirm which provider showed in the sheet), so iOS + iCloud Keychain PRF is still open.

**V-006 stays open.** Not yet covered: Windows (Chrome/Edge: no device available); iOS Safari with iCloud Keychain as the only
provider (PRF unconfirmed); the Android device-bound credential's PRF behavior at get(); a physical security key; the origin-independent
recovery path enrolling a fresh DeviceID at a *native* client; and independent review. Results persist across the section 2 reload;
**Reset saved results** starts a fresh run. `collector.py` serves the page and receives reports from phones (token-gated).
