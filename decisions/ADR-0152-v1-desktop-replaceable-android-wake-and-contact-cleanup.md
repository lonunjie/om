# ADR-0152 — Promote desktop, replaceable Android wake and local contact cleanup into V1

- **Status:** Accepted
- **Date:** 2026-09-28
- **Scope:** Run 9A final low-risk V2/later-to-V1 promotion; executable/adversarial evidence deferred to Run 9B
- **Builds on:** ADR-0038, ADR-0040, ADR-0044, ADR-0051, ADR-0055, ADR-0060, ADR-0129, ADR-0148, ADR-0149 and ADR-0150
- **Amends:** ADR-0150 only where it separately sequenced native desktop shells after the V1 reference family
- **Does not amend:** OM wire formats, OM-Crypto-v1, AccountControlState authority, provider correctness semantics, iOS/APNs lifecycle realities, or the rule that OS address-book data is never identity authority

## Context

After Run 8B, the remaining deferred set was audited one last time. Most remaining items are genuinely later because they add cryptographic, authority, privacy, ecosystem or operational risk. Three items are different: the architecture already contains their hard boundaries and postponing them would mostly defer implementation validation rather than reduce protocol risk.

1. Desktop already uses the same `om-core` + `om-ui` + `PlatformAdapter` architecture, has explicit endpoint-leakage controls, update requirements, multi-window profile-context rules and performance targets. Keeping `om-desktop` post-V1 would allow mobile/web assumptions to harden before desktop lifecycle and filesystem/window behavior are exercised.
2. Android push is already capability-addressed and [`UI_CORE_PLATFORM_CONTRACT.md`](../docs/architecture/ux/UI_CORE_PLATFORM_CONTRACT.md) already names `unifiedpush`. Treating non-FCM wake as merely future is inconsistent with OM's operatorless goal. V1 can standardize replaceable wake adapters without promising that every distributor supports every adapter.
3. PersonRecord already separates local human organization from cryptographic identity and already tracks stale/reassigned aliases. A safe local cleanup/export workflow adds useful V1 hygiene without adding a network object or continuity authority.

## Decisions

### 1. Native desktop is a V1 reference-family target

The V1 reference family is:

```text
PWA        first vertical slice
Android    early native validation target
iOS        V1 native validation target
Desktop    V1 native validation target
```

Normative boundaries:

1. `om-desktop` hosts the same portable Core and shared UI contracts as PWA/mobile. It does not reimplement identity, crypto, session, event, recovery, provider or profile-isolation semantics.
2. Desktop-specific behavior is expressed through `PlatformAdapter` capabilities such as secure storage, windowing/multi-window, notifications, filesystem/share integration, clipboard/privacy controls and update integration.
3. Independent windows capture immutable `ProfileExecutionContext`; a process-global active-profile selector never chooses storage/security namespace for already-running work.
4. macOS, Windows and Linux capability differences are reported honestly. A weaker Linux keyring/screenshot/backup surface cannot cause a weaker protocol or silent security fallback.
5. "V1 target" does **not** require PWA, Android, iOS, macOS, Windows and Linux binaries to launch on the same date. The first public V1 release may stage shells/OSes, but the shared contracts and reference-client engineering must treat desktop as a V1 constraint rather than a post-V1 redesign.
6. Desktop update/signing remains distribution-local and follows the same owner-root, rollback/freeze and transparency principles already required for reference distributions.

### 2. Android V1 has replaceable wake adapters and at least one non-FCM path may ship

Android background wake is a `PlatformAdapter`/distribution capability, never protocol correctness or identity authority.

The V1 capability vocabulary remains:

```text
backgroundPush = fcm | unifiedpush | persistent | none
```

A reference Android distribution MAY support one or more adapters and SHOULD support at least one practical non-FCM path when it can do so without misleading reliability/battery claims. No particular non-FCM provider is mandatory for protocol conformance.

Rules:

