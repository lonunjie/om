# ADR-0271 — Host adapters for the bridge, portability and nearby services in the engineering host (v1)

- **Status:** Delegated design decision (additive; independent review remains required).
- **Date:** 2026-11-27
- **Scope:** `bridges`, `portability` and `local_delivery` host adapters in `apps/web`; no wire, no Core authority change.
- **Authority:** delegated by the operator on 2026-11-26 and 2026-11-27. It follows UI_CORE_PLATFORM_CONTRACT (outcome classes, operation identity, UI-12, UI-13), MULTI_IDENTITY_AND_PROFILE_ISOLATION (ProfileExecutionContext) and ADR-0268, ADR-0269, ADR-0270. It amends no audited text and lowers no floor.

## Decisions

1. **Engineering builds only.** The adapters sit in `apps/web/src/host/scope/` and are constructed only when the engineering host has loaded the `scope-engines` module behind a build flag that is off in every release. A release build neither bundles that module nor imports these adapters; a test fails the release build if either appears. No release host supplies the optional `services.*` of the UI, so the screens stay unoffered (ADR-0268 item 1).
2. **One service per captured profile context.** Each adapter is created with an immutable `ProfileExecutionContext` and never reads an active-profile global; its async continuations stay in that profile.
3. **Authority stays in Core.** The adapter supplies facts and never decides. Room capabilities come from an injected `RoomAuthority`. The engineering default grants nothing, so every admit and remove is shown as blocked (`unauthorized`) until a real source of the member's Room capabilities exists. Failing closed is the safe default.
4. **Confirmation binding is the Core review target.** An admit or remove intent carries the opaque target the user saw (ADR-0270). Core refuses a target that is no longer current (`stale_target`), so a review of one offer cannot approve a changed one. The adapter never mints or caches a target.
5. **Durable operation identity.** Before calling Core the adapter records a pending operation in a profile-namespaced store, keyed by a digest of profile, conversation, intent kind and target. A restart or lost acknowledgement re-reads Core's view: if the effect is visible the operation is `Committed` and the record is cleared; otherwise it is retried under the same identity, never a new one. The outcome shown follows the contract's classes (`Committed`, `PendingReconciliation`, `Rejected(reason)`); a failure never becomes a second operation.
6. **Portability file handling is the host's.** File picking and saving go through an injected `FileHost`. The history candidates come from an injected `HistorySource`; the engineering default is empty, so `state()` reports export as `device_unavailable` rather than inventing a local-history source. Core's archive commands stay stateless. Import never changes live state (ADR-0263); the adapter passes only verified verdicts and live facts the host actually holds, and nothing read from the archive.
7. **Nearby wraps the typed `lanNearby` engine** for the captured profile. The capability row comes from the platform's `localDelivery.probe()`, never from UI probing; the choice is offered only when Core allows it.
8. **Not in v1:** a real Room-capability source, a real local-history source, and adapters in release builds. Each needs its own design and review.

## Why

The UI screens already assume a host that owns authorization, confirmation binding, durable identity and reconciliation. Writing that down keeps those duties in the host, keeps authority in Core, and avoids inventing a history or capability source the design does not yet define.
