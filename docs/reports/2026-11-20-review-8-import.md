# Independent review 8: specialist rulings imported as ADR-0244 to ADR-0249

**Date:** 2026-11-20 (PHT). **Input:** the reviewer's findings summary and six specialist ADR drafts. **Disposition:** not approved for release, enablement, restoration or product-code integration. Local working record, non-normative.

## Design sources read

The reviewer read [`AGENTS.md`](../../AGENTS.md), the constitution digest (C-14, C-20 to C-29, C-40 to C-50), ADR-0216, ADR-0225 to ADR-0227, ADR-0236 to ADR-0243, relevant parts of ADR-0199, ADR-0200, ADR-0203 and ADR-0230, [`PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md), the account, controller-change and recovery-record CDDL, [`recovery-sheet.md`](../../spec/protocol/recovery-sheet.md), type 38 and 39 and the registry rules of [`om-object-types.json`](../../spec/registries/om-object-types.json), parts of the metadata threat model and the account continuity document, and targeted Core paths (`control.rs`, `recovery_record.rs`, `recovery_sheet.rs`) and the v3 golden generators.

Not read or audited comprehensively by the reviewer: the full constitution originals, the whole design-gate list, all registries, side-channel and implementation review of the hybrid signatures, Home and transport conformance, UI and UX, machine contracts and formal models. This import read the drafts, the findings and the ADRs it edits, and checked statuses and pointers only; the full mapped design gate is still owed.

## Findings and rulings

| Question | Ruling | Status |
|---|---|---|
| ADR-0240 / CC3B-03 | ADR-0244: a durable P-only recipient needs no retained Q signer-to-digest summary; wrong historical hash and same-DeviceID collapse are rejectable only in predecessor-reverification mode or with a retained verified Q-to-P receipt. Vectors CC3C-01 to 04. | Accepted, gated |
| ADR-0240 / recovery-signed P | ADR-0245: a recovery-role signature is verified against Q's recovery method, not a DeviceCredential; optional private proposal key 11 carries only missing bytes. Vectors CC3D-01 to 04. | Accepted, gated |
| ADR-0241 / KITS3-03, 04 | ADR-0246: type 38 stays at 4,096; preferred route is a new type 47 with 8,192; any change to the frozen row is an audited-text change. Vectors KITS4-01 to 04. | Proposed |
| ADR-0242 / RK2B-01 | ADR-0247: a new v3 sheet custodies a digest of the exact signed envelope; v2 meaning unchanged. Vectors RK2D-01 to 04. | Accepted, gated |
| ADR-0242 / RK2B-02 | ADR-0248: explicit v2 record lineage with `acs_evidence[3] = null` iff the head has no recovery commitment. Vectors RK2E-01 to 04. | Accepted, gated |
| ADR-0242 / RK2B-03, ADR-0243 | ADR-0249: the public request-digest extension of ADR-0243 is rejected; sheet-only classification of ambiguous historical recovery links is permanently `needs_evidence`. Vectors RK2F-01 to 04. | Accepted; ADR-0243 rejected |

## Gates (all accepted-gated items)

Two independent codecs with real hybrid-signature goldens for the named vectors, CDDL and registry rows added in the same change, and cryptographic and metadata specialist sign-off. ADR-0246 additionally needs an explicit operator and audit-owner decision; `spec/registries` and CDDL were not edited in this import. Metadata notes: op 3 already exposes a candidate-hash QR correlator (ADR-0236); a larger kit may change traffic buckets; historical policies increase private recipient disclosure; the v3 sheet digest is owner-held only.

## Product-code integration blockers (from the findings)

Product code for ADR-0240 to ADR-0242 behavior must not be integrated until all of these hold:

1. ADR-0244 and ADR-0245 are settled in the op-3 schema (anchored-P versus predecessor reverification, recovery-role signer); no vote on an unanchored P or a borrowed recovery timer; the ADR-0236 QR candidate-hash exposure is reviewed; the ADR-0216 and ADR-0227 fork, quorum, inbox and exclusion prerequisites are met.
2. The type-38 4,096 conflict is resolved by an approved amendment or the type-47 lineage, applied consistently to CDDL, numeric and type registries, decoder, DeviceVault, promotion and sibling paths; envelope fit (framing, padding, real session limits) is verified and the kit-size traffic metadata decision is made.
3. The v3 sheet envelope commitment is specified, registered and independently verified (v2 is not claimed to detect signature-only changes); the policy-absent head is resolved with explicit v2 record semantics and AEAD/version dispatch, or that reader state stays restricted.
4. ADR-0243 stays rejected; positive RK2B-03 and RK2C behavior is removed under current W0 and the sheet-only `needs_evidence` fallback is normative; a root-authenticated dossier is not read as binding a nonce or providing time or veto evidence.
5. All CDDL, registries, labels, signed and hashed-byte vectors, traceability, threat and exposure descriptors, ADR pointers and feature gates change together, with two independent CBOR and hybrid-signature implementations tested for rotation, recovery signer, fork, veto, nonce substitution, crash and replay and every size boundary; full mapped design gate, repository validators, Home, transport and vault integration and specialist sign-off.

No release, security or restoration approval is expressed.
