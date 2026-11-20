# Expressive media slice 6 distribution

## Design sources read

- [Constitution digest](../constitution/CONSTITUTION_DIGEST.md) and `python3 tools/constitution.py find` for privacy, operatorless operation and extensibility. C-26 and C-46 govern unresolved wire semantics.
- [Agent architecture rules](../process/AGENT_ARCHITECTURE_RULES.md), [engineering entry point](../process/ENGINEER_START_HERE.md), the [expressive-media build plan](2026-11-20-expressive-media-build-plan.md), [Core evidence](../../verification/EXPRESSIVE_NOTES.md), and the [slice 4](2026-11-20-expressive-slice4.md), [slice 5a](2026-11-20-expressive-slice5a.md) and [slice 5b](2026-11-20-expressive-slice5b.md) reports.
- [PWA reference client](../architecture/client/PWA_REFERENCE_CLIENT.md), [endpoint coercion and platform leakage](../architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md), [security and privacy profiles](../architecture/ux/SECURITY_PRIVACY_PROFILES.md), [profile isolation](../architecture/client/MULTI_IDENTITY_AND_PROFILE_ISOLATION.md) and [storage and blob architecture](../architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md).
- [Expressive media architecture](../architecture/ecosystem/EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md), [rich-text and hostile-content security](../architecture/safety/RICH_TEXT_AND_CONTENT_SECURITY.md), [media asset packs](../ux-design/specs/MEDIA_ASSET_PACKS.md), [logical content-pack schema](../../spec/protocol/content-pack.logical.schema.json), [expressive CDDL](../../spec/cddl/expressive.cddl), and the [W1 expressive slot overlay](../../spec/registries/om-w1-expressive-slots.json).
- ADR-0089 through ADR-0091, ADR-0181 and ADR-0250 through ADR-0253. Also inspected the event attachment schema, object-type registry and applicable passages of the portable archive architecture and UX brief for a reusable transport or link format.

### Design sources outside the distribution checkpoint reading

At that checkpoint, the full gate reading stopped at the slice 6 design question. The constitution originals, including the mapped Privacy, Governance and Authority document, were covered through the digest exception. Full reading was not completed for the following gate sources.

- Client distribution includes PWA Native and Client Distribution Strategy, Client and Extension Architecture, Device Vault Client Host and Migration, ClientHost Update Lifecycle, Notifications and Push, and Plugin and App Extension Architecture.
- One-to-one cryptography includes Cryptography and Agility, Production Crypto Integration Gate, Async Key Material, One-to-One Session Management, Multi-Device Sync, and Canonical Encoding.
- Discovery includes Discovery Fabric, Invites IntroGate and Spam, Contact Sharing, Introduction and Service Accounts, and Key Transparency and Directory.
- Identity includes Identity Protocol, Identity Continuity, Account Continuity and Control, Recovery and Backup Policy, Identity Discovery Verification, Identifier Spoofing and Display, and Account Lifecycle Retirement and Legacy.
- Metadata and delivery include Multi-Relay and Delivery Resilience, Spaces Rooms Communities and Publication, Metadata Threat Model, Relay and Delivery Architecture, and Reachability and Object Repair.
- Events include Event Model, Time and Expiry Semantics, Messenger Baseline V1, and Scoped Permissions and Conversation Organization.
- UX includes Product and UX Principles, UI UX System, UI Core Platform Contract, Accessibility and Input, Motion and Performance, Themes Fonts and Personalization, Reconciled Interaction Model, Action to Core Intent Map, Navigation and Omnibar, Visual Language Color Icons and Assets, and Current UI References 20260929.

The mapped ADR set also remains to be read in full. The decisions listed in the preceding section were the additional slice-specific readings.

## Distribution decision resolved by ADR-0254

[ADR-0254](../../decisions/ADR-0254-expressive-media-pack-distribution.md) resolves the distribution decision. It defines one canonical pack file, the validation and size limits, and the sources that may enter the existing named confirmation flow.

