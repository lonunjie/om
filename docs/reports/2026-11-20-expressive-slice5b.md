# Expressive media slice 5b UI

## Design sources read

The non-constitution documents and ADRs returned by `python3 tools/design_gate.py needs --brief packages/om-ui apps/web/src packages/om-design` were read in full.

- `client_endpoint_distribution` includes [`PWA_NATIVE_AND_CLIENT_DISTRIBUTION_STRATEGY.md`](../architecture/client/PWA_NATIVE_AND_CLIENT_DISTRIBUTION_STRATEGY.md), [`PWA_REFERENCE_CLIENT.md`](../architecture/client/PWA_REFERENCE_CLIENT.md), [`CLIENT_AND_EXTENSION_ARCHITECTURE.md`](../architecture/client/CLIENT_AND_EXTENSION_ARCHITECTURE.md), [`DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`](../architecture/client/DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md), [`MULTI_IDENTITY_AND_PROFILE_ISOLATION.md`](../architecture/client/MULTI_IDENTITY_AND_PROFILE_ISOLATION.md), [`CLIENTHOST_UPDATE_LIFECYCLE.md`](../architecture/client/CLIENTHOST_UPDATE_LIFECYCLE.md), [`NOTIFICATIONS_AND_PUSH.md`](../architecture/client/NOTIFICATIONS_AND_PUSH.md), [`PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md`](../architecture/ecosystem/PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md), [`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](../architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md).
- `discovery_first_contact` includes [`DISCOVERY_FABRIC.md`](../architecture/discovery/DISCOVERY_FABRIC.md), [`INVITES_INTROGATE_AND_SPAM.md`](../architecture/discovery/INVITES_INTROGATE_AND_SPAM.md), [`CONTACT_SHARING.md`](../architecture/discovery/CONTACT_SHARING.md), [`INTRODUCTION_AND_SERVICE_ACCOUNTS.md`](../architecture/discovery/INTRODUCTION_AND_SERVICE_ACCOUNTS.md), [`KEY_TRANSPARENCY_AND_DIRECTORY.md`](../architecture/discovery/KEY_TRANSPARENCY_AND_DIRECTORY.md).
- `relay_reachability` includes [`RELAY_AND_DELIVERY_ARCHITECTURE.md`](../architecture/delivery/RELAY_AND_DELIVERY_ARCHITECTURE.md), [`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](../architecture/delivery/MULTI_RELAY_AND_DELIVERY_RESILIENCE.md), [`REACHABILITY_AND_OBJECT_REPAIR.md`](../architecture/delivery/REACHABILITY_AND_OBJECT_REPAIR.md).
- `ux_accessibility_personalization` includes [`PRODUCT_AND_UX_PRINCIPLES.md`](../overview/PRODUCT_AND_UX_PRINCIPLES.md), [`UI_UX_SYSTEM.md`](../architecture/ux/UI_UX_SYSTEM.md), [`UI_CORE_PLATFORM_CONTRACT.md`](../architecture/ux/UI_CORE_PLATFORM_CONTRACT.md), [OM Accessibility, Input and Inclusive Interaction](../architecture/ux/ACCESSIBILITY_AND_INPUT.md), [`MOTION_AND_PERFORMANCE.md`](../architecture/ux/MOTION_AND_PERFORMANCE.md), [`THEMES_FONTS_AND_PERSONALIZATION.md`](../architecture/ux/THEMES_FONTS_AND_PERSONALIZATION.md), [`SECURITY_PRIVACY_PROFILES.md`](../architecture/ux/SECURITY_PRIVACY_PROFILES.md), [`RECONCILED_INTERACTION_MODEL.md`](../architecture/ux/RECONCILED_INTERACTION_MODEL.md), [`ACTION_TO_CORE_INTENT_MAP.md`](../architecture/ux/ACTION_TO_CORE_INTENT_MAP.md), [`NAVIGATION_AND_OMNIBAR.md`](../architecture/ux/NAVIGATION_AND_OMNIBAR.md), [`VISUAL_LANGUAGE_COLOR_ICONS_AND_ASSETS.md`](../architecture/ux/VISUAL_LANGUAGE_COLOR_ICONS_AND_ASSETS.md), [`CURRENT_UI_REFERENCES_20260929.md`](../research/CURRENT_UI_REFERENCES_20260929.md).

Mapped ADRs were ADR-0004, ADR-0007, ADR-0012, ADR-0019, ADR-0020, ADR-0021, ADR-0022, ADR-0025, ADR-0033, ADR-0037, ADR-0038, ADR-0040, ADR-0044, ADR-0045, ADR-0051, ADR-0052, ADR-0053, ADR-0054, ADR-0055, ADR-0056, ADR-0057, ADR-0058, ADR-0059, ADR-0060, ADR-0068, ADR-0075, ADR-0077, ADR-0103, ADR-0115, ADR-0126, ADR-0129, ADR-0130, ADR-0159, ADR-0162. Additional decisions read in full were ADR-0089 through ADR-0091, ADR-0166, ADR-0178, ADR-0187, and ADR-0250 through ADR-0253.

The slice-specific reading covered [`AGENT_UX_BRIEF.md`](../ux-design/AGENT_UX_BRIEF.md), [`ATTACHMENTS_AND_MEDIA_UX.md`](../ux-design/specs/ATTACHMENTS_AND_MEDIA_UX.md), [`SEMANTIC_MOTION_AND_DELIGHT.md`](../ux-design/specs/SEMANTIC_MOTION_AND_DELIGHT.md), [`COMPONENT_BEHAVIOR_AND_STATES.md`](../ux-design/specs/COMPONENT_BEHAVIOR_AND_STATES.md), [Accessibility and Input Design](../ux-design/specs/ACCESSIBILITY_AND_INPUT.md), [`ACTION_REGISTRY.md`](../ux-design/specs/ACTION_REGISTRY.md), [`CONTENT_AND_MICROCOPY_VOICE.md`](../ux-design/specs/CONTENT_AND_MICROCOPY_VOICE.md), [`MEDIA_ASSET_PACKS.md`](../ux-design/specs/MEDIA_ASSET_PACKS.md) and [`HAPTIC_AND_SOUND_SEMANTICS.md`](../ux-design/specs/HAPTIC_AND_SOUND_SEMANTICS.md).

