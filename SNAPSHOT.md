# OM Architecture Snapshot

Current state as of **2026-11-22**. The dated log that used to live in this file (through 2026-11-20) is kept unchanged in
[`docs/checkpoints/SNAPSHOT_HISTORY.md`](docs/checkpoints/SNAPSHOT_HISTORY.md); the per-change record is [`CHANGELOG.md`](CHANGELOG.md), the
ADRs in [`decisions/INDEX.md`](decisions/INDEX.md) and git. Update this file when the state below changes, not once per commit.

## State

- **Phase:** scoped engineering (`scoped_engineering_w0`). Engineering is authorized for eleven of twelve scopes (operator instruction 2026-11-23 for the last five: ADR-0256 to ADR-0260; `communities_publications` is held back because the `publication` conformance suite is only planned, ADR-0130 guard): `core_kernel` (ADR-0169), `provider_reference` (ADR-0171), `reference_client_ui` (ADR-0172), `groups` (ADR-0177), `calls` (ADR-0217), `expressive_media` (ADR-0250), `portability` (ADR-0256), `local_delivery` (ADR-0257), `services_bots` (ADR-0258), `plugins` (ADR-0259) and `bridges` (ADR-0260). Each authorization waives its gate's pre-engineering phase for engineering only; the evidence stays a release obligation and every later-wave feature stays behind a capability flag that is off in every release. [`machine/repository_contract.json`](machine/repository_contract.json) is the authority.
- **Design:** `design_open=0`. The design and the constitution are complete and were audited; agents implement them and do not amend
  them (see the design gate in [`AGENTS.md`](AGENTS.md)). Deviations of shipped code from the design are listed in the D-tables of the
  status reports, each citing the passage it departs from.
- **Review model:** reviews were performed by AI models at the operator's direction (ADR-0167). Gates passed this way carry
  `pre_engineering_review_basis=operator_single_vendor_ADR-0167`, which is not vendor-independent review. No human cryptographer
  has audited OM.
- **Verification gates:** all 24 gates (V-001..V-024) are `open` overall. V-007 (calls) and V-012 (expressive media) have
  pre-engineering `waived_by_adr` for engineering only, with release obligations still open. Six gates still have pre-engineering open
  (V-008, V-010, V-011, V-013, V-014, V-024); each blocks an unauthorized scope. Ledger entries are never flipped without the
  independence the gate asks for. Evidence index: [`2026-11-22-evidence-index.md`](docs/reports/2026-11-22-evidence-index.md).
- **Public release gates:** PR-001, PR-005, PR-006, PR-012 and PR-014 are `waived_by_adr` (ADR-0183) for the open beta; PR-002, 003,
  004, 007, 008, 009, 010, 011 and 013 are open and bind only a release that makes the claim they guard. The beta makes none of
  those claims.
- **Releases:** one declared release, `om-open-beta-1` (`authorized`, scopes `core_kernel`, `provider_reference`,
  `reference_client_ui`, no claims). The live web build at <https://om.lonunjie.org> reports version 0.0.12 (read from its
  `bundle-manifest.json` on 2026-11-22). Groups, calls and expressive media are in no release declaration.
- **Ledgers (2026-11-22):** 433 requirements, 176 threats, 234 decisions, 255 ADR files.

## What is live and what is not

| Area | State |
|---|---|
| Live (0.0.12, 2026-11-04) | One-to-one chats with a per-person delivery service, device linking, password-sealed backup file, attachments, voice notes, location cards, local search, app lock |
| Built on `main`, not in a release | Rooms (groups, signed lazy module), one-to-one calls (off by build flag), stickers and custom reactions (off by build flag), closed-app alerts, owner recovery flows, reply, reactions, edit and delete, polls, scheduled send |
| Not built | Group calls, typing indicators, discovery by phone number or username, a second delivery-service implementation |
| Never run on real devices | Push delivery, calls, stickers; the V-006 device matrix on the shipped client |

The plain-language version is [`SUPPORT_MATRIX_AND_FAQ.md`](docs/process/SUPPORT_MATRIX_AND_FAQ.md) (draft).

## Where to look

| For | Read |
|---|---|
| What an engineer starts from | [`ENGINEER_START_HERE.md`](docs/process/ENGINEER_START_HERE.md) |
| Core status | [`2026-10-28-core-kernel-m1-status.md`](docs/reports/2026-10-28-core-kernel-m1-status.md) |
| Home and deviations D1 to D13 | [`2026-10-28-relay-status.md`](docs/reports/2026-10-28-relay-status.md) |
| Calls and V-007 | [`2026-11-18-calls-build-plan.md`](docs/reports/2026-11-18-calls-build-plan.md), [`2026-11-19-calls-v007-evidence.md`](docs/reports/2026-11-19-calls-v007-evidence.md) |
| Expressive media and V-012 | [`2026-11-20-expressive-media-build-plan.md`](docs/reports/2026-11-20-expressive-media-build-plan.md), [`2026-11-20-expressive-slice7.md`](docs/reports/2026-11-20-expressive-slice7.md), [`2026-11-21-recovery-parser-fuzz.md`](docs/reports/2026-11-21-recovery-parser-fuzz.md) |
| Beta state and what the operator still owns | [`2026-10-29-open-beta-checklist.md`](docs/reports/2026-10-29-open-beta-checklist.md) |
| Last audit of drift from the design | [`2026-11-03-return-to-the-audited-design.md`](docs/reports/2026-11-03-return-to-the-audited-design.md) |

## Next work

Real-device checks (push, calls, stickers, the V-006 matrix), the first person-run setup with the Cloudflare and GitHub accounts, an
independent review of the open gates, and a release declaration for groups, calls and expressive media (operator decision) are the
steps between the current `main` and a release that offers them. Group calls, typing indicators, discovery and the unauthorized
scopes wait on design or scope decisions.
