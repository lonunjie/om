# Run 10A — residual V1 boundary audit

**Date:** 2026-09-28 22:00 PHT  
**Scope:** bounded design audit only; no product engineering, no environment-dependent carrier integration work, no independent-review self-approval  
**Input checkpoint:** Run 9B / ADR-0153 ([`CHECKPOINT_20260928_2122.md`](../checkpoints/CHECKPOINT_20260928_2122.md))

## Purpose

Runs 6A through 9B intentionally pulled several low-risk foundations from the later backlog into V1 when their authority, security and migration boundaries were already mature. This pass asks the inverse question one final time:

> Is any residual post-V1 feature still deferred mainly because of sequencing, even though the current V1 abstractions already make it safe and useful to commit now?

The audit is deliberately narrow. It does **not** reopen the high-risk cryptographic or service-authority work already kept outside V1 (SPQR/Triple Ratchet, PQ MLS, group deniability, payments, live location, guaranteed provider-side scheduled send, massive private-broadcast crypto, erasure coding, extra PQ algorithms, nation-state anonymity/attestation systems). It focuses on adapter-local, endpoint-local, recognition and presentation surfaces where a V1 promotion might plausibly avoid later architecture surgery.

## Promotion test

A residual feature qualifies for V1 only if all of the following hold:

1. **No new authority:** it does not create a new identity, account, relationship, provider, social-graph or service authority.
2. **No new cryptographic construction:** it reuses reviewed V1 security profiles rather than adding new primitives/protocols.
3. **No silent security downgrade:** weaker external transports remain explicit and cannot masquerade as OM-native E2EE.
4. **No mandatory external gatekeeper:** ordinary OM V1 remains usable if a carrier, app store, platform vendor, cloud provider or proprietary API refuses access.
5. **No second canonical message model:** the feature either stays local/presentation-only or fits the existing event/provenance/adapter contracts.
6. **No architectural-rewrite pressure:** deferring the implementation would otherwise force V1 interfaces or persistent semantics to be redesigned later.
7. **Evidence is proportionate:** committing the feature now would not create a large new verification/security burden disproportionate to its V1 value.

A feature that fails criterion 6 is particularly important to keep deferred: if V1 already contains the stable extension socket, there is little architectural value in pulling the concrete implementation forward.

## Residual candidates

| Candidate | Existing V1 socket | Result | Reason |
|---|---|---|---|
| Android SMS/MMS integration | `TransportAdapter`, transport provenance, weaker-security UI, local imported-transport model | **Keep post-V1** | Useful but Android/store-role dependent; full inbox/send integration requires default-SMS handling and sensitive permissions. The adapter socket already prevents later core redesign. |
| Carrier RCS integration | `TransportAdapter`, capability model, provenance/fidelity boundary | **Keep post-V1** | Android 37 adds an alternative-message-transport hook for the default SMS app, but this does not make carrier RCS a portable OM dependency or generic cross-platform send API. Carrier/platform capability remains variable. |
| “Send via system SMS app” handoff | ordinary OS intent/share capability | **Implementation detail, not V1 architecture** | A user-initiated OS handoff can be added without importing carrier messages into OM or changing canonical state. No architecture promotion is necessary. |
| Private mutual-contact PSI/counts | recognition-evidence model already separates hints from authentication | **Keep post-V1/research** | Exact private mutual discovery still creates social-graph privacy and protocol/research questions. V1 may omit it; direct/manual recognition evidence already fits the model. |
| LegacyPolicy activation | existing dormant critical ACS extension | **Keep disabled/later** | Activation changes recovery/authority behavior, so it is not an endpoint-local convenience feature. |
| Additional expressive-media codecs/profiles | W1 expressive-media/content-pack extension points | **Keep later** | New parsers/codecs add hostile-content and compatibility surface. V1 already has enough extension structure to add reviewed profiles later without redesign. |
| Exact privacy-preset defaults | typed policy/preset concepts | **Measure before freeze** | This is a product-research/measurement decision, not a missing V1 architecture feature. Exact defaults should follow representative usability/privacy testing. |
| Exact UI framework/breakpoints/theme/font/motion tokens | shared `om-ui`, design tokens, platform adapters | **Measure before freeze** | These are deliberately deferred implementation/product choices. Freezing them now would reduce evidence-driven flexibility without strengthening protocol compatibility. |
| IndexedDB vs OPFS-backed SQLite crossover | `StorageAdapter` | **Measure before freeze** | The abstraction is already frozen; the crossover belongs to performance evidence, not V1 protocol scope. |