| Source | Ruling |
| --- | --- |
| Received OM message | An ordinary attachment with media type `application/vnd.om.content-pack` opens **Review pack**. The sender is identified as the person who sent the file. |
| Portable file | A picked or dropped `.ompack` opens the same review using local bytes. |
| Deep link or QR | Deferred in W1 under ruling 5. |

The confirmed pack installs through staged writes and atomic Core activation. The profile's blob store holds item ciphertext by digest. Pack controller authority stays scoped to release continuity.

## Related integration gaps

- **Cache and picker.** At the distribution checkpoint, Slice 3 recorded cache membership and validated supplied bytes. The web host supplied conversation asset descriptions. The next slice needed profile-local ciphertext storage, a Core-validated preview path and staged writes that fit the 16 MiB Wasm command envelope.
- **Inline emoji.** The design permits semantic placement from authenticated `message.create.content` and `message.edit.replacement`. The existing reducer stores flattened text and an ordered asset list. That representation cannot recover positions for older messages or distinguish repeated emoji fallback text from identical ordinary text. ADR-0254 ruling 7 keeps the existing projection and uses fallback text where authenticated placement is unavailable.
- **Expressive module.** At the distribution checkpoint, the web loader selected base or groups. The separate expressive artifact existed as a build target. Its measured size remains open.

At the distribution decision checkpoint, the capability was default-off and the main Wasm size was unchanged.

## Distribution decision checkpoint verification

| Check | Result |
| --- | --- |
| `pnpm exec tsc -b apps/web` | Passed. |
| `pnpm exec eslint apps/web packages/om-ui packages/om-core-wasm packages/om-platform-api packages/om-web-platform packages/om-core-harness` | Passed. |
| `pnpm --filter @om/web exec vitest run --dir src --maxWorkers 2` | 29 files and 207 tests passed. This is baseline coverage. |
| `python3 tools/web_guard.py` and `python3 tools/core_guard.py` | Both reported zero errors. |
| `python3 tools/om_docs.py fmt` | Zero rewrites and zero unresolved links. |
| `make check` and `git diff --check` | Passed. |

The existing Playwright case uses a locally seeded pack. The distribution decision checkpoint recorded baseline checks only.

## Implementation

**Design sources read.** This pass read ADR-0250 through ADR-0254, the [constitution digest](../constitution/CONSTITUTION_DIGEST.md), the [agent architecture rules](../process/AGENT_ARCHITECTURE_RULES.md), the [engineering entry point](../process/ENGINEER_START_HERE.md), the [canonical encoding rules](../architecture/crypto/CANONICAL_ENCODING.md), the [PWA reference client](../architecture/client/PWA_REFERENCE_CLIENT.md), the [endpoint leakage rules](../architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md), [profile isolation](../architecture/client/MULTI_IDENTITY_AND_PROFILE_ISOLATION.md), [privacy profiles](../architecture/ux/SECURITY_PRIVACY_PROFILES.md), the [expressive-media architecture](../architecture/ecosystem/EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md), the [storage architecture](../architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md), the [hostile-content boundary](../architecture/safety/RICH_TEXT_AND_CONTENT_SECURITY.md), the [media asset-pack UX](../ux-design/specs/MEDIA_ASSET_PACKS.md), [expressive CDDL](../../spec/cddl/expressive.cddl), the [Core evidence](../../verification/EXPRESSIVE_NOTES.md) and the slice handoffs. It checked the R-108 requirement and the existing Slice 3 verifier and activation scenario.

The product-path gate returned `client_endpoint_distribution`, `crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability`, `time_ordering_events` and `ux_accessibility_personalization`. The mapped documents and ADRs for those domains have full readings recorded in the [slice 4](2026-11-20-expressive-slice4.md), [slice 5a](2026-11-20-expressive-slice5a.md) and [slice 5b](2026-11-20-expressive-slice5b.md) reports. This pass used those readings for the unchanged mapped sources. ADR-0254 supplies the distribution and lineage decisions added since those passes.

