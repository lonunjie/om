# ADR-0172 — V-006 result, first-release web matrix and `reference_client_ui` authorization (W0)

- **Status:** Accepted
- **Decided by:** operator, 2026-10-01 ("Ship matrix: macOS, iOS, Android")
- **Date:** 2026-10-01
- **Authorizes:** engineering scope `reference_client_ui` only
- **Basis:** [`2026-10-01-claude-sonnet-5-5-v006-review.md`](../verification/reviews/2026-10-01-claude-sonnet-5-5-v006-review.md), ADR-0167, ADR-0170, ADR-0171

## Context

`reference_client_ui` depends on `core_kernel` (ADR-0169). V-021 and V-023 passed pre-engineering (ADR-0170), V-018 moved to
pre-release (ADR-0170), and V-022 passed. V-006 was the last blocker. It needed real passkey ceremonies, which the operator ran on
macOS Chrome, Safari and Firefox, an iPhone and an Android phone on 2026-10-01 (see the review).

## Decision

1. **First-release web matrix.** The first reference PWA supports macOS Chrome, Safari and Firefox, iOS Safari and Android Chrome.
   Windows Chrome/Edge is **not tested**: it is not in the first-release matrix, the non-extractable-WebCrypto fallback is expected
   to work there but is not claimed, and running the V-006 probe on Windows (or removing Windows from the product) is a V-006
   pre-release item.
2. **Wrapper selection by test (R-391).** The client uses WebAuthn PRF wrapping only after the authenticator reports `prf.enabled` and
   a later `get()` returns a PRF output for the vault salt; otherwise it uses the non-extractable WebCrypto wrapper. A browser
   capability flag is never proof, because third-party passkey providers advertise PRF yet return none.
3. **Reduced at-rest property (R-391).** No first-release platform offers a device-bound credential that can wrap the vault. Whenever
   the vault is wrapped by a synced passkey's PRF, the UI states the ADR-0129 rule 15 property. An Android device-bound credential
   that cannot evaluate PRF is not used for vault wrapping.
4. **Migration (R-392).** Adding a device at an unrelated origin works through the origin-independent recovery path (recovery key,
   fresh DeviceID, fresh device key, new vault wrapper), and passkey-only recovery stays distribution-bound. The client keeps this
   path testable on every first-release platform and never relies on `storage.persist()` or on one gesture covering two prompts.
5. `reference_client_ui` is authorized: om-ui shell, onboarding, DeviceVault, PWA ClientHost, V1 IdentityProfile switcher and the
   voice-note and one-shot location-card UX, built against
   [`UI_CORE_PLATFORM_CONTRACT.md`](../docs/architecture/ux/UI_CORE_PLATFORM_CONTRACT.md) and the `reference_client_ui` entry of
   [`machine/engineering_scope_contracts.json`](../machine/engineering_scope_contracts.json).

## Consequences

- `reference_client_ui.authorized=true` with `authorizing_adr=ADR-0172`. Authorized scopes: `core_kernel`, `provider_reference`,
  `reference_client_ui`.
- V-006's pre-release criteria stay open: the published support matrix with regression tests on each supported browser, including the
  Windows decision above. V-018's participant study and the V-021 forensic audit still block any public or beta release.
- This passes under the ADR-0167 review model, not vendor-independent review, and the evidence executor and the reviewer were one
  agent session (disclosed in the review).
