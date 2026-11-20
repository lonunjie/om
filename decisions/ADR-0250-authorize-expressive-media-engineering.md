# ADR-0250 — Authorize engineering scope `expressive_media` (W1), by explicit operator instruction

- **Status:** **Proposed**, awaiting the operator's explicit instruction. Authorizing an engineering scope is a decision of the operator, not a review result and not a delegation; nothing in this ADR is applied until the operator accepts it.
- **Date:** 2026-11-20
- **Authorizes (on acceptance):** engineering scope `expressive_media` only. No other scope changes state.
- **Depends on:** `core_kernel` (ADR-0169), already authorized.
- **Waives (on acceptance):** the **pre-engineering** phase of gate V-012 (`expressive_media_and_pack_parser_safety`) for engineering purposes only. Its pre-release phase is untouched and stays open.
- **Plan:** [`2026-11-20-expressive-media-build-plan.md`](../docs/reports/2026-11-20-expressive-media-build-plan.md)

## Context

`expressive_media` (W1) covers stickers, GIF-like looping animation, custom emoji and reactions, and portable content packs. The design is complete and audited: [`EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md`](../docs/architecture/ecosystem/EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md), ADR-0089 (bounded inert media), ADR-0090 (portable inert content packs), ADR-0091 (generic entitlement boundary, commerce external), ADR-0129 rule 10 (ExpressiveAssetRef is an attachment-ref), ADR-0128 rule 18 (later-wave content never breaks W0 receivers), and the logical schemas [`expressive-asset.logical.schema.json`](../spec/protocol/expressive-asset.logical.schema.json) and [`content-pack.logical.schema.json`](../spec/protocol/content-pack.logical.schema.json). The queue items A-001 to A-003 are resolved.

