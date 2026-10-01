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

**V-006 stays open.** Not yet covered: Windows (Chrome/Edge), iOS Safari, Android Chrome (need HTTPS); a device-bound credential
path (security key or non-synced profile); new-origin enrollment migration; the origin-independent recovery path enrolling a fresh
DeviceID; and independent review. Results persist across the section 2 reload; **Reset saved results** starts a fresh run.