1. **FCM is convenience, not dependency.** Disabling or replacing FCM cannot change AccountID, DeviceID, RelationshipBinding, relay mailbox, EventID or message correctness. Pending ciphertext remains fetchable from ordinary OM delivery infrastructure.
2. **UnifiedPush-compatible path is replaceable.** A distributor/distributor-app/gateway is a `PushProvider`/adapter boundary. It receives only the minimum opaque wake routing material and declares timing/IP/token/profile-linkage metadata in its MetadataExposureDescriptor.
3. **Persistent path is bounded and honest.** A persistent encrypted relay/Home connection may keep a device responsive without FCM where the OS permits. It is authenticated as an ordinary endpoint transport, cannot carry new authority, and its battery/network/background limitations are surfaced rather than assumed equivalent to FCM.
4. **`none` is valid.** The user may disable vendor/alternate push. OM then wakes on app open or whatever background execution the OS grants. The UI must explain delayed delivery; queued messages are not lost.
5. **Adapter switching is state-neutral.** Changing FCM -> UnifiedPush -> persistent -> none (or back) rotates/revokes old wake registrations where possible but does not rekey the account or change canonical conversation state. **ADR-0153 clarifies that an unconfirmed provider-side retirement is residual metadata/wake exposure, not a universally time-bounded guarantee unless an enforceable lease is declared.**
6. **Profile isolation still applies.** Shared platform tokens/endpoints are declared linkage points. "Keep separate" profiles must not silently expose the same token to unrelated PushProviders; per-profile handles remain opaque and local mapping remains profile-scoped.
7. Push never becomes an OM-operated mandatory service. Self/community/distribution-operated adapters remain interchangeable where platform credentials permit.

### 3. PersonRecord cleanup/export is V1 local UX, never identity evidence

V1 clients SHOULD provide a local contact-hygiene workflow for PersonRecords with stale/historical/reassigned aliases.

The workflow:

1. shows current versus historical aliases and their provenance/evidence level;
2. lets the user select fields to export/update in an OS address-book contact;
3. presents a preview/diff before any OS-address-book mutation;
4. requires explicit user action and OS permission for each export/update operation;
5. never automatically deletes or overwrites an OS contact solely because OM considers an alias stale;
6. never treats an OS-contact match, edit or exported field as proof that two AccountIDs are the same person;
7. preserves local provenance that a continuity association was cryptographic, social/manual, or merely alias-based;
8. treats exported address-book data as outside OM's cryptographic/security boundary once handed to the OS/contact provider.

No PersonRecord, address-book export or cleanup record becomes a network protocol object. Public/shared synchronization of PersonRecord is not introduced by this ADR.

### 4. Existing V1 promotions are reflected consistently

Current normative prose must no longer describe:

- rich-list presentation as later-wave (the W0 body grammar remains frozen, but `om.rich-list.v1` is V1);
- bounded private multi-device preference sync as merely future;
- desktop native shells as post-V1/later;
- UnifiedPush-compatible Android wake as merely hypothetical future work;
- safe PersonRecord cleanup/export as only a future idea.

Historical checkpoints/reviews retain their original text and are not rewritten.

## Verification obligations for Run 9B

Run 9A intentionally reopens only these repo-local evidence obligations:

1. **V-021 — desktop/reference-family boundary:** prove PWA/Android/iOS/Desktop share the same Core/UI/security semantics while desktop window/filesystem/key-store/notification differences remain capability-reported; include independent-window ProfileExecutionContext and no desktop-only protocol authority.
2. **V-015 — replaceable Android wake/privacy:** exercise FCM, UnifiedPush-compatible, persistent and none transitions; prove push failure/provider change cannot lose canonical messages or grant authority; verify provider/profile linkage follows declared metadata boundaries and old registrations are retired where possible.
3. **V-021 — PersonRecord local export safety:** prove export/cleanup is explicit local UI, preserves provenance, and cannot mutate AccountID/RelationshipBinding/continuity evidence or silently overwrite an OS contact.

Run 9B must complete these repo-local criteria, run the full portable evidence suite, perform a stale-language sweep and reactivate consolidated independent review only if the package remains consistent.

## Explicitly still deferred

This ADR does **not** promote SPQR/Triple Ratchet, PQ MLS, LegacyPolicy activation, live location, stories/status, guaranteed cloud scheduled send, mutual-contact PSI, SMS/MMS/RCS, mini-apps, payments, erasure-coded StoragePools, cryptographic group deniability, massive private-broadcast crypto, additional PQ algorithms or additional expressive-media codecs/profiles into the V1 baseline.