The unlock contract ([`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](../docs/process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md)) otherwise requires every gate that lists the scope to have `pre_engineering_status` passed or waived by an ADR. V-012 is the only gate that lists `expressive_media`. Its three pre-engineering criteria are not met and no evidence is recorded: fuzzing of candidate decoders and pack parsers for an agreed CPU-hour budget under the resource ceilings; canonical ContentPack manifest and signature vectors agreeing across two implementations; and the ADR-0156 wire freeze (frozen CDDL, registered fields and labels, two-codec vectors) for `ExpressiveAssetRef`, inline type 4 `custom_emoji` and `ReactionSet` key 3. No `spec/cddl/` file exists for any of it. The operator may choose to authorize engineering now and keep that evidence as a release obligation. This is the same shape as ADR-0217 (V-007), ADR-0170 (V-018) and ADR-0183.

## Decision (on acceptance)

1. **`expressive_media` is authorized**, by explicit operator instruction. Engineers follow [`ENGINEER_START_HERE.md`](../docs/process/ENGINEER_START_HERE.md), the `expressive_media` entry of [`engineering_scope_contracts.json`](../machine/engineering_scope_contracts.json) and the design gate in [`AGENTS.md`](../AGENTS.md).
2. **What the scope covers.**
   - Sticker, custom-emoji, reaction-asset and loop-animation sending and receiving as ordinary `message.create` with an expressive attachment-ref, `ReactionSet` key 3 and inline type 4, under the `om.expressive.webp-v1` profile and its three resource profiles, with the decoded-pixel and frame budget enforced before decode.
   - Capability-gated emission with a W0-valid fallback (ADR-0128 rule 18); fresh encryption per send, `shared_blob` off by default.
   - Content pack release verification, signed control-state lineage, forks, atomic install and removal, local pack store; direct sharing and local import. Rendering authority stays the digest-bound blob.
   - The shared `om-ui` surfaces (picker, sticker message, reactions, inline emoji, Reduced Motion fallback) and a browser-native decoder driver.
   - **Not covered:** any entitlement-presentation code or proof scheme, any catalog client or service, any marketplace, payment or commerce surface, GIF search, additional media profiles. Each needs its own ADR.
3. **Gate V-012 pre-engineering is waived by this ADR, for engineering only.**
   - *Risk accepted:* expressive-media parsing and pack-handling code, and the wire shapes for the three W1 slots, can be written before the fuzz evidence, the independent signature and manifest interoperation evidence and the independent review exist. A hostile image or pack could reach a parser that has not yet had its agreed fuzz budget, and a wire or signature construction chosen during engineering could be found wrong later.
   - *Compensating controls:* (a) the first build slice freezes the wire (CDDL, registered numeric fields and domain-separation labels, positive and negative vectors, two independent codecs agreeing, including the ContentPack manifest and signature vectors) before any other code, by the V-005 method, additively and under ADR-0165; (b) the second slice ships the header-level profile and budget validator and the fuzz harness before any decoder is reachable, and the fuzz budget is agreed with the operator before slice 0 closes; (c) decoding uses browser or OS facilities only (no custom codec, no new dependency), behind a worker and a measured memory ceiling; (d) every construction the design leaves open is written in an ADR by delegation that does not amend audited text, and listed in the plan; (e) the feature is built behind a capability flag that is off in every release until the release evidence below is in.
4. **Still required BEFORE RELEASE, not before engineering.** Any release that offers expressive media, any release declaring `expressive_media` in `public_releases`, and any claim about parser or pack safety needs V-012 pre-engineering and pre-release evidence in full: the agreed fuzz budget completed on the shipped decoders and parsers with no unresolved crash (independent tool); manifest and signature vectors agreeing across two independently written implementations; the frozen wire with two-codec vectors; and shipped parsers under continuous fuzzing. `om-open-beta-1` does not list `expressive_media` and stays as it is.
5. **Constraints.** No new cryptographic primitive: pack signatures use the existing durable-signature profile. Expressive and pack content is data, never code and never authority; a live catalog, locator or entitlement service never becomes rendering authority; no catalog or marketplace is hosted by the project (C-70). No mandatory or default fetch address is compiled in, and no automatic third-party fetch happens under a privacy profile that forbids it. A signature is never shown as a copyright, safety or "official" claim (R-108). Floors are unchanged (C-20). No design change: where the design is silent the engineer writes the question in the plan's list and stops, or an ADR by delegation records a construction that does not amend audited text.
6. **Where did authority go?** Nowhere new. A sticker or pack is data a person chooses to send or install; a pack's controllers authorize only that pack's updates; a catalog or distributor may index, rank or gate acquisition for itself but has no protocol privilege over identity, conversations or rendering.

## Applied on acceptance

None of this is applied while the ADR is Proposed. On the operator's explicit acceptance:

- [`machine/repository_contract.json`](../machine/repository_contract.json): `engineering_scopes.expressive_media.authorized = true`, `authorizing_adr = "ADR-0250"`.
- [`machine/architecture_verification_gates.jsonl`](../machine/architecture_verification_gates.jsonl): V-012 `pre_engineering_status` and `pre_engineering_independence_status` become `waived_by_adr` with a waiver sentence naming this ADR. Its overall `status` stays `open`.
- [`machine/decisions.jsonl`](../machine/decisions.jsonl) a new D-record (next free D-number); [`INDEX.md`](INDEX.md) status text; [`AGENTS.md`](../AGENTS.md), [`SNAPSHOT.md`](../SNAPSHOT.md), [`ENGINEER_START_HERE.md`](../docs/process/ENGINEER_START_HERE.md) and the unlock table name the sixth authorized scope (generated rows via [`tools/render_gates.py`](../tools/render_gates.py); 6 of 12; the remaining count of gates with pre-engineering open is recomputed by the validator).
- [`docs/reports/2026-11-20-expressive-media-build-plan.md`](../docs/reports/2026-11-20-expressive-media-build-plan.md) status line updated to authorized.

## Consequences

- The review basis becomes the operator's decision, not independent review. The independent-tool and independent-implementation criteria of V-012 move to the release obligations in point 4.
- Slice 0 of the plan is design-bound specification work (wire freeze) and is the only thing that may be built first. Plan questions Q1 (fuzz budget) and Q3 (decoded-pixel and frame budget figures) need the operator's answer before slice 0 freezes numbers.
- If the operator does not accept this ADR, the scope stays locked and nothing in the repository changes.
