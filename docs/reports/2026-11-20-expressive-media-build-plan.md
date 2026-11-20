# Expressive media build plan (scope `expressive_media`, authorized by ADR-0250)

Date: 2026-11-20. Status: scope **authorized** on the operator's explicit instruction, 2026-11-20 ([`ADR-0250`](../../decisions/ADR-0250-authorize-expressive-media-engineering.md), Accepted); no product code written yet. Q1 and Q3 are answered by ADR-0251; Q4 and Q5 stay open (see ADR-0251).

## Design sources read

Read in full: [`EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md`](../architecture/ecosystem/EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md), [`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](../process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md), [`expressive-asset.logical.schema.json`](../../spec/protocol/expressive-asset.logical.schema.json), [`content-pack.logical.schema.json`](../../spec/protocol/content-pack.logical.schema.json), ADR-0082, ADR-0083, ADR-0089, ADR-0090, ADR-0091, ADR-0217 (template), the `expressive_media` entry of [`engineering_scope_contracts.json`](../../machine/engineering_scope_contracts.json), the `expressive_media` and `calls` entries of [`repository_contract.json`](../../machine/repository_contract.json), every row of [`architecture_verification_gates.jsonl`](../../machine/architecture_verification_gates.jsonl) (V-012 is the only gate listing the scope), the 2026-11-18 calls build plan (structure model).

Read in part (the passages that name expressive media): ADR-0129 (rule 10, ExpressiveAssetRef is an attachment-ref), ADR-0128 rule 18 (later-wave content never breaks W0 receivers), [`RICH_TEXT_AND_CONTENT_SECURITY.md`](../architecture/safety/RICH_TEXT_AND_CONTENT_SECURITY.md) ("Hostile content boundary", "Images/video/audio", the 2026-09-23 expressive inline refinement), [`events.cddl`](../../spec/cddl/events.cddl) (`attachment-ref`, `ReactionSet` key 3 and inline type 4 reserved slots, extensions key 23), the V-012 section of [`FEASIBILITY_RECONNAISSANCE_20260923.md`](../../verification/FEASIBILITY_RECONNAISSANCE_20260923.md) and of [`GATE_PASS_CRITERIA.md`](../../verification/GATE_PASS_CRITERIA.md), [`MESSENGER_BASELINE_V1.md`](../architecture/events/MESSENGER_BASELINE_V1.md) (sticker, animation and reaction rows), [`MEDIA_ASSET_PACKS.md`](../ux-design/specs/MEDIA_ASSET_PACKS.md) (opening), [`ARCHITECTURE_MANDATED_V1_SURFACES.md`](../ux-design/specs/ARCHITECTURE_MANDATED_V1_SURFACES.md) section 5, [`PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md`](../architecture/ecosystem/PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md) (the sticker-pack lines), the constitution through [`CONSTITUTION_DIGEST.md`](../constitution/CONSTITUTION_DIGEST.md) and `constitution.py find`.

**Not read, and every slice must read before it builds** (run `python3 tools/design_gate.py needs <paths>` for the slice's paths; note it currently reports "no mapped code path" for docs paths, and the `expressive_media_content_packs` domain of [`architecture_map.json`](../../machine/architecture_map.json) is the map to follow): [`entitlement-presentation.logical.schema.json`](../../spec/protocol/entitlement-presentation.logical.schema.json), [`rich-text.logical.schema.json`](../../spec/protocol/rich-text.logical.schema.json), [`MESSENGER_BASELINE_AND_ECOSYSTEM_QUEUE.md`](../process/MESSENGER_BASELINE_AND_ECOSYSTEM_QUEUE.md) and the A-001 to A-003 resolutions, [`PRODUCT_AND_UX_PRINCIPLES.md`](../overview/PRODUCT_AND_UX_PRINCIPLES.md) in full, the rest of [`RICH_TEXT_AND_CONTENT_SECURITY.md`](../architecture/safety/RICH_TEXT_AND_CONTENT_SECURITY.md), [`EVENT_MODEL.md`](../architecture/events/EVENT_MODEL.md), [`STORAGE_AND_BLOB_ARCHITECTURE.md`](../architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md), [`MULTI_IDENTITY_AND_PROFILE_ISOLATION.md`](../architecture/client/MULTI_IDENTITY_AND_PROFILE_ISOLATION.md), [`CRYPTOGRAPHY_AND_AGILITY.md`](../architecture/crypto/CRYPTOGRAPHY_AND_AGILITY.md) (the durable-signature profile), [`ATTACHMENTS_AND_MEDIA_UX.md`](../ux-design/specs/ATTACHMENTS_AND_MEDIA_UX.md), [`SEMANTIC_MOTION_AND_DELIGHT.md`](../ux-design/specs/SEMANTIC_MOTION_AND_DELIGHT.md), `ACCESSIBILITY_AND_INPUT.md`, [`ACTION_REGISTRY.md`](../ux-design/specs/ACTION_REGISTRY.md), ADR-0156, ADR-0165, ADR-0176 and ADR-0218 (the freeze method), ADR-0150, ADR-0148 (how optional non-critical extensions were promoted), the whole of [`MEDIA_ASSET_PACKS.md`](../ux-design/specs/MEDIA_ASSET_PACKS.md).

## 1. What the scope covers, per the design

Wave W1 ("stickers, animation, content packs"), depends on `core_kernel` only (already authorized, ADR-0169). The design is "canonical pre-engineering architecture" (checkpoint 2026-09-23) and `design_open=0`; the queue items A-001 to A-003 are resolved.

- **Inert bounded media.** Four semantic kinds (`sticker`, `custom_emoji`, `reaction_asset`, `loop_animation`); one baseline profile `om.expressive.webp-v1` (static or animated WebP, alpha allowed, no audio, no scripts, no SVG, Lottie or TGS). Ceilings: `sticker.v1` 512x512, 1 MiB, 3 s, 30 fps; `custom-emoji.v1` 256x256, 512 KiB, 3 s, 30 fps; `loop-animation.v1` 1024x1024, 6 MiB, 10 s, 30 fps. A versioned decoded-pixel and frame budget is enforced before or during decode. OM writes no WebP codec; platform decoders are used. Unknown profiles degrade to a placeholder and fallback text.
- **`ExpressiveAssetRef`** is the frozen W0 `attachment-ref` plus `kind`, `media_profile_id`, `alt_text`, optional `fallback_text`, optional `pack_ref` (ADR-0129 rule 10). Fresh encryption and a new blob per send by default; a declared `shared_blob` mode is off by default and carries a MetadataExposureDescriptor line (V-015).
- **Where it travels.** Stickers and animations: ordinary `message.create` with an expressive attachment-ref. Custom reactions: `ReactionSet` key 3 (`reaction_asset`). Custom emoji in text: inline type 4. All three are the W1 reserved slots; a sender emits them only when every recipient device advertises the encoding, otherwise a W0-valid fallback (ADR-0128 rule 18).
- **Rendering authority** is the digest-bound blob, never a pack, catalog or entitlement service. Entitlement gates acquisition, install and update only, never viewing delivered content.
- **Content packs.** Data only: `ContentPackID` (random 256-bit), immutable canonical-CBOR `ContentPackRelease` with `ContentPackReleaseID` as its hash, optional hash-chained `ContentPackControlState` signed under the durable-signature profile, forks get a new ID with `forked_from`. Installable from a message, file, QR or deep link, website, any catalog; no mandatory catalog. Provenance is never a copyright, safety or "official" claim (R-108, C-83).
- **Entitlement.** An optional `EntitlementPresentation` interface for distributors; no payment rail, price, token or marketplace (ADR-0091).
- **Client rules.** Reduced Motion shows a static representative frame until the person plays it; stickers are contextual, never permanent navigation; every animated asset has a static fallback.
- **Scope contract** ([`engineering_scope_contracts.json`](../../machine/engineering_scope_contracts.json)): activation of an asset or pack is atomic and an interrupted install leaves the prior valid state; content-addressed cache retry never changes rendering identity; an invalid pack or asset is rejected or ignored, never executed; expressive content never becomes code or authority; a live catalog or locator never becomes rendering authority over the digest and BlobID.

## 2. Blocking gates and open prerequisites

Unlock rule (section 5 of the unlock contract), checked 2026-11-20:

| # | Condition | State |
|---|---|---|
| 1 | Accepted ADR names the scope | **Met: ADR-0250 accepted 2026-11-20** |
| 2 | Every gate listing the scope, plus `always_required` (V-017), is passed or waived | V-017 passed. **V-012 `pre_engineering_status` open, independence open.** It is the only gate with `unlock_scopes` containing `expressive_media`. |
| 3 | No open design-queue item lists the scope | Met (A-001 to A-003 resolved) |
| 4 | `depends_on` authorized | Met (`core_kernel`) |
| 5 | Gates with `interface_impact` listing the scope evidenced | Met (none list it) |
| 6 | Clean [`validate_architecture.py`](../../tools/validate_architecture.py) | Met at the time of writing |

V-012 pre-engineering criteria, all three unmet and no evidence recorded (`criteria_met` and `evidence` are empty; no `verification/prototypes/` directory for the gate exists, and the "existing ContentPack manifest vectors" the third criterion refers to do not exist in the repository):

1. Candidate decoders and pack parsers fuzzed for an agreed CPU-hour budget under the resource ceilings, no unresolved crash (independent tool).
2. Canonical ContentPack manifest and signature vectors agree across two implementations (independent implementation).
3. ADR-0156 wire freeze: frozen CDDL, registered numeric fields and domain labels, positive and negative vectors with two independent codecs, for inline type 4, `ReactionSet` key 3, `ExpressiveAssetRef` and the manifest. There is no `spec/cddl/` file for any of it; the schemas are logical JSON only, which the unlock contract says never substitutes.

Pre-release (never an engineering blocker): shipped parsers under continuous fuzzing. Additionally the scope contract lists "content-pack/media conformance and parser/resource-limit evidence required by its gate" as an integration prerequisite, and the fuzz budget ("agreed CPU-hour budget") is **not yet agreed** (question Q1).

ADR-0250 is accepted, so V-012 pre-engineering is waived for engineering only, as ADR-0217 did for V-007, and the three criteria become compensating controls inside slices 0, 1 and 7 and release obligations.

## 3. Architecture of the build

| Layer | Where | Responsibility |
|---|---|---|
| Wire | `spec/cddl/expressive.cddl` (new), registries, vectors | frozen first, additive, ADR-0165 |
| Core | `crates/om-core` (new `expressive` module) | profile and budget validation of header-level facts, capability gate and fallback choice, `ContentPackRelease` verification, control-state lineage, atomic install, pack store; the only authority on what is renderable |
| Binding | `om-wasm`, `packages/om-core-wasm`, `packages/om-platform-api` | typed intents (`SendSticker`, `SetReactionAsset`, `InstallPack`, `RemovePack`) and a view of installed packs and renderable assets |
| Decoder driver | `packages/om-web-platform` | browser-native WebP decode with the decoded-pixel and frame budget; reports facts back to Core; no custom codec, no wasm bytes |
| UI | `packages/om-ui` | picker, sticker-only message, custom reaction, inline custom emoji, Reduced Motion static frame, pack details and install confirmation |
| Transport and storage | existing attachments, blobs, repair, `message.create`, `reaction.set` | unchanged; no new endpoint, no catalog |

## 4. Constitution rules that bind it

Cited from [`CONSTITUTION_DIGEST.md`](../constitution/CONSTITUTION_DIGEST.md); the originals win on disagreement.

- **C-04** creator-disappearance: a sent sticker renders after the pack, creator or catalog vanishes. Slice 2 and 3 acceptance tests it.
- **C-06, C-63, C-65, C-81** no indispensable institution, no hidden privilege, no proprietary mandatory extension, no mandatory project service: no default catalog, no compiled-in fetch address, WebP only (no royalty-bearing mandatory codec).
- **C-10, C-12** providers are swappable tools; no storage, catalog or commerce provider owns history meaning.
- **C-20, C-44** floors unchanged; a security-affecting change needs a new extension ID, never a version bump. A profile that would change fallback validity gets a new ID.
- **C-29, C-31** privacy structural; no telemetry, no beaconing to a creator or catalog server to render a message (the "automatic fetching" privacy-profile rule). Shared-blob linkage is a declared exposure, off by default.
- **C-40, C-41, C-42, C-43, C-46** criticality, public registries, the permanent critical prefix, immutable IDs, shared meaning published independently of code: the new inline node, reaction field and pack extension are registered, non-critical where a W0 fallback exists (ADR-0128 rule 18), and never expressed only in client code.
- **C-47, C-56** canonical encoding and golden vectors before freeze; freeze only what can outlive its library (pack signing uses the existing durable-signature profile, not a new primitive).
- **C-26** ambiguity at a wire or security boundary is resolved in design, not improvised: see section 6.
- **C-70, C-80, C-83** capability is not operation, liability follows the operator, signatures prove provenance not copyright; the UI never says "verified", "licensed" or "official" from a signature (R-108).
- **C-89** OwnMessenger: media and themes replaceable without weakening the security floor; packs and themes never get UI-spoofing power (security invariant 9).

Rules of the design gate that bind it: no invention (design gate 2 and 4); the Design-Read and Design-Departures trailers on every code commit.

## 5. Slices

Each slice fits one agent session, ends in a commit, keeps `make check` green and carries the trailers. The feature sits behind a capability flag that is off in every release until the V-012 release evidence exists. Slice 0 is the only thing that may be built first. Nothing sizes the wasm delta yet; slices 1 to 3 each record the raw and gzip delta (there is no budget file for the main module; a measured delta above 150 KB raw is a stop-and-ask, the same ceiling the calls plan uses).

### Slice 0. Wire freeze and the delegated constructions (specification, no product code)

- **Reads:** the full `expressive_media_content_packs` domain, ADR-0156, ADR-0165, ADR-0176, ADR-0218, [`events.cddl`](../../spec/cddl/events.cddl), [`common.cddl`](../../spec/cddl/common.cddl), [`CANONICAL_ENCODING.md`](../architecture/crypto/CANONICAL_ENCODING.md).
- **Does:** `spec/cddl/expressive.cddl` with `ExpressiveAssetRef` (as attachment-ref plus registered keys), the `ReactionSet` key 3 body, inline type 4 `custom_emoji`, `ContentPackRelease`, `ContentPackItem`, `ContentPackControlState`, the `media_profile_id` registry entries (`om.expressive.webp-v1` and the three resource profiles as versioned, immutable IDs), the capability ID a device advertises for each encoding, domain-separation labels for `ContentPackReleaseID` and pack signatures, the decoded-pixel and frame budget numbers (once ruled, Q3). Vectors under `verification/vectors/`; a verification prototype under `verification/prototypes/expressive-wire` modelled on `call-wire`. Additive registry changes only. ADR by delegation for each construction the design leaves open and that Q-numbers below do not reserve for the operator. File registry, requirement and traceability ledgers updated.
- **Done when:** `python3 tools/validate_architecture.py` and `make check` pass; two independently written codecs (Rust `om-codec` and the second codec used for the ADR-0176 and ADR-0218 freezes) decode and re-encode every positive vector identically and reject every negative one, command named in the report; negative vectors cover oversized profile claims, unknown `kind`, unknown critical extension, duplicate keys, non-canonical encoding, a `pack_ref` with a mismatched release, and a W0 receiver fed the W1 body; a test proves no field carries an AccountID or delivery capability; ContentPack manifest and signature vectors agree across both codecs (this closes V-012 criteria 2 and 3 as evidence, even though the gate stays waived, not flipped).

### Slice 1. Core profile and budget validator, and the hostile-input harness

- **Reads:** [`RICH_TEXT_AND_CONTENT_SECURITY.md`](../architecture/safety/RICH_TEXT_AND_CONTENT_SECURITY.md) in full, [`STORAGE_AND_BLOB_ARCHITECTURE.md`](../architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md), slice 0 output.
- **Does:** `crates/om-core/src/expressive/` pure validator: given declared profile, kind and the container header facts (canvas, frame count, per-frame duration, total duration, encoded size) accept or reject against the versioned ceilings before any decode; reject mismatches between the ref's claimed `width`, `height`, `duration_ms`, `plaintext_size` and the container; a header-only WebP chunk walker with hard bounds (no decode); the corpus and fuzz harness (`cargo fuzz` or the repository's chosen tool) wired into `verification/`.
- **Done when:** unit and property tests reject every over-ceiling case from the design table (each profile, each limit, one over and exactly at); the chunk walker refuses truncated, overlapping, recursive-size and oversized-length inputs without allocating beyond a stated bound (asserted); a fuzz run of the agreed CPU-hour budget (Q1) ends with no unresolved crash, report recorded; the validator never calls a decoder and never opens a URL; wasm delta recorded.

### Slice 2. Core: sending and receiving expressive content

- **Reads:** [`EVENT_MODEL.md`](../architecture/events/EVENT_MODEL.md), [`MESSENGER_BASELINE_V1.md`](../architecture/events/MESSENGER_BASELINE_V1.md) rows, ADR-0128 rule 18 in full, [`MULTI_IDENTITY_AND_PROFILE_ISOLATION.md`](../architecture/client/MULTI_IDENTITY_AND_PROFILE_ISOLATION.md), the existing attachment code in `crates/om-core`.
- **Does:** sticker and loop-animation send as `message.create` plus an expressive attachment-ref with fresh key and new blob per send; custom reaction (`ReactionSet` key 3); inline `custom_emoji`; per-recipient-device capability check and the W0-valid fallback (Unicode or `fallback_text`); receive path: authenticated decrypt, digest check, validator, then "renderable" or placeholder with `alt_text`; the declared `shared_blob` mode implemented off by default with the exposure descriptor line; reaction convergence stays the normal `reaction.set` register.
- **Done when (Rust scenario tests, fake host):** a sticker reaches a recipient with no pack, no catalog and the sender's pack removed (C-04); an unknown `media_profile_id` or `kind` yields the placeholder and `alt_text`, no render effect; a W0 device in a recipient set forces the fallback body and no W1 slot is emitted; a tampered ciphertext is rejected by digest; two sends of the same sticker produce different BlobIDs and keys; with `shared_blob` on, one BlobID is reused and the descriptor test fails if the line is missing; restart mid-send never changes asset identity (`restart_contract`); golden state snapshots and backup migration tests still pass.

### Slice 3. Core: content pack release, lineage and atomic install

- **Reads:** [`CRYPTOGRAPHY_AND_AGILITY.md`](../architecture/crypto/CRYPTOGRAPHY_AND_AGILITY.md) durable-signature profile, [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) (signed control-state continuity pattern), slice 0 ADR.
- **Does:** `ContentPackRelease` verify (canonical bytes, `ContentPackReleaseID`), control-state chain verification, fork declaration, a pack store with staged install then atomic activation, removal, eviction, local policy (full cache, lazy, favourites-only), refusal of any non-data field. No catalog client, no fetch, no entitlement code.
- **Done when:** a hostile-manifest corpus (unknown keys, duplicate item IDs, over 512 items, reference to a blob outside the pack, oversized strings, cyclic `forked_from`, a release whose `previous_release_id` skips) is rejected with a stable reason; an interrupted install leaves the prior pack active (kill-point tests at every write); a fork cannot extend the old lineage; a lost controller key freezes updates without invalidating releases; removing a pack does not alter a previously received message; a signature never produces a "verified" flag in the view model beyond "signed by <evidence>" (R-108); no pack field can name an executable, URL-as-render-source or protocol hook.

### Slice 4. Web decoder driver (browser-native, budgeted)

- **Reads:** [`PLATFORM_CAPABILITY_MATRIX.md`](../architecture/client/PLATFORM_CAPABILITY_MATRIX.md), [`PWA_REFERENCE_CLIENT.md`](../architecture/client/PWA_REFERENCE_CLIENT.md), `apps/web` CSP and worker rules, [`tools/web_guard.py`](../../tools/web_guard.py).
- **Does:** `packages/om-web-platform` family capability that decodes through browser facilities in a worker where available, enforces the pixel and frame budget before allocation, supports pause and a static representative frame, reports facts to Core. No custom codec, no new dependency, lazy chunk.
- **Done when:** an image-bomb corpus (small file, huge declared canvas; many frames) is refused before decode within a measured memory ceiling; animated assets honour Reduced Motion (static until played); a decoder failure shows the fallback, never a broken embed; main bundle delta is the lazy stub only; `web_guard` and lint pass.

### Slice 5. UI: picker, sticker message, custom reaction, inline emoji

- **Reads:** [`AGENT_UX_BRIEF.md`](../ux-design/AGENT_UX_BRIEF.md), [`ATTACHMENTS_AND_MEDIA_UX.md`](../ux-design/specs/ATTACHMENTS_AND_MEDIA_UX.md), [`SEMANTIC_MOTION_AND_DELIGHT.md`](../ux-design/specs/SEMANTIC_MOTION_AND_DELIGHT.md), [`COMPONENT_BEHAVIOR_AND_STATES.md`](../ux-design/specs/COMPONENT_BEHAVIOR_AND_STATES.md), `ACCESSIBILITY_AND_INPUT.md`, [`ACTION_REGISTRY.md`](../ux-design/specs/ACTION_REGISTRY.md), [`CONTENT_AND_MICROCOPY_VOICE.md`](../ux-design/specs/CONTENT_AND_MICROCOPY_VOICE.md), [`MEDIA_ASSET_PACKS.md`](../ux-design/specs/MEDIA_ASSET_PACKS.md), [`HAPTIC_AND_SOUND_SEMANTICS.md`](../ux-design/specs/HAPTIC_AND_SOUND_SEMANTICS.md).
- **Does:** contextual picker (not permanent navigation), recents and favourites (local), sticker-only message presentation, custom reaction and inline custom emoji rendering, pack details with honest provenance wording, install confirmation, Action Registry entries, copy for every state and failure. Behind the build flag.
- **Done when:** jsdom plus axe for every state; keyboard-only Playwright test sends a sticker, reacts with a custom reaction and removes a pack; every asset exposes `alt_text` to assistive technology; Reduced Motion test; no remote image URL in the DOM for any expressive asset; copy lint passes (no codec, blob or signature vocabulary); a pack cannot restyle or overlay security UI (test against the safety-number and external-link surfaces).

### Slice 6. Distribution: install from a message, a file and a link

- **Reads:** [`PWA_REFERENCE_CLIENT.md`](../architecture/client/PWA_REFERENCE_CLIENT.md) deep-link handling, [`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](../architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md), [`SECURITY_PRIVACY_PROFILES.md`](../architecture/ux/SECURITY_PRIVACY_PROFILES.md).
- **Does:** direct pack sharing carrying the signed manifest and blob references, portable pack file import, deep link and QR handling, and the privacy-profile rule that no automatic third-party fetch happens under Maximum Privacy. No catalog, no entitlement, no marketplace.
- **Done when:** a pack installs from each of the three sources with no network request to any third-party host (request-log assertion); Maximum Privacy blocks the link path and says why in plain words; a malformed link or file never partially installs; the e2e test installs and sends from a received pack between two identities.

