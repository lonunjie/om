# `reference_client_ui` — first web tasks

- **Date:** 2026-10-11
- **Scope:** engineering scope `reference_client_ui` (ADR-0172), PWA-canonical per ADR-0178
- **Status:** working plan, non-normative. It authorizes nothing and changes no design; the scope contract in [`machine/engineering_scope_contracts.json`](../../machine/engineering_scope_contracts.json) and the sources it lists stay authoritative.

## Starting point

No product code exists yet: there is no `Cargo.toml`, `packages/` or `apps/` in the repository. The layout to create is in [`ENGINEERING_REPOSITORY_LAYOUT.md`](../process/ENGINEERING_REPOSITORY_LAYOUT.md) (`packages/om-ui`, `om-design`, `om-platform-api`, `om-web-platform`, `apps/web`; React + TypeScript strict + Vite; pnpm only).

The scope's only integration prerequisite is a **Core command/view harness**; a live provider is optional. So UI work does not wait for `core_kernel`. It runs against a typed, clearly non-production harness that is later swapped for the real WASM bridge.

## Shape of the first slice

A walking skeleton in the first-release matrix (macOS Chrome/Safari/Firefox, iOS Safari, Android Chrome) that does five things:
1. installs as a PWA;
2. creates a DeviceVault using the tested wrapper selection (R-391);
3. renders a conversation list, conversation and composer from the harness;
4. switches IdentityProfiles with namespaced state;
5. ships as a signed, hash-manifested bundle that a native shell can embed unchanged (ADR-0178, R-407).

## Tasks

Order is by dependency. `A` tasks need no Core; `B` tasks need the harness; `C` is integration.

### UW-0 (A) Workspace bootstrap — do first

- pnpm workspace, strict TS, Vite, React, Vitest, Playwright. The npm guard blocks `npm`/`npx`.
- Lint and CI rules from the layout doc: no `dangerouslySetInnerHTML`, no `any` across Core/Platform boundaries, no remote CDN, fonts or themes, restrictive CSP and Trusted Types in the production config. Malformed deep links, CSP failures and theme validation failures fail closed.
- Wire into `make check`/`make verify`, REUSE and licensing.
- `tools/om_registry.py sync` classifies by folder, so `node_modules/`, `dist/` and lockfile handling must be settled first (ignore rules, or the registry will explode).
- **Done when:** an empty `apps/web` builds, serves under the production CSP, and the lint rules fail a deliberate violation.

### UW-1 (A) Typed Core boundary and harness

- `om-platform-api`/`om-ui` types for intents, queries and view models, from [`UI_CORE_PLATFORM_CONTRACT.md`](../architecture/ux/UI_CORE_PLATFORM_CONTRACT.md) and [`ACTION_TO_CORE_INTENT_MAP.md`](../architecture/ux/ACTION_TO_CORE_INTENT_MAP.md).
- Outcome classes `Committed | PendingAuthorization | PendingNetwork | PendingReconciliation | NeedsUserAction | Rejected(reason)`, with provider-neutral reason classes.
- An Action Registry that can only emit typed intents, never string RPC.
- A **dev-only harness** implementing the same interface. Its crypto is unmistakably non-production and impossible to ship (build-time guard plus a visible marker).
- The harness must be able to simulate uncertain network outcomes and process restarts, so the following can be tested: UI-01, UI-03, UI-12, UI-13.
- **Done when:** UI tests pass those four cases and no UI code path writes key, provider or authority state.
- **Coordination:** agree the harness interface with whoever builds `om-wasm` so the swap in C1 is mechanical.

### UW-2 (A) PlatformAdapter and web implementation

- Capability families from the contract, each reporting `available | partial | unavailable` from runtime probes, never from user-agent checks.
- `om-web-platform` implements what browsers can do. It must report honestly where they cannot (screenshot blocking, backup exclusion, app-switcher blanking, clipboard sensitivity, per [`PLATFORM_CAPABILITY_MATRIX.md`](../architecture/client/PLATFORM_CAPABILITY_MATRIX.md)).
- Design the adapter as shell-agnostic: a native shell will later supply a different implementation of the same interface (ADR-0178 rule 2).
- **Done when:** UI-02 passes (an unavailable capability degrades explicitly with no semantic or security fallback) and the probe output matches the measured V-006/V-021 matrix on the five target browsers.

### UW-3 (A) WebDeviceVault and ClientHost — highest risk

