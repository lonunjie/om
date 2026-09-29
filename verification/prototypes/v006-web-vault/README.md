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

Then open `http://localhost:8806` in each desktop browser. For origin binding, also open `http://127.0.0.1:8806` (a
different RP ID) and paste the vault credential ID from the first report.

## Recording results

On each browser, press every button in order (1 → 5), then **Download report**. Save each report as
`results/<platform>-<browser>.json` in this folder. A run with a failed or unsupported capability is still a result:
V-006 needs the documented behavior, not only successes. The gate passes when the matrix covers the intended first-release
browsers and each criterion has a recorded outcome.

## Status

The automatic parts (capabilities, WebCrypto key persistence, passphrase layer) were exercised in Chromium 152 on
2026-09-29. The PRF ceremonies need a real user-verifying authenticator and have not been run yet.
