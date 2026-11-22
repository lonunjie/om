# Evidence index: V-007, V-012 and the public release gates

**Date:** 2026-11-22. This is an index, not a verdict. It adds no evidence and flips no ledger entry: the gate ledgers
([`architecture_verification_gates.jsonl`](../../machine/architecture_verification_gates.jsonl), [`public_release_gates.jsonl`](../../machine/public_release_gates.jsonl))
are unchanged, and every row below restates what the linked report says. Status words: **met**, **partly met**, **open**. A gate's
"independent" requirements (independent reviewer, tool or implementation) are separate from criteria and none is recorded as
satisfied for V-007 or V-012. Every review to date was performed by AI models (ADR-0167); no human cryptographer has audited OM.

## Design sources read

This pass read the gate entries in [`GATE_PASS_CRITERIA.md`](../../verification/GATE_PASS_CRITERIA.md) for V-007 and V-012, the public release gate
ledger, the V-007 evidence report, the expressive slice 7 report with its addenda, the recovery-parser fuzz report, the beta gate
readiness report and the open beta checklist. It did not read the architecture documents of the calls or expressive-media domains again,
and it re-ran no tests: all results are quoted from the linked reports and result files.

## V-007 `sframe_interop` (calls)

Ledger: overall `open`; pre-engineering `waived_by_adr` for engineering only (ADR-0217); independence required: pre-engineering
independent reviewer, pre-release independent implementation. Detail and manual device matrix:
[`2026-11-19-calls-v007-evidence.md`](2026-11-19-calls-v007-evidence.md). Build plan: [`2026-11-18-calls-build-plan.md`](2026-11-18-calls-build-plan.md).

| Criterion | State | Where the evidence is |
|---|---|---|
| Pre-engineering 1: SFrame key schedule, counters and replay across two browser engines and one native runtime through an SFU, with reconnect and membership rekey | Partly met | One-implementation tests (RFC 9605 vector, replay, counters) and a Chromium two-party run with ICE restart. Second engine, native runtime, SFU and real membership rekey are open. [Evidence report](2026-11-19-calls-v007-evidence.md); [SFrame tests](../../packages/om-web-platform/src/calls/sframe.test.ts); [Chromium spec](../../apps/web/e2e/calls.spec.ts) |
| Pre-engineering 2: no ringing, ICE or TURN allocation for callers without an accepted relationship; relay-only toward non-contacts (R-234) | Partly met | Core admission checks and a disallowed-contact browser trace; the non-contact receive path end to end and the non-contact mesh trace are open. Same report |
| Pre-engineering 3: a Room member who is not joined cannot decrypt media from the Room epoch alone (R-283) | Open | Only a fail-closed framing test exists; no Room key distribution or SFU. Same report |
| Pre-engineering 4: wire freeze and specialist-reviewed key delivery to joined devices (R-343) | Partly met | One-to-one wire frozen ([ADR-0218](../../decisions/ADR-0218-calls-wire-freeze-and-delegated-constructions.md)). Group call bodies, key delivery without a one-to-one relationship, and the specialist review are open |
| Pre-release: full claimed matrix | Open | Manual device matrix all "Pending" (iOS Home Screen, Android Chrome, desktop Firefox and WebKit, SFU and native). No production relay for call traffic |
| Independence (reviewer; implementation) | Open | Not satisfied |

Calls are off in the public build and are in no release declaration.

## V-012 `expressive_media_and_pack_parser_safety`

Ledger: overall `open`; pre-engineering `waived_by_adr` for engineering only (ADR-0250); independence required: pre-engineering independent
tool and independent implementation, pre-release independent tool. Main report: [`2026-11-20-expressive-slice7.md`](2026-11-20-expressive-slice7.md)
(criteria table and addenda); plan: [`2026-11-20-expressive-media-build-plan.md`](2026-11-20-expressive-media-build-plan.md).

| Criterion | State | Where the evidence is |
|---|---|---|
| Pre-engineering 1: decoders and pack parsers fuzzed for the agreed CPU-hour budget, no unresolved crash | Partly met | 120.2 CPU-hours coverage-guided libFuzzer under a sanitizer, no finding ([summary](../../verification/results/expressive-fuzz-latest.json)); a second fuzzer, AFL++, 30.2 CPU-hours, no crash or hang, no sanitizer ([summary](../../verification/results/expressive-fuzz-afl-latest.json)); a sanitized rebuild, 12.1 CPU-hours, no finding ([summary](../../verification/results/expressive-fuzz-sanitized-latest.json)); a browser-path fuzz, 20,000 iterations per probe, no failure, no pixels decoded ([summary](../../verification/results/expressive-fuzz-browser-latest.json)). Each is its own account and they are not added together. The same method was applied to the recovery and link-ceremony parsers ([report](2026-11-21-recovery-parser-fuzz.md), [summary](../../verification/results/recovery-fuzz-latest.json)); that is extra evidence outside V-012's criteria |
| Pre-engineering 2: canonical ContentPack manifest and signature vectors agree across two implementations | Partly met | [Vectors](../../verification/vectors/expressive-wire-vectors.json.gz) replayed by the product codec and an independently written Node harness ([method](../../verification/prototypes/expressive-wire/run.sh)); the full script was not re-run in the latest pass |
| Pre-engineering 3: wire freeze for inline type 4, ReactionSet key 3 and `ExpressiveAssetRef` | Partly met | [CDDL](../../spec/cddl/expressive.cddl) and [ADR-0252](../../decisions/ADR-0252-expressive-media-wire-freeze-and-delegated-constructions.md); two codecs agree per the slice reports. Independence untouched |
| Pre-release: shipped parsers under continuous fuzzing | Open | None configured; the runner and targets are the starting point |
| Independence (independent tool and implementation) | Open | Not satisfied |