**Design sources outside this pass's full reading.** The constitution originals were covered through the digest exception. The remaining mapped sources listed in the checkpoint's design sources were read in the slice handoffs. The full pre-release V-012 evidence and the browser capability matrix were outside the current verification scope.

**Lineage question resolved.** ADR-0254 ruling 2 supplies the missing manifests in key 5. A fresh profile replays the complete predecessor chain through the Slice 3 verifier. A profile with the predecessor pin can verify an ordinary update from that pin. Every supplied predecessor must link in order and match a verified control-state commitment. A file missing the required lineage is refused before staging, with “This pack needs an earlier version” when an earlier version is required.

- Core decodes the local canonical container with the existing OM-CBOR codec, checks the two chain counts and aggregate size ceilings, and matches each ciphertext digest to the target manifest. It opens the named ciphertext and validates WebP header facts through the existing budget walker before creating a review. The digest entry uses the OM SHA-256 digest structure.
- The expressive bridge receives bounded file chunks. Confirmation binds the reviewed release and controller head to a profile-scoped ticket. Begin and stage issue one validated cache write at a time. Core activates the inventory through its existing single-record transaction after all cache writes succeed.
- The profile vault stores ciphertext by digest. The web host serializes pack mutations and cleanup per profile across tabs and checks the durable inventory before touching the cache. Interrupted or obsolete cache entries are reclaimed against the active inventory. Picker previews use the Slice 4 driver after Core validation and publish frames after Core accepts decoder observations.
- **Review pack** opens a received pack as an ordinary attachment on request. The review names the sender as the person who sent the file. Choosing or dropping a local `.ompack` enters the same named confirmation. The message path uses ordinary attachment delivery, and the file path uses local bytes. Provenance and license addresses display as inert text. Pack controller authority stays scoped to release continuity.
- The engineering flag defaults off. An opt-in web build can load the digest-checked expressive artifact. Devices already using the groups artifact keep the groups loader preference. Rulings 5 and 7 retain file-only acquisition and the existing timeline projection.

## Verification

Run on the build host unless noted.

| Check | Result |
| --- | --- |
| `cargo test --workspace --offline --no-fail-fast` | Every test result line ok, no failures. Includes the container hostile corpus (each listed rejection, exactly-at and one-over limits for 64 KiB overhead, 32 MiB and the chain counts), the golden-vector re-decode test, and the staged-install kill-point test that checks the prior install survives a failed write or commit. |
| Wasm builds (base, groups, expressive) | Built. |
| `pnpm repro` in `apps/web` | Reproducible: two clean builds gave identical manifests. |
| `pnpm check:size-budget` | Base 7455647 raw / 2092642 gzip, groups 9332383 raw / 2700860 gzip, delta +1876736 raw / +608218 gzip (limit +2169546 / +652950). Within budget; the default build is unchanged because the expressive artifact is opt-in. |
| Vitest `--dir src` | om-ui 458, om-core-wasm 94, om-core-harness 38, om-platform-api 9, om-web-platform 127, apps/web 213 tests, all passed. |
| `pnpm exec tsc -b apps/web`, ESLint on touched packages | Clean. |
| `expressive-pack.spec.ts` (chromium, opt-in setup bundle, `OM_EXPRESSIVE_E2E=1`) | Passed on three consecutive runs. Alice sends a pack file as an attachment, Bob reviews it ("File sent by Alice"), installs it, sends a sticker, then updates from a local `.ompack`. The request log shows no third-party request for the message install or the file update. |
| `python3 tools/om_docs.py fmt`, `make check` (local) | Clean. |

Not verified: Firefox and WebKit runs of the new spec, the native decode matrix and a visual layout pass.

## Open questions for the operator

- Link, QR and deep-link acquisition stay unbuilt by ADR-0254 ruling 5; a later ADR would need its own resolver design.
- Inline emoji placement stays on the existing projection by ruling 7.