### Slice 7. Hardening and the first V-012 evidence (still not a release)

- **Reads:** [`verification/GATE_PASS_CRITERIA.md`](../../verification/GATE_PASS_CRITERIA.md), the V-012 gate, [`METADATA_TRACE_TEST_PLAN.md`](../process/METADATA_TRACE_TEST_PLAN.md).
- **Does:** the full fuzz budget on the real parsers and the browser path, a metadata trace (no relay-visible signal beyond ordinary deposits; shared-blob linkage line present), the V-012 evidence report listing each pre-engineering and pre-release criterion as met, partly met or open with links. The gate ledger is not flipped (independent tool and independent implementation still required).
- **Done when:** the report exists with evidence links; every bug found is fixed in its owning slice; `make verify` passes.

### Deferred (not in W1 build, outlined only)

- **Entitlement presentation and any concrete proof scheme:** the registry has no scheme defined; building a verifier before one is chosen would invent one. Needs its own ADR and the operator's yes (Q7).
- **Catalog and creator tooling, GIF search:** no design; capability is not operation (C-70); not built or hosted by the project.
- **AVIF, WebM, vector animation profiles:** a future optional capability after cross-platform decoder review.

## 6. Design silences, as questions

Per AGENTS.md "Design gate" rule 2, each stays unbuilt until answered; where an answer is delegable and does not amend audited text it is a delegated ADR in slice 0.

