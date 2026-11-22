# Release declaration proposal: groups, calls and expressive media (draft, not decided)

**Status: a draft for the operator, 2026-11-22. Nothing here is decided and nothing here edits a ledger.** It lists, for each of the three
features, what a release declaration would need to say and what evidence each statement needs. The authority is
[`machine/repository_contract.json`](../../machine/repository_contract.json); its `public_releases` entry `om-open-beta-1` is unchanged by this
document and still lists the scopes `core_kernel`, `provider_reference` and `reference_client_ui` and no claims. Whether to add a scope or a
claim, to open a new release entry, or to ship the features switched off is the operator's decision (ADR-0130 rule 7: a release with status
`authorized` or `published` needs every applicable gate `passed` or `waived_by_adr`).

## Design sources read

The gate entries of V-004, V-007, V-009, V-012 and V-019 in [`architecture_verification_gates.jsonl`](../../machine/architecture_verification_gates.jsonl),
the release obligations in [ADR-0177](../../decisions/ADR-0177-authorize-groups-engineering.md), [ADR-0217](../../decisions/ADR-0217-authorize-calls-engineering.md)
and [ADR-0250](../../decisions/ADR-0250-authorize-expressive-media-engineering.md), the public-release vocabulary in the contract,
[`SECURITY_RELEASE_GATES.md`](../process/SECURITY_RELEASE_GATES.md), the [evidence index](../reports/2026-11-22-evidence-index.md) and
[`vite.config.ts`](../../apps/web/vite.config.ts) for how each feature is switched. Not re-read for this draft: the calls, groups and
expressive architecture documents, and the ADR-0183 waiver text (quoted here only through the evidence index). No test was re-run; results
are quoted from the linked reports.

## What a declaration is, in this repository

A release declaration is an entry in `public_releases` with an `id`, a `status` (`planned`, `authorized`, `published`), the engineering `scopes`
it offers, a `distribution_kind`, a list of `claims` drawn from the fixed vocabulary, and an optional waiver ADR. The vocabulary has no claim
named "groups", "calls" or "expressive media": those are **scopes** (`groups`, `calls`, `expressive_media`), and each scope's own ADR says what
must hold before a release offers it. The ten claims (`background_delivery`, `link_previews`, `provider_diversity`, `censorship_resilience` and
so on) are separate: claim only what is true, and for each claim the matching PR gate must pass or be waived.

## Option summary

| Option | What ships | What the declaration says |
|---|---|---|
| A. Keep the beta declaration | A new web build with the three features off, or groups reachable as described below | `om-open-beta-1` unchanged, no new scope. Nothing new may be advertised; the support matrix labels apply |
| B. Add `groups` | Rooms offered to everyone | `groups` added to the scopes, after the groups evidence below |
| C. Add `calls` | One-to-one calls offered | `calls` added, after the V-007 evidence below. No group calls, no claim of call security beyond what is evidenced |
| D. Add `expressive_media` | Stickers, custom reactions, pack files | `expressive_media` added, after the V-012 evidence below |
| E. Alerts claim | Push alerts when OM is closed | The `background_delivery` claim, only if the PR-006 push platform matrix is passed or waived for that wording; otherwise the copy stays "best effort" and no claim is declared |

These can be taken one at a time; nothing requires all three together.

## Groups (scope `groups`, ADR-0177)

**State on `main`.** Rooms are built (up to 32 people) and, since ADR-0187, the groups module ships in every build as a second signed asset
that is fetched, checked and loaded only when a person first uses groups (there is no build flag for it any more). So a web release built
from `main` would put Rooms in front of every user unless the release removes the entry points. Decide this before building, not after: the
build in the rollout kit is the beta build plus whatever `main` contains.

| Needed for the claim | Where it stands | Evidence |
|---|---|---|
| V-004 pre-release: multi-client partition and crash integration harness passing on two implementations or two runtimes | Open. Pre-engineering criteria 8/8 (model, ordering probe) on 2026-10-01, reviewed on the operator's single-vendor basis (ADR-0167) | Gate entry V-004; its pre-release criterion is not recorded as met |
| V-009 pre-release: every conforming implementation passes the materialization corpus | Open. Pre-engineering 5/5 including a second independently written materializer | Gate entry V-009 |
| V-019 pre-release: interoperable authenticated migration on two implementations, once the ADR-0107 triggers are met | Open; triggers not asserted here | Gate entry V-019, [ADR-0175](../../decisions/ADR-0175-v019-group-migration-is-an-om-level-authenticated-transition.md) |
| No claim of post-quantum confidentiality for v1 groups | A wording rule, not a test (ADR-0177 point 4) | Check every user-facing sentence |
| Cross-vendor re-check of the review basis | Open (ADR-0177 point 1 keeps it a pre-release item) | None; needs an independent reviewer |
| Product evidence | Automated groups end-to-end runs on Chromium, Firefox and WebKit, 48 of 48 on the last full run; larger-Room behaviour checked to the 32 cap | [`SNAPSHOT.md`](../../SNAPSHOT.md); no real-device run exists |

