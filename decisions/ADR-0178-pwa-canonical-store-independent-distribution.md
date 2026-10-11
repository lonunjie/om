# ADR-0178 — PWA is the canonical client; native shells are thin hosts; store-independent distribution

- **Status:** Accepted (operator direction, 2026-10-11)
- **Date:** 2026-10-11
- **Amends:** ADR-0007 and ADR-0044 ("PWA first, not PWA only") — the PWA moves from *first* to *canonical*; ADR-0152 §1 only where it implies native-first capability
- **Does not amend:** OM wire formats, OM-Crypto-v1, AccountControlState, the `PlatformAdapter` contract, or the V1 reference-family list (ADR-0150/0152)

## Context

Store and platform gatekeepers are now a demonstrated takedown and capture surface for exactly the apps OM resembles (press reports, volatile; re-verify before release):

- **Bitchat** was removed from the Apple App Store in India on 2026-10-03 under a MeitY Section 69A demand, after GitHub takedown orders in July and a China removal in April. TestFlight links also stopped working there.
- **Android developer verification** began enforcement on 2026-09-30 in Brazil, Indonesia, Singapore and Thailand, with global rollout planned for 2027. It applies to sideloaded apps on certified devices, so direct APK and F-Droid-style distribution is no longer a guaranteed escape.

The architecture already says store work must not block OM-native delivery (absolute rule 19) and that shells carry no business logic (shared-UI constraints). What it leaves open is whether the PWA is a stepping stone or the product. This ADR settles it.

## Decision

1. **The PWA is the canonical client.** Every user-facing OM capability is specified and shipped in the PWA first, capability-gated through `PlatformAdapter`. The web build is the behavioral reference. A feature that only a native shell can offer must be a *platform capability* (a system surface or permission), never a product or protocol feature.
2. **Native shells are thin hosts of the same signed bundle.** Android, iOS and desktop shells embed the same `om-web` bundle (shared `om-ui` + `om-core` compiled to WASM) in a system web view and add only `PlatformAdapter` capabilities: hardware-backed key wrapping, background wake/push, native call UI, contacts/camera/notification permissions, secure-surface flags, and share-sheet/file integration. No messaging, crypto, session, recovery or provider logic lives in Kotlin or Swift. Native FFI to `om-core` stays permitted where a capability needs it, but is an optimization, not the default.
3. **Shells never load app code from a live remote origin.** The bundle is packaged in the shell and verified by the same ClientHost update lifecycle, owner-root and transparency rules as the PWA (ADR-0077, ADR-0130). A shell that navigates to a mutable origin for application code would turn one origin or hosting-account compromise into total compromise and is non-conformant.
4. **Distribution order of preference.** (a) PWA from any origin, including self-hosted and the reference Cloudflare Pages deployment; (b) direct signed desktop and Android builds, and third-party or community stores; (c) Play Store and App Store listings, as optional, distribution-local conveniences. Nothing in (c) may be required for any feature, onboarding path or update. On iOS the Home Screen PWA is the primary path; an App Store shell is optional and its loss must not remove any capability that the PWA offers.
5. **Takedown resilience is a product property.** The PWA must install and update from any conforming origin; a user must be able to move to a different origin by device link/transfer (not IndexedDB copying); and the Home flow must not hard-code a single origin or store URL. Invitations and onboarding never depend on a store listing.
6. **Honest limits stay honest.** Where the PWA is weaker (screenshot blocking, backup exclusion, VoIP wake, background guarantees — see [`PLATFORM_CAPABILITY_MATRIX.md`](../docs/architecture/client/PLATFORM_CAPABILITY_MATRIX.md)), the UI reports it from the runtime capability probe and Hardened/Maximum recommend a native shell. Making the PWA canonical does not upgrade any of these claims.

## Consequences

- The `reference_client_ui` scope (ADR-0172) builds web first against the already-measured macOS, iOS and Android web matrix; Android/iOS shells are a later, smaller scope than previously implied.
- `groups` (ADR-0177) is unaffected in wire terms and supported in practice: OpenMLS 0.9.0 passes the V-004 probe on `wasm32-unknown-unknown`, so the PWA runs the same group code as any shell.
- Android wake: the shell exposes `unifiedpush`/`persistent`/`fcm` adapters (ADR-0152); the PWA uses standards Web Push. None is protocol authority.
- Store/regulator exposure moves from the product to optional distribution channels. It does not remove the hosting-account and origin risk of the PWA itself, which stays governed by ADR-0077 and [`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](../docs/architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md) §6.
- New pre-release item: the support matrix and Android developer-verification status must be re-checked against official platform documentation before any public release (AGENTS "External facts").

## Where did authority go?

Nowhere new. No store, origin or shell is a protocol authority; each remains a replaceable distribution (ADR-0130).