Also read [`EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md`](../architecture/ecosystem/EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md), [`AGENT_ARCHITECTURE_RULES.md`](../process/AGENT_ARCHITECTURE_RULES.md), [`ENGINEER_START_HERE.md`](../process/ENGINEER_START_HERE.md), [`ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md`](../process/ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md), [`HARDENING_REVIEW_20260924.md`](../reviews/HARDENING_REVIEW_20260924.md), the expressive entries in the machine contracts, R-108, the [`2026-11-20-expressive-media-build-plan.md`](2026-11-20-expressive-media-build-plan.md), [`EXPRESSIVE_NOTES.md`](../../verification/EXPRESSIVE_NOTES.md), [`2026-11-20-expressive-slice4.md`](2026-11-20-expressive-slice4.md) and [`2026-11-20-expressive-slice5a.md`](2026-11-20-expressive-slice5a.md).

The constitution was read through [`CONSTITUTION_DIGEST.md`](../constitution/CONSTITUTION_DIGEST.md) and `tools/constitution.py find` for provenance, security, creator disappearance and metadata.

### Design sources outside this reading

The constitution originals were covered through the digest exception. [`PLATFORM_CAPABILITY_MATRIX.md`](../architecture/client/PLATFORM_CAPABILITY_MATRIX.md), [`METADATA_TRACE_TEST_PLAN.md`](../process/METADATA_TRACE_TEST_PLAN.md) and the full V-012 verification criteria were outside this UI reading. The slice handoffs supplied the relevant Core and decoder contracts.

## Implementation

- The shared picker is reachable from the composer and the existing reaction control in an active one-to-one conversation. Recents follow committed use. Favourites and installed packs use the captured profile's Core state. Settings shows their per-device scope when the capability is enabled.
- Pack review shows the exact title, release sequence, controller threshold/count, item count, declared size and licensing. Provenance shows **Signed by** with the controller key evidence held by Core. Install and update require a separate named confirmation bound to the reviewed release and controller state. Review submits the manifest with any available local bytes under Core’s Lazy cache policy. Removal is local and has its own named confirmation.
- Recognized expressive attachments use a larger media presentation. The message retains existing text, timestamps, delivery state and reply context. Reactions retain the ordinary reaction control and count. Validated rich-text nodes can render custom emoji in place with their text fallback.
- The app uses the slice 5a client with the slice 4 platform decoder. Opened frames are painted on an accessible canvas with Core's alternative text. Reduced Motion starts on a representative frame and playback starts on explicit input. Descriptions stay in the DOM as text on failure. The web host resolves the existing Core plan for metadata locally and opens remote conversation assets only on a user action. Pack content enters the UI as text and bounded pixels. Rendering has no asset URL attributes and pack fields have no style or overlay inputs.
- The web build uses the default-off `VITE_OM_EXPRESSIVE_MEDIA` flag and requires a ready Core capability. The engineering harness opts in with `?expressive`. The UI uses the existing signed module loader. V-012 release obligations remain open.

Core retains the authority to prepare a send, activate a pack and accept a decode. The UI issues typed actions and renders the resulting state.

## Integration questions

- What Core timeline projection should carry authenticated inline emoji positions and distinguish attachment fallback text from a caption? Slice 5a provides asset lookup by index and flattened message text. The UI preserves the text and only substitutes inline nodes when a semantic document supplies them. The harness provides that document for its inline fixture. The real timeline continues to use its text fallback for inline emoji.
- What profile-local cache reader will Slice 6 supply for installed pack items and their previews? The picker currently uses accessible text labels and shows missing items as unavailable. The loopback fixture supplies local bytes. The real web host still needs pack acquisition, cache wiring and a build that packages the expressive-capable Core with the existing verification path.
- Should a canonical custom reaction token be assigned later? ADR-0252 leaves it free text. The picker uses the pack's explicit fallback when it fits the existing reaction key rules. Items without one remain outside the custom reaction choices.

## Verification

The changes are uncommitted. The keyboard-only [Playwright spec](../../apps/web/e2e/expressive.spec.ts) exercises a sticker send, custom reaction and pack removal on the loopback harness. Browser execution, Playwright execution, Cargo and Wasm builds were outside this pass. The browser focus, visual layout and native decode matrix remain unverified.

| Check | Result |
| --- | --- |
| `pnpm --filter @om/web exec vitest run --dir src` | Passed. Includes copy lint and jsdom/axe coverage for the expressive picker, confirmation, failure, fallback and Reduced Motion states. The hostile-content case checks the safety-number notice and external link DOM and rejects asset URLs and styles. |
| `pnpm --filter @om/ui exec vitest run --dir src --maxWorkers 2` | Passed. A concurrent run exceeded the existing conversation-list test timeout and left its axe check running. |
| `pnpm exec tsc -b apps/web` | Passed. |
| Standalone TypeScript check of the Playwright spec | Passed. |
| `pnpm exec eslint apps/web packages/om-ui packages/om-design` | Passed. |
| `python3 tools/web_guard.py` | Passed. |
| `python3 tools/om_docs.py fmt` and `make check` | Passed. |
| `git diff --check` | Passed. |
