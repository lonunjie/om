# Expressive-media wire rulings and Core evidence

## Design sources read

Read the [build plan](../docs/reports/2026-11-20-expressive-media-build-plan.md), [ENGINEER_START_HERE.md](../docs/process/ENGINEER_START_HERE.md), [AGENT_ARCHITECTURE_RULES.md](../docs/process/AGENT_ARCHITECTURE_RULES.md), ADR-0250 through ADR-0253, [ADR-0155](../decisions/ADR-0155-non-critical-extension-validity-and-presentation-edit-closure.md), ADR-0128 rule 18, and the [constitution digest](../docs/constitution/CONSTITUTION_DIGEST.md), including C-26, C-43 and C-44.

Read the [expressive-media architecture](../docs/architecture/ecosystem/EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md), [hostile-content boundary](../docs/architecture/safety/RICH_TEXT_AND_CONTENT_SECURITY.md), [blob architecture](../docs/architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md), [profile isolation](../docs/architecture/client/MULTI_IDENTITY_AND_PROFILE_ISOLATION.md), [expressive CDDL](../spec/cddl/expressive.cddl), [media-profile registry](../spec/registries/om-expressive-media.json) and [slot overlay](../spec/registries/om-w1-expressive-slots.json). Inspected the relevant enum, codec branch, vector generator and frozen fixture rows.

## Design sources not read in the earlier pass

The earlier pass stopped before finishing the mapped Core domains, the event model, the messenger baseline, the durable-signature and account-control documents, and the later UI and distribution sources.

## Slice 2 ruling

