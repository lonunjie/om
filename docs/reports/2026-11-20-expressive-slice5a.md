# Expressive media slice 5a Core binding and TypeScript handoff

## Design sources read

The specified design gate returned the following domains. The mapped non-constitution documents were read in full.

- `crypto_1to1` — [`CRYPTOGRAPHY_AND_AGILITY.md`](../architecture/crypto/CRYPTOGRAPHY_AND_AGILITY.md), [`PRODUCTION_CRYPTO_INTEGRATION_GATE.md`](../architecture/crypto/PRODUCTION_CRYPTO_INTEGRATION_GATE.md), [`ASYNC_KEY_MATERIAL.md`](../architecture/crypto/ASYNC_KEY_MATERIAL.md), [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md), [`MULTI_DEVICE_SYNC.md`](../architecture/crypto/MULTI_DEVICE_SYNC.md), [`CANONICAL_ENCODING.md`](../architecture/crypto/CANONICAL_ENCODING.md).
- `discovery_first_contact` — [`DISCOVERY_FABRIC.md`](../architecture/discovery/DISCOVERY_FABRIC.md), [`INVITES_INTROGATE_AND_SPAM.md`](../architecture/discovery/INVITES_INTROGATE_AND_SPAM.md), [`CONTACT_SHARING.md`](../architecture/discovery/CONTACT_SHARING.md), [`INTRODUCTION_AND_SERVICE_ACCOUNTS.md`](../architecture/discovery/INTRODUCTION_AND_SERVICE_ACCOUNTS.md), [`KEY_TRANSPARENCY_AND_DIRECTORY.md`](../architecture/discovery/KEY_TRANSPARENCY_AND_DIRECTORY.md).
- `identity_control_recovery` — [`IDENTITY_PROTOCOL.md`](../architecture/identity/IDENTITY_PROTOCOL.md), [`IDENTITY_CONTINUITY.md`](../architecture/identity/IDENTITY_CONTINUITY.md), [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md), [`RECOVERY_AND_BACKUP_POLICY.md`](../architecture/identity/RECOVERY_AND_BACKUP_POLICY.md), [`IDENTITY_DISCOVERY_VERIFICATION.md`](../architecture/identity/IDENTITY_DISCOVERY_VERIFICATION.md), [`IDENTIFIER_SPOOFING_AND_DISPLAY.md`](../architecture/identity/IDENTIFIER_SPOOFING_AND_DISPLAY.md), [`ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md`](../architecture/identity/ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md).
- `metadata_privacy` — [`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](../architecture/delivery/MULTI_RELAY_AND_DELIVERY_RESILIENCE.md), [`NOTIFICATIONS_AND_PUSH.md`](../architecture/client/NOTIFICATIONS_AND_PUSH.md), [`STORAGE_AND_BLOB_ARCHITECTURE.md`](../architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md), [`SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md`](../architecture/groups/SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md), [`METADATA_THREAT_MODEL.md`](../threat-model/METADATA_THREAT_MODEL.md).
- `relay_reachability` — [`RELAY_AND_DELIVERY_ARCHITECTURE.md`](../architecture/delivery/RELAY_AND_DELIVERY_ARCHITECTURE.md), [`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](../architecture/delivery/MULTI_RELAY_AND_DELIVERY_RESILIENCE.md), [`REACHABILITY_AND_OBJECT_REPAIR.md`](../architecture/delivery/REACHABILITY_AND_OBJECT_REPAIR.md).
- `time_ordering_events` — [`EVENT_MODEL.md`](../architecture/events/EVENT_MODEL.md), [`TIME_AND_EXPIRY_SEMANTICS.md`](../architecture/events/TIME_AND_EXPIRY_SEMANTICS.md), [`CANONICAL_ENCODING.md`](../architecture/crypto/CANONICAL_ENCODING.md), [`MESSENGER_BASELINE_V1.md`](../architecture/events/MESSENGER_BASELINE_V1.md), [`SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`](../architecture/groups/SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md).

The mapped ADRs were ADR-0002, ADR-0003, ADR-0004, ADR-0008, ADR-0010, ADR-0011, ADR-0012, ADR-0019, ADR-0020, ADR-0021, ADR-0022, ADR-0025, ADR-0026, ADR-0027, ADR-0028, ADR-0029, ADR-0030, ADR-0033, ADR-0040, ADR-0063, ADR-0065, ADR-0066, ADR-0067, ADR-0072, ADR-0073, ADR-0075, ADR-0076, ADR-0087, ADR-0088, ADR-0098, ADR-0105, ADR-0106, ADR-0107, ADR-0111, ADR-0118, ADR-0119, ADR-0120, ADR-0121, ADR-0124, ADR-0126, ADR-0127, ADR-0128, ADR-0129, ADR-0130, ADR-0140, ADR-0157, ADR-0159.

