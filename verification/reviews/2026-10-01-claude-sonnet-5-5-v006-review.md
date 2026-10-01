# V-006 WebDeviceVault browser-matrix review — Claude Sonnet 5.5, 2026-10-01

- **Reviewer:** Claude Sonnet 5.5 (`claude-sonnet-5-5`), ADR-0167 operator review model (not vendor-independent).
- **Disclosure:** the same agent session ran the probes and wrote this review. A fresh-session re-check of the raw reports under
  [`results/`](../prototypes/v006-web-vault/results/) is recommended before any public release. The operator ran every
  biometric/passkey prompt, so the platform authenticators were real; the drivers (CDP, AppleScript, Marionette) only pressed buttons.
- **Tree:** om `main` after `83b132f`.
- **Scope:** pre-engineering criteria of V-006, which unlocks `reference_client_ui`.

## Evidence

Probe: [`verification/prototypes/v006-web-vault/`](../prototypes/v006-web-vault/README.md). Real-device runs on 2026-10-01: macOS
Chrome 154, Safari 27.0.1 and Firefox 157; iPhone Safari 26.6.1; Android 16 Chrome 154 (OPPO CPH2645) with Google Password Manager
and with Bitwarden. Raw reports are under `results/`. Two exploratory reports are marked as such in their `harness` block (Android
plain-HTTP localhost run with mixed providers; the first iPhone report ran steps on a single origin).

## Findings

| ID | Severity | Summary | Resolution |
|---|---|---|---|
| F-20 | Major | Browsers and providers advertise PRF but return none: `extension:prf` was `true` on iPhone and Android while Bitwarden returned no PRF at create or `get()` | ADR-0172 / R-391: wrapper chosen by an enrollment-time test, never by a capability flag |
| F-21 | Major | No first-release platform offers a device-bound credential that can wrap the vault (macOS and iOS synced only; Android's device-bound credential returns no PRF), so ADR-0129 rule 15's reduced property is the normal PRF case | ADR-0172 / R-391: the UI states the reduced property whenever PRF wrapping uses a synced passkey |
| F-22 | Minor | Safari rejects a ceremony that starts about 1.4 s after another prompt closes ("The document is not focused") | DEVICE_VAULT doc: bounded focus wait; no flow relies on one gesture covering two prompts |
| F-23 | Minor | The probe's own section 5 used an IP as RP ID (always invalid) and its report was lost on the required reload | Fixed in the probe before the runs recorded here |

## Per-criterion disposition

| Criterion | Disposition | Basis |
|---|---|---|
| Browser-matrix prototype with new-origin enrollment migration | **accept** | Passkey-PRF, non-extractable WebCrypto, passphrase layer and recovery-key migration to a second origin pass on every first-release platform |
| Documented behavior for every unsupported/failed capability on the intended matrix | **accept** for the matrix fixed by ADR-0172 | Matrix recorded in PLATFORM_CAPABILITY_MATRIX §3; Windows is excluded and carried pre-release |
| PRF on synced passkeys and `signalCurrentUserDetails` per browser/platform, recovery-key fallback exercised where PRF is unsupported | **accept** | Firefox lacks `signalCurrentUserDetails`; Bitwarden returns no PRF; the recovery-key and WebCrypto paths ran in those cases |
| Device-bound availability, separate credential/salt path, reduced at-rest property shown | **accept** | Distinct outputs for the vault and RecoveryRoot salts and a separate RecoveryRoot credential on every platform; reduced property stated in the DeviceVault doc |
| Loss of the original RP: passkey-only recovery is distribution-bound, origin-independent recovery enrolls a fresh DeviceID | **accept** | Credential rejected at a second hostname on macOS (three browsers) and iPhone (not separately recorded on Android); recovery key restores state and enrolls a fresh DeviceID and key at a different origin on every first-release platform |

## Limits

- Windows Chrome/Edge and a physical security key were not tested.
- The iPhone sheet's provider name was not confirmed; PRF returned only with Bitwarden not selected.
- Origin binding on Android relies on the WebAuthn RP-ID rule; no Android run recorded it.
- `storage.persist()` varied (Android granted in one run, denied in another); the client must not depend on it.

## Result

V-006 passes pre-engineering for the ADR-0172 first-release matrix under the ADR-0167 model. The pre-release criterion (published
support matrix with regression tests on each supported browser, now also covering Windows or its removal) stays open.
`reference_client_ui` is eligible for its scoped unlock ADR.