The operator ruled on 2026-11-20 that Slice 2 follows the frozen W1 wire until an ADR changes it. The frozen grammar admits only `expressive-kind = 1..4`. An unknown kind in a reaction or inline emoji body rejects the event with `value_range` before presentation. The frozen corpus explicitly rejects a reaction event carrying kind 5. [Build plan, slice 2](../docs/reports/2026-11-20-expressive-media-build-plan.md#slice-2-core-sending-and-receiving-expressive-content) · [Expressive CDDL](../spec/cddl/expressive.cddl#L29) · [Frozen vectors](vectors/expressive-wire-vectors.json.gz)

The message-create extension uses the same closed enum. With an unknown kind, its payload is unusable and the base message survives under ADR-0155. Its `alt_text` is in the unusable payload. An unknown `media_profile_id` already has the required placeholder behavior when the kind is valid. [ADR-0252 rules 1–3](../decisions/ADR-0252-expressive-media-wire-freeze-and-delegated-constructions.md) · [ADR-0155 §1](../decisions/ADR-0155-non-critical-extension-validity-and-presentation-edit-closure.md#1-a-non-critical-registered-extension-never-decides-containing-object-validity)

For an unrecognized discriminator, Slice 2 reserves the `alt_text` placeholder for a wire-valid kind with an unsupported media profile. This ruling uses the existing W1 wire and closes the earlier question.

## Commands and results

`python3 tools/design_gate.py needs --brief crates/om-core/src/expressive crates/om-core/src/lib.rs crates/om-codec/src verification/prototypes/expressive-wire verification/EXPRESSIVE_NOTES.md machine/file_registry.json` exited 0. Its output listed `crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability` and `time_ordering_events`. The notes-only path check returned the following.

```text
$ python3 tools/design_gate.py needs --brief verification/EXPRESSIVE_NOTES.md machine/file_registry.json
no mapped code path: the design gate does not apply
```

The following command read the frozen fixture expectations directly.

```sh
python3 - <<'PY'
import gzip
import json

with gzip.open("verification/vectors/expressive-wire-vectors.json.gz", "rt") as f:
    corpus = json.load(f)
for section in ("vectors", "structs"):
    for row in corpus[section]:
        row_id = row["id"]
        if row_id.startswith(("ConversationEvent/", "ExpressiveAssetRef/", "ExpressiveAssetV1Extension/")) and ("kind-5" in row_id or "unknown-profile" in row_id):
            print(row_id, row["expect"], row.get("error"), row.get("semantic"))
PY
```

```text
ConversationEvent/reaction-unknown-profile-placeholder accept None {'verdict': 'placeholder', 'rule': 'unknown_profile'}
ConversationEvent/create-extension-unknown-profile-placeholder accept None {'verdict': 'placeholder', 'rule': 'unknown_profile'}
ConversationEvent/neg/reaction-asset-kind-5 reject value_range None
ExpressiveAssetRef/unknown-profile accept None None
ExpressiveAssetRef/neg/kind-5 reject value_range None
ExpressiveAssetV1Extension/neg/kind-5 reject value_range None
```

The existing W0 attachment test was the only Rust test run.

```text
$ cargo test -p om-core --lib events::tests::attachments_and_presentation_extensions_are_w0_valid_and_parse --offline
running 1 test
test events::tests::attachments_and_presentation_extensions_are_w0_valid_and_parse ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 75 filtered out; finished in 0.01s
```

## Slice 2 wire conflict: scope of inline type 4

### Design sources read for this finding

Read the build plan, the constitution digest, ADR-0128, ADR-0155, ADR-0156, ADR-0165 and ADR-0176, ADR-0250 through ADR-0253, the expressive-media and hostile-content architecture, the event model, the messenger baseline, the storage and profile-isolation docs, and the durable-signature and account-control docs. Checked the relevant rules in [`expressive.cddl`](../spec/cddl/expressive.cddl), [`events.cddl`](../spec/cddl/events.cddl), [`group.cddl`](../spec/cddl/group.cddl) and [`discovery.cddl`](../spec/cddl/discovery.cddl), the two W1 overlays, the numeric-fields registry, the frozen corpus and the product codec's registry loaders.

### Design sources not read in full

The full mapped-domain reading was still in progress. The original constitution documents were covered through the digest, and later UI, decoder-driver and distribution sources were outside the finding.

### Conflicting acceptance

The expressive CDDL introduces `RichTextX` for `MessageCreateX` and `MessageEditX`. The frozen group CDDL still types `PollCreate.question` as `RichText`, and the frozen discovery CDDL does the same for `MessageRequestPayload.greeting`. The original `RichText` uses the inline union with variants 1–3. [Expressive CDDL](../spec/cddl/expressive.cddl#L57) · [Group CDDL](../spec/cddl/group.cddl#L217) · [Discovery CDDL](../spec/cddl/discovery.cddl#L107) · [W0 inline grammar](../spec/cddl/events.cddl#L61)

The frozen expressive overlay instructs codecs to add type 4 to the shared `Inline` union and remove its wave marker. That merge changes every caller of `RichText` in the merged registry, including the poll and first-contact structures. The group overlay changes event-body dispatch and leaves that shared union in place. A decoder applying the registry merge as written accepts these bodies while a decoder using their frozen CDDL refuses them. [Expressive overlay](../spec/registries/om-w1-expressive-slots.json) · [Group overlay](../spec/registries/om-w1-event-bodies.json) · [ADR-0252](../decisions/ADR-0252-expressive-media-wire-freeze-and-delegated-constructions.md)

The probe reused the valid inline node and its asset from `ConversationEvent/create-inline-custom-emoji`. It used that RichText as the question of a two-option `PollCreate` and as key 6 of `MessageRequestPayload/minimal`. The checks exercised the codec's struct and unsigned-body validation. They establish a wire grammar difference, with no claim that a production receiver currently uses the expressive overlay or that a first-contact signature would verify after mutation.

**Ruling 2, 2026-11-20.** Core follows the normative CDDL and rejects type 4 with `value_range` outside `message.create.content` and `message.edit.replacement`. The scoped product loader uses the frozen overlay's field definitions and confines the extended inline grammar to those two fields, including nested link and mention labels. The W0 grammar remains active for poll questions, first-contact greetings and every other `Inline` use. The loose overlay is a finding for a later additive ADR. The frozen CDDL, registries and vector corpus retain their existing bytes.

### Commands and results from this pass

`python3 tools/design_gate.py needs --brief crates/om-core/src/expressive crates/om-core/src/lib.rs crates/om-core/tests crates/om-codec/src verification/prototypes/expressive-wire verification/EXPRESSIVE_NOTES.md machine/file_registry.json` exited 0 and listed `crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability` and `time_ordering_events`.

```text
$ cargo test -p om-codec --test expressive_vectors --offline
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.96s

$ CARGO_TARGET_DIR="$PWD/target" cargo test --offline --manifest-path /private/tmp/om-expressive-wire-4a_s74en/Cargo.toml inline_union_scope -- --nocapture
running 1 test
PollCreate.question: W1 groups = Err("value_range"), expressive overlay = Ok(())
MessageRequestPayload.greeting: W0 = Err("value_range"), expressive overlay = Ok(())
MessageCreate.content: expressive overlay = Ok(())
test inline_union_scope ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```

## Slices 1–3 Core pass

### Design sources read

This pass used the build plan, the constitution digest, the engineer entry point and architecture rules, ADR-0128 rule 18, ADR-0250 through ADR-0253, the expressive-media and hostile-content architecture, the event model, the messenger baseline, the storage architecture, profile isolation, account-control continuity, and the durable-signature section of the cryptography architecture. It checked the frozen expressive CDDL, the W0 and group inline sites, the event-body and expressive overlays, the media and domain-separation registries and the frozen negative fixtures. The earlier readings are recorded above. The header walker used the WebP RIFF and bitstream header layouts. [WebP container specification](https://developers.google.com/speed/webp/docs/riff_container) · [WebP lossless bitstream specification](https://developers.google.com/speed/webp/docs/webp_lossless_bitstream_specification)

The entire six-domain gate reading, including all mapped ADRs, was not completed in this pass. Original constitution documents were covered through the digest. Later UI, distribution and platform decoder sources were outside the Core implementation.

### Implementation boundaries and follow-up findings

- The header walker consumes borrowed slices with fixed-size scratch state. It refuses unknown or nested container chunks outside the admitted WebP shapes. It checks the frozen aggregate frame-rate rule and validates declared width, height, duration and encoded size against the container. The product decoder belongs in the later platform slice.
- A closed wire kind fails with `value_range` in inline and reaction bodies. An unusable non-critical create extension is ignored. A recognized kind with an unsupported media profile exposes its inert `alt_text` without a fetch plan. If an inline fallback exceeds its 128-scalar budget, the sender emits plain W0 text.
- Capability selection reads the ACS-committed credential of every eligible recipient device. Missing credentials or capabilities select W0 fallback. Enabling the feature before account creation advertises the frozen capability IDs. Existing and newly linked credentials retain their capability sets until the applicable credential reissue path is integrated.
- This Core carriage is for 1:1 relationships. Room carriage needs an attachment path and a complete authenticated set of member-device capabilities. The current Room flow retains its baseline grammar and refuses type 4 in polls.
- The shared-blob option reuses an explicitly marked, still-stored outgoing reference within the profile. The metadata-exposure line is available through the Core policy API. The provider disclosure surface must consume it before release.
- Pack inventory is isolated in a local DeviceVault record. Core validates supplied ciphertext from the host's content-addressed local cache, then stages a single activation write. The host owns cache-byte lifetime. An uninstall drops the manifest and cache membership and retains the lineage pin. A pack with an unsupported profile can be installed lazily. A full or favourites-only cache requires validated assets for its selected items.
- Content-pack forks use new pack IDs. A known fork edge is checked against held lineage. An unresolved source digest is inert provenance. Unknown non-critical extension data remains in the hashed manifest and acquires no behavior.

### Unfinished

- ADR-0251 release evidence still needs instrumented libFuzzer and AFL++ runs for all four parsers and the full per-target CPU-hour budget. The offline header property harness is smoke evidence. Platform decoder cross-checks, the worker's memory and time ceilings, and raw and gzip wasm deltas remain unmeasured.
- Room carriage, capability reissue integration, the provider disclosure consumer, and the platform cache and decoder integrations remain outside the implemented 1:1 Core path.
- A later additive ADR should reconcile the loose inline overlay with the CDDL scope. The scope flag stays off by default.

### Current offline test results

All of the following commands exited 0. The blocks preserve Cargo's result lines. When a command ran several binaries, the binary names identify the lines in execution order.

```text
$ cargo test --offline -p om-codec --test expressive_vectors --test w0_vectors
expressive_vectors
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.97s
w0_vectors
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s

$ cargo test --offline -p om-core --lib expressive
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 76 filtered out; finished in 0.84s

$ cargo test --offline -p om-core --test attachments --test expressive_harness --test backup --test secrets --test w0_conformance --test chat_context --test chat_mentions --test first_contact --test fan_out --test device_link_vectors --test v001_vectors
attachments
test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.96s
backup
test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.78s
chat_context
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.78s
chat_mentions
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s
device_link_vectors
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
expressive_harness
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.33s
fan_out
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.67s
first_contact
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.49s
secrets
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
v001_vectors
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.85s
w0_conformance
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s

$ cargo test --offline -p om-core --test attachments expressive_w0_device_gets_fallback_and_no_w1_slot
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 28 filtered out; finished in 0.43s

$ cargo test --offline -p om-core --test scenarios state_survives_restart_and_messaging_continues
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 13 filtered out; finished in 0.20s
```

`git diff --check`, `git diff --cached --name-only` and `git diff --name-only -- spec` exited 0 with empty output. All changes are local and unstaged.