Slice-specific sources were the expressive-media build plan, the slice 4 report, [`EXPRESSIVE_NOTES.md`](../../verification/EXPRESSIVE_NOTES.md), ADR-0250 through ADR-0253, the expressive-media architecture, [`expressive.cddl`](../../spec/cddl/expressive.cddl), the media asset-pack and attachments UX specs, the semantic motion, accessibility and action registry specs, and ADR-0082, ADR-0083, ADR-0089, ADR-0090 and ADR-0091. The engineering entry point, architecture rules, drift control and transaction contract, the expressive scope entries in the machine contracts, the rich-text hostile-content boundary, profile isolation, the baseline ecosystem queue, the product and UX principles, and ADR-0187 were also read.

The constitution was covered through [`CONSTITUTION_DIGEST.md`](../constitution/CONSTITUTION_DIGEST.md) and `constitution.py find` for extension, privacy and local-first rules. The mapped [`PRIVACY_GOVERNANCE_AUTHORITY.md`](../constitution/PRIVACY_GOVERNANCE_AUTHORITY.md) and the other originals were outside the full reading under the digest exception. The slice 5 UI component, microcopy and haptic specs, slice 6 distribution sources, and slice 7 evidence plan were outside this binding pass.

## Binding

- The main Wasm entry point exposes the readiness query. The expressive commands are compiled into the engineering artifact built by `pnpm --filter @om/core-wasm build:wasm:expressive`. Load `@om/core-wasm/om_wasm_expressive.wasm` with the existing loader and pass `expressiveMedia: true` to the Core runtime before creating test identities. The runtime option and the slice 4 platform capability each default to off.
- `SendExpressive` uses the existing Core dispatch and durable outcome path. Its attachment target accepts a prepared sticker or loop. Its inline and reaction targets use the existing W1 carriage and W0 fallback decisions. Pack items prepare as freshly encrypted blobs and use the existing profile-local staging and delivery path.
- `CoreClient.expressive` exposes pack preview, explicit install, removal, installed pack list, picker state, and opened conversation assets. An install requires confirmation of the reviewed release and controller state. Favourites and recents are local to a profile's installed release. The host commits the DeviceVault write before reporting acceptance.
- `expressive.decode` gives the slice 4 driver the opened plaintext and the corresponding Core header facts. After the driver returns, Core reopens the same source against its current profile state and compares dimensions and any observed frame count. Refusal closes playback and yields the alt-text placeholder. The bitmap-only path may return an unknown frame count. The caller owns the lifecycle of accepted playback.
- The dev harness exercises the same client API with seeded pack and message fixtures. It simulates preparation, send and open, profile-local picker state, confirmation binding, and decoder mismatches for UI tests.

Core holds pack, message and rendering authority. The host moves ciphertext through the existing provider API and provides local pack cache bytes. Pack provenance is display metadata. The decoder supplies observations and frames.

## Integration boundaries

The separate artifact follows the groups build precedent. A current before/after Wasm size comparison was outside this pass. The build host needs to measure raw and gzip artifacts and apply the build plan's 150 KB raw delta threshold before considering the main module.

The current JSON ABI caps each request at 16 MiB. Full and favourites-only installs whose supplied cache blobs, hex encoding and manifest exceed that envelope require a multi-request cache path. Lazy manifest installation fits the existing wire bounds. Pack import, cache acquisition and UI placement belong to later slices. The inline target sends the existing Core single-node form. The binding uses the existing 1:1 carriage.

## Verification

| Check | Result |
| --- | --- |
| `cargo check -p om-wasm --offline` | Passed. |
| `cargo check -p om-wasm --offline --tests --features expressive-media` | Passed. |
| `cargo check -p om-wasm --offline --features expressive-media,om-core/production` | Passed. |
| `cargo check -p om-core --test attachments --offline` | Passed. |
| `pnpm --filter @om/core-harness exec vitest run --dir src` | 38 passed. |
| `pnpm --filter @om/core-wasm exec vitest run --dir src` | 88 passed. The handoff tests use typed mock bridge responses. Existing-artifact tests used the local Wasm binaries. |
| `pnpm --filter @om/platform-api exec vitest run --dir src` | 9 passed. |
| `pnpm --filter @om/web-platform exec vitest run --dir src` | 127 passed. |
| `pnpm exec tsc -b apps/web` and scoped ESLint for the web app and modified packages | Passed. |
| `python3 tools/web_guard.py` and `python3 tools/core_guard.py` | Both reported zero errors. |
| `python3 tools/om_docs.py fmt`, `make check` and `git diff --check` | Passed. |

Native scenarios cover default-off behavior, stale profile contexts, pack picker commit and restart behavior, and decoder observation mismatches. Build-host verification includes native test execution, the newly built Wasm variant, the import guard on that artifact, and raw/gzip measurement. Browser execution and V-012 release evidence remain open.

The work is uncommitted.
