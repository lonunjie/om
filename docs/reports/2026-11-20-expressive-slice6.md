# Expressive media slice 6 distribution

## Design sources read

- [Constitution digest](../constitution/CONSTITUTION_DIGEST.md) and `python3 tools/constitution.py find` for privacy, operatorless operation and extensibility. C-26 and C-46 govern unresolved wire semantics.
- [Agent architecture rules](../process/AGENT_ARCHITECTURE_RULES.md), [engineering entry point](../process/ENGINEER_START_HERE.md), the [expressive-media build plan](2026-11-20-expressive-media-build-plan.md), [Core evidence](../../verification/EXPRESSIVE_NOTES.md), and the [slice 4](2026-11-20-expressive-slice4.md), [slice 5a](2026-11-20-expressive-slice5a.md) and [slice 5b](2026-11-20-expressive-slice5b.md) reports.
- [PWA reference client](../architecture/client/PWA_REFERENCE_CLIENT.md), [endpoint coercion and platform leakage](../architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md), [security and privacy profiles](../architecture/ux/SECURITY_PRIVACY_PROFILES.md), [profile isolation](../architecture/client/MULTI_IDENTITY_AND_PROFILE_ISOLATION.md) and [storage and blob architecture](../architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md).
- [Expressive media architecture](../architecture/ecosystem/EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md), [rich-text and hostile-content security](../architecture/safety/RICH_TEXT_AND_CONTENT_SECURITY.md), [media asset packs](../ux-design/specs/MEDIA_ASSET_PACKS.md), [logical content-pack schema](../../spec/protocol/content-pack.logical.schema.json), [expressive CDDL](../../spec/cddl/expressive.cddl), and the [W1 expressive slot overlay](../../spec/registries/om-w1-expressive-slots.json).
- ADR-0089 through ADR-0091, ADR-0181 and ADR-0250 through ADR-0253. Also inspected the event attachment schema, object-type registry and applicable passages of the portable archive architecture and UX brief for a reusable transport or link format.

### Design sources outside the full reading

The full gate reading stopped at the slice 6 design question. The constitution originals, including the mapped Privacy, Governance and Authority document, were covered through the digest exception. Full reading was not completed for the following gate sources.

- Client distribution includes PWA Native and Client Distribution Strategy, Client and Extension Architecture, Device Vault Client Host and Migration, ClientHost Update Lifecycle, Notifications and Push, and Plugin and App Extension Architecture.
- One-to-one cryptography includes Cryptography and Agility, Production Crypto Integration Gate, Async Key Material, One-to-One Session Management, Multi-Device Sync, and Canonical Encoding.
- Discovery includes Discovery Fabric, Invites IntroGate and Spam, Contact Sharing, Introduction and Service Accounts, and Key Transparency and Directory.
- Identity includes Identity Protocol, Identity Continuity, Account Continuity and Control, Recovery and Backup Policy, Identity Discovery Verification, Identifier Spoofing and Display, and Account Lifecycle Retirement and Legacy.
- Metadata and delivery include Multi-Relay and Delivery Resilience, Spaces Rooms Communities and Publication, Metadata Threat Model, Relay and Delivery Architecture, and Reachability and Object Repair.
- Events include Event Model, Time and Expiry Semantics, Messenger Baseline V1, and Scoped Permissions and Conversation Organization.
- UX includes Product and UX Principles, UI UX System, UI Core Platform Contract, Accessibility and Input, Motion and Performance, Themes Fonts and Personalization, Reconciled Interaction Model, Action to Core Intent Map, Navigation and Omnibar, Visual Language Color Icons and Assets, and Current UI References 20260929.

The mapped ADR set also remains to be read in full. The decisions listed in the preceding section were the additional slice-specific readings.

## Distribution decision required

Slice 6 is blocked on the interoperable acquisition format. The build plan names all three sources and reuses ordinary OM attachments and blob repair. ADR-0252 freezes the immutable `ContentPackRelease` and the signed `ContentPackControlState` that authorizes it. Its rule 9 bars locations and repair capabilities from release items. Its rule 7 limits the on-message `pack_ref` to attribution, and the `om.expressive.asset.v1` extension describes an individual asset attachment. Neither carries the release and signed lineage needed for installation.

| Source | Decision needed |
| --- | --- |
| Received OM message | What versioned payload identifies a pack offer inside ordinary authenticated attachment carriage? How does it associate the release and signed control-state chain with separately delivered, digest-bound ciphertext and an allowed OM storage or repair route? |
| Portable file | What versioned container carries the canonical release, control-state chain and ciphertext? The manifest limits bound metadata and individual items. The container also needs aggregate byte and entry limits and a rule for missing, duplicate and unreferenced blobs. |
| Deep link or QR | What validated payload identifies the pack, binds it to the intended release, and resolves through an allowed OM route without contacting an arbitrary web origin? The current first-contact link exchanges contact-introduction material. The general profile resolver chooses an identity and returns a target string. |

Record these choices and their validation rules in an accepted distribution decision before implementing product parsers or adding pack semantics to ordinary messages. The [build plan stop conditions](2026-11-20-expressive-media-build-plan.md#8-stop-conditions-for-every-slice) and ADR-0250 require the design question to be resolved first. The reviewed flow still requires explicit confirmation of the exact pack and a single atomic activation by Core. Maximum Privacy must refuse the link and QR paths before acquisition I/O. A pack controller's authority stays scoped to release continuity.

## Related integration gaps

- **Cache and picker.** Slice 3 records cache membership and validates supplied bytes. The runtime commits the pack inventory, while the real web host currently supplies only the conversation asset description. The picker uses its optional `readCached` hook for send preparation and renders text labels. It still needs profile-local ciphertext storage and a Core-validated preview path. The one-shot Wasm install command has a 16 MiB JSON limit and hex-encodes the inputs, so it cannot carry every valid full pack in one call. Any staged storage integration has to preserve the prior active install on a failed import or write.
- **Inline emoji.** The design permits semantic placement from authenticated `message.create.content` and `message.edit.replacement`. The existing reducer stores flattened text and an ordered asset list. That representation cannot recover positions for older messages or distinguish repeated emoji fallback text from identical ordinary text. The timeline should keep the existing fallback where the authenticated placement is unavailable. Before changing the projection, confirm how it will retain the accepted placement with the winning revision and how the UI will distinguish a caption from generated fallback text.
- **Expressive module.** The web loader currently selects base or groups. The separate expressive artifact exists as a build target, and the app checks the runtime capability before exposing the UI. Integration and measurement of the separate artifact remain open.

Distribution, cache and timeline product code were not changed. The capability remains default-off. There is no main Wasm size delta from this change.

## Verification

| Check | Result |
| --- | --- |
| `pnpm exec tsc -b apps/web` | Passed. |
| `pnpm exec eslint apps/web packages/om-ui packages/om-core-wasm packages/om-platform-api packages/om-web-platform packages/om-core-harness` | Passed. |
| `pnpm --filter @om/web exec vitest run --dir src --maxWorkers 2` | 29 files and 207 tests passed. This is baseline coverage. |
| `python3 tools/web_guard.py` and `python3 tools/core_guard.py` | Both reported zero errors. |
| `python3 tools/om_docs.py fmt` | Zero rewrites and zero unresolved links. |
| `make check` and `git diff --check` | Passed. |

The new Rust scenarios, malformed-input corpus, request-log and Maximum Privacy coverage, and two-identity Playwright spec await the acquisition contract. The existing Playwright case uses a locally seeded pack and does not exercise delivery between identities. No browser or Cargo/Wasm build was run. The report and its file registry entry are uncommitted.