Expressive media is off in the public build and is in no release declaration. Metadata trace of the expressive portion: slice 7 report,
"Metadata trace"; real-build captures (browser, native, Homes, push) are open.

## Public release gates (PR-001 to PR-014)

Sources: the ledger and ADR-0183 ([open beta waivers](../../decisions/ADR-0183-open-beta-release-waivers.md)); the per-gate analysis is
[`2026-10-28-beta-gate-readiness.md`](2026-10-28-beta-gate-readiness.md) and the beta state is
[`2026-10-29-open-beta-checklist.md`](2026-10-29-open-beta-checklist.md). The readiness report is dated 2026-10-28 and some of its "today" statements
were overtaken (the checklist records what landed on 2026-10-29); the state column below follows the checklist and ledger where they differ.
"Waived" means the beta ships without the criteria under ADR-0183; it does not mean they are met.

| Gate | Ledger | Binds | Criteria state | Notes and links |
|---|---|---|---|---|
| PR-001 jurisdiction and role review | `waived_by_adr` | The open beta | Open | No role-specific review exists. A draft privacy notice exists: [`BETA_PRIVACY_NOTICE.md`](../process/BETA_PRIVACY_NOTICE.md) (not reviewed by counsel). Triggers for counsel: ADR-0183 section 2 |
| PR-002 patent text | `open` | Only if normative outside contributions are invited | Open | Counsel-reviewed text not written |
| PR-003 trademark and compatibility policy | `open` | Only if "official" status is claimed at scale | Open | Not written |
| PR-004 store compliance | `open` | A store listing | Open | No store distribution (ADR-0178) |
| PR-005 software supply chain | `waived_by_adr` | PWA origin, deploy template | Partly met | Met or partly: pinned roots with delegation and expiry, custody statement, provenance and SBOM, rebuild and compare procedure, single-operator digest log ([`deploy/REBUILD-AND-COMPARE.md`](../../deploy/REBUILD-AND-COMPARE.md), checklist B3, B4). Open: independent source and release mirrors, independent release-transparency monitors, independent review |
| PR-006 push platform matrix | `waived_by_adr` | Scope `reference_client_ui` | Open | Waived for "no background delivery claim". Push is built on `main` and not released; no real-device run ([V-007 report](2026-11-19-calls-v007-evidence.md) pending rows, [V-006 kit](2026-10-28-v006-device-checks.md)) |
| PR-007 link previews | `open` | Only if previews are claimed | Open | No preview feature; do not claim it |
| PR-008 LAN matrix | `open` | Only if a LAN mode is claimed | Open | `local_delivery` scope is not authorized |
| PR-009 provider diversity evidence | `open` | Only if claimed | Open | Do not claim |
| PR-010 evidence export profile | `open` | Only if claimed | Open | Do not claim |
| PR-011 key transparency | `open` | Only if advertised | Open | Do not advertise |
| PR-012 disclosure program | `waived_by_adr` | The open beta | Partly met | Contact ([`SECURITY.md`](../../SECURITY.md), `security.txt`), embargo policy and implementer list file landed 2026-10-29 (checklist B1, B2). The signed advisory channel and an independent review were open at the last reading |
| PR-013 provider monoculture limit | `open` | Resilience or censorship-resistance claims | Open | A second Home with no cloud-vendor dependency is not built; the SQLite Home work is a step toward it. Make no resilience claim |
| PR-014 illegal-content process | `waived_by_adr` | Provider-readable public surfaces | Open | Waived for the beta; no such surface is offered |

## Other release gates that bind the current beta

| Gate | State | Where |
|---|---|---|
| V-006 web device vault, pre-release | Partly met | Probe passed on five browsers, not on the shipped client; real-device run and Windows are open ([checklist](2026-10-28-v006-device-checks.md), [readiness](2026-10-28-beta-gate-readiness.md)) |
| V-018 participant study, pre-release | Open | Kit exists, no results ([kit](2026-10-28-v018-participant-study-kit.md)) |

## What this index does not cover

V-004, V-009 and V-019 (groups) are recorded in the checkpoint history ([`SNAPSHOT_HISTORY.md`](../checkpoints/SNAPSHOT_HISTORY.md)) and their pre-release items
are not indexed here. The six gates with pre-engineering still open (V-008, V-010, V-011, V-013, V-014, V-024) each block an unauthorized scope.