The three pre-release items are not met by automated tests alone: each names a second implementation or an independent reviewer. If the
operator wants to ship Rooms before they are met, that is a waiver by ADR, and an ADR by delegation cannot amend audited text; it would
record a deviation citing the passage it departs from.

## Calls (scope `calls`, ADR-0217)

**State on `main`.** One-to-one calls are built behind the build setting `VITE_OM_CALLS`, which `vite.config.ts` refuses in a production
build (R-072). Offering calls therefore needs a deliberate, reviewed change to that rule, not just a declaration. Group calls are not built.

| Needed for the claim (ADR-0217 point 4) | Where it stands | Evidence |
|---|---|---|
| SFrame key schedule, counters and replay interoperating across two browser engines and one native runtime through an SFU, with reconnect and membership rekey | Partly met: one implementation, an RFC 9605 vector, a Chromium two-party run with ICE restart. Second engine, native runtime, SFU and membership rekey are open | [V-007 evidence report](../reports/2026-11-19-calls-v007-evidence.md) |
| No ringing, ICE or TURN allocation toward a caller without an accepted relationship; relay-only toward non-contacts (R-234) | Partly met: admission checks and a disallowed-contact trace; the non-contact receive path and mesh trace are open | Same report |
| Silent-listener trace (R-283) | Open: a fail-closed framing test only | Same report |
| Specialist-reviewed key delivery for joined participants without a one-to-one relationship (R-343) | Required only before group calls ship; not needed for one-to-one | Same report |
| The full claimed manual matrix (iOS Home Screen, Android Chrome, desktop Firefox and WebKit) | Every row "Pending" | Same report |
| Independent reviewer and independent implementation | Not satisfied | Gate entry V-007 |
| A production relay for call traffic | None exists. Calls need a TURN lookup backend; the contract is in the calls build plan | [Calls build plan](../reports/2026-11-18-calls-build-plan.md) |

Honest wording that is already in the product: call screens say "Open OM to answer before the call ends" and do not promise a ring while OM is
closed. Any release that offers calls should say in the support matrix that calls have not been tried on real phones unless that run exists.

## Expressive media (scope `expressive_media`, ADR-0250)

**State on `main`.** Stickers, custom reactions and `.ompack` files are built behind `VITE_OM_EXPRESSIVE_MEDIA`, which `vite.config.ts` also
refuses in a production build, and which produces a separate wasm artifact (its own dist, with its own size accounting). Offering it needs the
same kind of deliberate change as calls.

| Needed for the claim (ADR-0250 point 4) | Where it stands | Evidence |
|---|---|---|
| The agreed fuzz budget completed on the shipped decoders and parsers with no unresolved crash, with an independent tool | Partly met: 120.2 CPU-hours libFuzzer under a sanitizer, 30.2 CPU-hours AFL++, a 12.1 CPU-hour sanitized rebuild, a browser-path fuzz; no finding. The independent-tool requirement is not recorded as satisfied | [Evidence index](../reports/2026-11-22-evidence-index.md), [slice 7 report](../reports/2026-11-20-expressive-slice7.md) |
| Manifest and signature vectors agreeing across two independently written implementations | Partly met (product codec and a Node harness); the full script was not re-run in the latest pass | Same |
| The frozen wire with two-codec vectors | Partly met | [ADR-0252](../../decisions/ADR-0252-expressive-media-wire-freeze-and-delegated-constructions.md) |
| Shipped parsers under continuous fuzzing | Open: none configured | Evidence index |
| Specialist review of the threshold construction | Open (release obligation) | None yet; needs a specialist |
| Authenticated capability claim for linked devices | An open design question (it touches the frozen link ceremony and needs an additive ADR). The safe path in force is the W0 fallback: if any member device lacks a verified capability, the whole send falls back to plain content | Open design question; no ruling yet |
| Native decoder behaviour and the 64 MiB measurement | Open | Slice reports |

## Claims that stay undeclared in every option

`public_production_resilience`, `censorship_resilience`, `provider_diversity`, `evidence_export_interop`, `key_transparency`, `link_previews`,
`lan_local_mode`, `official_compatibility` and `external_normative_contributions` have no supporting evidence and should not appear. The
support matrix ([`SUPPORT_MATRIX_AND_FAQ.md`](../process/SUPPORT_MATRIX_AND_FAQ.md)) already says so in user language.

## Decisions this proposal leaves to the operator

1. Which of A to E to take, and in what order.
2. For groups: whether a release hides the entry points until the pre-release items are met, or declares `groups` with a waiver.
3. For calls and expressive media: whether to change the production-build refusal of their flags, and when (after the real-device runs).
4. Whether the first release after 0.0.12 is named a beta continuation (same declaration) or a new declared release.
5. Who the independent reviewer is, and when the specialist reviews are booked; none of the three scopes can honestly claim more than the
   operator's single-vendor review basis until then.

## What this proposal did not do

It did not edit [`machine/repository_contract.json`](../../machine/repository_contract.json), any gate ledger, any ADR, [`SNAPSHOT.md`](../../SNAPSHOT.md), or the support matrix.