| # | Question | Why it matters | Who |
|---|---|---|---|
| Q1 (answered by ADR-0251) | What is the "agreed CPU-hour budget" for the fuzz criterion, and which fuzzer and corpus source? | V-012 criterion 1 names a budget nobody has agreed; slice 1 and 7 cannot say done. | Operator |
| Q2 | Which exact WebP parser and decoder libraries are in the verification prototype and the reference runtime? The feasibility note says the work "should begin only after" they are selected. | Criterion 1 is defined over "candidate decoders". Browser-native in the PWA is the design's stated preference; the header walker in Core is a plan choice. | Delegation, reviewed |
| Q3 (answered by ADR-0251) | What are the numeric decoded-pixel and frame budgets ("a versioned decoded-pixel/frame budget")? | The design names the budget but gives no figure, so slice 0 cannot freeze it. | Operator or delegation |
| Q4 | How does a device advertise that it understands inline type 4, `ReactionSet` key 3 and each media profile? ADR-0128 rule 18 requires the advertisement but this review found no registered capability ID or carriage. | Slice 2 cannot decide fallback per recipient device without it; ties to the `required_capabilities` and extension key 23 mechanism. | Delegation (new capability ID, additive) |
| Q5 | Which bytes are signed and under what domain label for `ContentPackRelease` and `ContentPackControlState`? The design says "OM's durable-signature profile" without naming the label or the controller/threshold encoding. | C-47 and V-012 criterion 2 need exact vectors. | Delegation |
| Q6 | The pack-ref fields: are `pack_id`, `release_id`, `item_id` carried in `ExpressiveAssetRef` as opaque 32-byte values with `item_id` a text string up to 128 characters (as the logical schema suggests)? Does an unverifiable `pack_ref` (release not installed) get displayed at all? | Attribution wording and R-108. | Delegation |
| Q7 | Is any entitlement code in W1? The design permits it as optional and registry-only; no proof scheme exists. | Avoid inventing a scheme. This plan says no. | Operator |
| Q8 | Pack and favourites sync across a person's devices: account-private sync, or local only? The design calls installed-pack UX "local policy". | Affects the multi-device story and the profile-isolation rule against cross-profile dedup. | Operator |
| Q9 | A received pack: may a recipient install it from a message with one tap, and what confirmation is required? The design lists "another OM message" as a source but defines no confirmation or trust UI. | Slice 5 and 6 UX. | Operator or UX delegation |
| Q10 | Where do GIF-like loops from outside OM come from (the design allows import and normalization of GIF, APNG, AVIF, WebM)? Normalization requires an encoder; the design says conversion tools may exist but ships none. | Slice 5 cannot offer "send a GIF" without a source and an encoder. This plan builds none; stickers and loops come from installed packs and local normalization of images only if Q2 selects an encoder. | Operator |
| Q11 | Stickers in groups use ordinary `message.create` there; any per-Room policy to disable expressive media (moderation)? The design is silent beyond ordinary attachment policy. | Group scale (R-113) and decentralized moderation. | Operator or delegation |