- `DeviceVault` per [`DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`](../architecture/client/DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md) §4–5 and the ADR-0172 resolution:
  - per-profile random PVK with domain-separated local subkeys;
  - wrapper chosen by test (PRF salt, `prf.enabled`, a later `get()` that returns output), never by capability flag;
  - non-extractable WebCrypto fallback;
  - bounded wait for document focus before each ceremony (Safari "document is not focused");
  - never one gesture for two prompts;
  - no reliance on `storage.persist()`.
- Show the reduced at-rest property in the UI whenever a synced passkey's PRF wraps the vault (R-391).
- Vault salt and credential are distinct from the RecoveryRoot custodian's.
- Origin-independent add-device path (recovery key, fresh DeviceID, fresh key, new wrapper) kept testable on every platform (R-392).
- IndexedDB behind an encrypted storage adapter; browser-eviction handling per §7.
- Reuse `verification/prototypes/v006-web-vault` as the starting probe, and keep its recorded results as regression fixtures.
- **Done when:** enrollment, unlock, lock and eviction recovery pass on the five browsers; the recovery-key add-device flow works at a second origin; no key material is reachable from UI code.
- **Coordination:** the seal/open interface sits at the `om-storage-api` boundary of `core_kernel`; confirm its shape before implementing. Keep this task on the strongest model and give it a review pass, since it is security-sensitive.

### UW-4 (A) Service worker, app shell and signed bundle

- Service worker limited to app shell, wake handling and explicitly designed safe retries; correctness never depends on it staying alive.
- Update flow per [`CLIENTHOST_UPDATE_LIFECYCLE.md`](../architecture/client/CLIENTHOST_UPDATE_LIFECYCLE.md): versioned releases, hash manifest, verification hook in the client, freeze/rollback handling.
- Make the production build a **reproducible bundle with a manifest and a signature slot**. This is the artifact native shells embed (ADR-0178 rule 3), so define the format now.
- Release transparency (Rekor-compatible profile, ADR-0133) can stay a stub with a clear hook until pre-release.
- **Done when:** the bundle rebuilds byte-identically from a clean checkout; an update with a tampered file is rejected; offline start works; the app is installable on all five browsers.

### UW-5 (B) Design system and AppShell

- `om-design` tokens, light, dark and Auto, `prefers-reduced-motion`, semantic motion only.
- One built-in theme first (OM Calm); the other three in ADR-0162 follow. Theme import is later (milestone 1 explicitly has no theme ecosystem).
- Reserved security surfaces that themes cannot hide or spoof (UI-04).
- Adaptive AppShell: Chats as the compact root, no one-item tab bar, Omnibar trigger sharing the composer zone, expanded layouts for tablet and desktop.
- Accessibility primitives: focus visibility, targets, screen-reader names.
- **Done when:** automated a11y checks and visual-regression baselines exist for the shell and primitives; a11y regressions block CI (accessibility conformance suite is release-blocking).

### UW-6 (B) IdentityProfile namespace and switcher

- All local state (vault, view cache, drafts, adaptive navigation) namespaced by IdentityProfile from the first schema; no hidden identifier linking profiles.
- Immutable `ProfileExecutionContext` captured at dispatch for every profile-sensitive action, search, callback and notification action.
- Switcher without logout or rekey.
- **Done when:** UI-06, UI-07 (ask which profile acts on an unknown deep link), UI-10 and UI-11 pass.

### UW-7 (B) First run and onboarding

- Invite-first zero-contact first run: share invite, show QR, scan QR, accept an invitation. No address-book upload.
- Vault enrollment and recovery-key creation as plain-language steps, with Home tier facts available when needed (T0/T1 observation, migration, client-origin choice).
- Applies the user-facing test: no Wi-Fi-configuring knowledge needed.
- **Done when:** a new user reaches an empty Chats screen with a working vault and a saved recovery key on each platform, and the flow passes the accessibility suite.

### UW-8 (B) Conversation slice

- ConversationList, ConversationView, Composer, MessageRequest, delivery-state rendering, security-notice rendering.
- RichTextV1 semantic rendering only; no message HTML. Decrypted files are hostile: validate before preview (images and audio fuzzed with the `fuzz/` targets as they appear).
- Pending states survive restart; a timeout never shows as a terminal failure (UI-12, UI-13 in real screens).
- **Done when:** a scripted two-profile conversation runs end to end on the harness, including a restart mid-send.

### C1 (C) Swap harness for real Core; two-browser messaging

Blocked on `om-wasm` from `core_kernel`. Replace the harness behind the same interface and run PWA milestone P1 (two browsers on different machines message across restarts). Keep the harness for UI tests.