## Carrier-messaging research checkpoint

The carrier candidate was checked against current official Android/Google documentation on 2026-09-28 because these platform rules are volatile.

### SMS/MMS

- Android's Telephony provider states that only the user-selected **default SMS app** can write the SMS provider and receive the primary SMS/MMS delivery broadcasts.
- Google Play treats SMS permissions as highly sensitive and generally requires the app to be the active default SMS (or other specifically eligible default handler/approved exception) before requesting them; the feature must be documented as core functionality and is subject to Play declaration/review.

Implication for OM: making unified SMS/MMS a V1 reference-client commitment would add Android distribution/policy/default-handler obligations that are unrelated to OM's secure native messaging correctness. Keeping it post-V1 does **not** create architecture debt because the current `TransportAdapter` + provenance/security-class model already reserves the integration boundary.

### RCS

- Android exposes `ImsRcsManager`/UCE for RCS registration/capability exchange, with several operations requiring privileged telephony access or carrier privileges.
- Android API level 37 adds `AlternativeMessageTransportService`, allowing the **default SMS app** to accept an SMS/MMS send request and send it over an alternative transport such as RCS. The app is then responsible for that transport and send result.
- The public Google RCS Business Messaging API is a business-agent service surface, not a general replacement for consumer carrier messaging inside an independent messenger.

Implication for OM: the ecosystem is moving in a direction that may make future carrier integration cleaner, which validates preserving `TransportAdapter`; it does not justify making carrier RCS a V1 dependency or promise. OM should revisit RCS when a concrete authorized transport path can be implemented and tested without changing the core event/identity model.

Official sources checked:

- Android Telephony provider: https://developer.android.com/reference/android/provider/Telephony
- Android default-handler requirements: https://developer.android.com/guide/topics/permissions/default-handlers
- Google Play SMS/Call Log policy: https://support.google.com/googleplay/android-developer/answer/10208820
- Android `AlternativeMessageTransportService`: https://developer.android.com/reference/android/service/messaging/AlternativeMessageTransportService
- Android `ImsRcsManager`: https://developer.android.com/reference/android/telephony/ims/ImsRcsManager
- Google RCS Business Messaging API: https://developers.google.com/business-communications/rcs-business-messaging/reference/rest

## Why there is no Run-10A promotion ADR

No candidate passes the promotion test strongly enough to justify changing V1 semantics.

The most plausible item, carrier messaging, already has exactly the architecture treatment desired for a safely deferred feature:

```text
foreign/carrier transport
        ↓
TransportAdapter + capabilities
        ↓
explicit provenance/security class
        ↓
local materialized conversation view
```

The concrete adapter may arrive later without changing AccountID, RelationshipBinding, EventID, OM-native E2EE, relay/provider semantics or the V1 message grammar. Promoting it now would therefore add platform/store obligations while buying almost no architectural protection.

The remaining local/product items likewise either already have an adequate abstraction or intentionally depend on measurement/research. Creating a new ADR merely to restate existing deferrals would add decision churn without changing the design.

## Boundary conclusion

**Run 10A promotes zero additional post-V1 features.**

This is a positive closure result rather than a lack of findings. After Runs 6A–9B, the residual boundary now cleanly separates:

- features whose stable foundations are already in V1; from
- features that genuinely introduce new crypto, authority, privacy, parser, platform-policy, carrier/API or measurement obligations.

The V1 feature boundary should therefore be treated as **intentionally complete for external architecture review**. Future promotions should require a new concrete fact—such as a reviewed standard, a stable public platform capability, measurement evidence, or an independent-review finding—not another speculative sweep for more scope.

## Verification / handoff consequence

- No wire/schema/registry/authority/security-profile change is made.
- No machine requirement, decision, threat or verification-gate criterion is added or reopened.
- Existing Run-9B repo-local evidence remains current.
- The consolidated different-vendor `core_kernel` review handoff remains active.
- Engineering remains unauthorized.