## 7. What CI can and cannot prove

| Kind | Proved in CI | Needs a person or later work |
|---|---|---|
| Wire, manifests, signatures | frozen CDDL, positive and negative vectors, two codecs agreeing | independent review of the signature construction |
| Resource ceilings | validator unit and property tests, image-bomb corpus under a memory ceiling | decoder behaviour on every shipped browser and OS |
| Fuzzing | the agreed CPU-hour budget on the real parsers | continuous fuzzing of shipped parsers (pre-release) |
| Atomic install and restart | kill-point tests | none |
| Privacy | request-log assertion of no third-party fetch; metadata trace | none |
| UI | jsdom plus axe, keyboard-only Playwright, Reduced Motion | real-device motion and battery on iOS and Android |

## 8. Stop conditions for every slice

Stop and write the question here if: the design is silent on something the slice must decide and it is not in section 6; a pack or asset path would execute, fetch or render from a URL, locator or catalog; a media profile change would alter fallback validity without a new ID; a security UI surface could be styled by content; a measured wasm delta passes 150 KB raw; a relay-visible signal beyond ordinary deposits would be added; the work needs a file another session has uncommitted changes in (check `git status` first).

## 9. Recommendation

ADR-0250 authorized the scope; Q1 and Q3 are answered by ADR-0251 before slice 0 freezes numbers.