### C2 (C) Shell-readiness check

Embed the signed bundle in a throwaway Android web view and an iOS `WKWebView` (not a product shell) to prove nothing in the bundle assumes a browser tab, a remote origin or a service worker the shell cannot host. This validates ADR-0178 early and costs little.

## Deferred past this slice

Groups screens (the `groups` scope is now open and depends on Core group state), voice-note and location-card UX (UI-08, UI-09), theme and font import, Spaces/Publications, calls, Settings sub-screens (Devices, Protection, Home health). Devices and Recovery come next; the device-link flow must keep the `NoSilentLink` invariant.

## Suggested parallelism

After UW-0: run UW-1, UW-2, UW-4 and UW-5 in parallel; UW-3 starts once UW-2's interface exists; UW-6 follows UW-1 and UW-3; UW-7 and UW-8 follow UW-5 and UW-6. Keep concurrency modest because the subagents share one rate limit.

## Decisions for you

1. **Where the code lives.** The layout doc assumes a monorepo at the `om` repository root, which today is a design-only repository (1,456 registered files). Recommended: keep it there as the layout says, and settle the registry and ignore rules in UW-0. The alternative is a separate `om-app` repository registered in `registry/repos.toml`, at the cost of splitting atomic changes across protocol, specs and code.
2. **Product-code review rule.** Under ADR-0167 a Claude review pass is the only reviewer. For UW-3 in particular, consider asking for an independent human or cross-vendor read before any release; this does not block starting.

## Pre-release items this plan does not close

V-006 (published support matrix with regression tests per browser; Windows decision), V-018 participant study, V-021 forensic audit on real devices.

## Status (2026-10-11, end of session)

UW-0 through UW-9 are built and verified: 209 tests, `make check`, `pnpm web:check`, reproducible bundle, harness excluded from production. Commits `8336101`, `dcd88df`, `90bcab8` and the UW-9 commit that follows them. The full flow (onboarding with the real WebDeviceVault, shell, profile switch, send) was run in a real browser against the dev-only Core.

| Task | State |
|---|---|
| UW-0 workspace, UW-1 Core boundary + harness, UW-2 PlatformAdapter, UW-4 service worker + signed bundle, UW-5 design/shell, UW-6 profiles, UW-7 onboarding, UW-8 conversations, UW-9 composition + English copy | done |
| UW-3 DeviceVault | done and lead-reviewed; passphrase factor blocked on a reviewed Argon2id dependency |
| C1 swap harness for real Core | blocked on `core_kernel` `om-wasm` |
| C2 shell-readiness (bundle in Android web view / `WKWebView`) | not started |

Open items for the next session:
1. Service worker and PRF passkey paths need a real-browser check (Chrome, Safari, Firefox, iOS, Android); the in-app pane supports neither. Playwright is not installed.
2. Decide on QR encode/decode and Argon2id dependencies (onboarding hides QR/scan without them).
3. ~~Reconcile the theme name~~ done 2026-10-11: ADR-0179 names the default theme OM Calm and the docs were updated.
4. Release keys and the real trust root for the signed bundle (production builds are unsigned and the service worker refuses them by design).
5. Next tasks: C2, Devices and Recovery settings screens, groups screens, voice-note and location-card UX.

## Status update (2026-10-11, second session)

Commits `8228773` (ADR-0179) and `507a128`. Typecheck, lint, 273 tests, `make check`, production build, no-harness check and the reproducible-bundle check pass; QR, onboarding, Settings and Devices were run in the in-app browser.

- **Done:** Argon2id passphrase factor (`@noble/hashes` 2.4.0; nested wrap, floor m=19 MiB/t=2, default m=64 MiB/t=3, NFKC), opt-in in `ProtectDevice`, `UnlockVault` screen; QR show/scan (`@paulmillr/qr` 0.3.0), Permissions-Policy now `camera=(self)`; Settings > Devices and Recovery on typed Core queries/intents, with harness support.
- **Not built:** change/add/remove passphrase after setup; `UnlockVault` and `LinkThisDevice` are not wired into the app flow (no production Core yet); replace recovery key, "that was me" recovery approval, lift suspension, promote device (no intents yet).
- **Needs a real browser/device:** camera scan, PRF passkey path and service worker (the in-app pane has no authenticator or camera); Argon2id timing on a low-end phone.
- **Open:** whether the nested passphrase wrap needs an ADR/traceability entry (labels are implementation-private); spec says "Suspend is one tap" but the UI adds a confirmation (kept); release keys and trust root; C1 and C2 still open.
