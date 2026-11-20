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

## Distribution decision resolved by ADR-0254

[ADR-0254](../../decisions/ADR-0254-expressive-media-pack-distribution.md) resolves the distribution decision. It defines one canonical pack file, the validation and size limits, and the sources that may enter the existing named confirmation flow.

| Source | Ruling |
| --- | --- |
| Received OM message | An ordinary attachment with media type `application/vnd.om.content-pack` opens **Review pack**. The sender is identified as the person who sent the file. |
| Portable file | A picked or dropped `.ompack` opens the same review using local bytes. |
| Deep link or QR | Deferred in W1 under ruling 5. |

The confirmed pack installs through staged writes and atomic Core activation. The profile's blob store holds item ciphertext by digest. Pack controller authority stays scoped to release continuity.

## Related integration gaps

- **Cache and picker.** Slice 3 records cache membership and validates supplied bytes. The runtime commits the pack inventory, while the real web host currently supplies only the conversation asset description. The picker uses its optional `readCached` hook for send preparation and renders text labels. It still needs profile-local ciphertext storage and a Core-validated preview path. The one-shot Wasm install command has a 16 MiB JSON limit and hex-encodes the inputs, so it cannot carry every valid full pack in one call. Any staged storage integration has to preserve the prior active install on a failed import or write.
- **Inline emoji.** The design permits semantic placement from authenticated `message.create.content` and `message.edit.replacement`. The existing reducer stores flattened text and an ordered asset list. That representation cannot recover positions for older messages or distinguish repeated emoji fallback text from identical ordinary text. ADR-0254 ruling 7 keeps the existing projection and uses fallback text where authenticated placement is unavailable.
- **Expressive module.** The web loader currently selects base or groups. The separate expressive artifact exists as a build target, and the app checks the runtime capability before exposing the UI. Integration and measurement of the separate artifact remain open.

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

**Design sources read.** The slice 6b review read ADR-0251 through ADR-0254, ADR-0090, the constitution digest and C-26, the architecture rules and engineering entry point, the expressive-media build plan and slice 4, 5a and 5b handoffs, and the [Core evidence](../../verification/EXPRESSIVE_NOTES.md). It compared the [canonical encoding rules](../architecture/crypto/CANONICAL_ENCODING.md), [expressive CDDL](../../spec/cddl/expressive.cddl), [common CDDL](../../spec/cddl/common.cddl), and [logical content-pack schema](../../spec/protocol/content-pack.logical.schema.json) with the [slice-3 verifier](../../crates/om-core/src/expressive/pack.rs) and its [atomic update scenario](../../crates/om-core/tests/attachments.rs). It inspected the lineage and fork sections of the expressive-media architecture and the digest and BlobID definitions in the storage architecture.

**Design sources outside the full reading.** The constitution originals were covered through the digest exception. The product-path gate was enumerated. Full reading of the remaining mapped documents and ADRs stopped at the verification question below. The gate has no mapped code path for this report.

**Open verification rule for portable updates.** ADR-0254 ruling 1 gives a file exactly one target release manifest and an ordered control-state chain. Ruling 2 reuses slice-3 verification. The current verifier requires release sequence 1 on a fresh profile. On a profile with a pin, it checks an update's sequence and `previous_release_id` against the held predecessor. An intermediate control state commits to its release's digest and sequence. Checking that release's `previous_release_id` and any genesis `forked_from` requires the corresponding manifest bytes.

Consider a release-2 file with a valid signed genesis state and a valid signed successor state. A fresh profile has only the release-2 manifest. Should it require the predecessor to be installed first, or can it establish continuity from the signed states and target manifest alone? A third option is to amend the file format to carry the predecessor manifests. The same rule determines whether a file can advance a held pin across intermediate releases. This acceptance rule determines the golden vectors and install tests.

The slice is at the [build plan stop condition](2026-11-20-expressive-media-build-plan.md#8-stop-conditions-for-every-slice). This pass updates the report only.

## Verification

The commands below verify the baseline. The portable parser corpus, staged-install kill points, new accessibility states, request log, and two-identity scenario remain pending the acceptance rule above.

| Check | Result |
| --- | --- |
| `cargo check --offline -p om-wasm --features expressive-media --tests` | Passed. |
| `pnpm exec tsc -b apps/web` | Passed. |
| `pnpm exec eslint apps/web packages/om-ui packages/om-core-wasm packages/om-platform-api packages/om-web-platform packages/om-core-harness` | Passed. |
| `pnpm --filter @om/web exec vitest run --dir src --maxWorkers 2` | 29 files and 207 tests passed, including existing jsdom/axe and copy checks. |
| `python3 tools/web_guard.py` and `python3 tools/core_guard.py` | Both reported zero errors. |
| `python3 tools/om_docs.py fmt` | Zero rewrites and zero unresolved links. |
| `make check` and `git diff --check` | Passed. |

The report update is uncommitted. This pass ran no browsers, Rust tests or Wasm builds.
