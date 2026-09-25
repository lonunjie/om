# Review brief: core_kernel independent review (one consolidated pass)

You are the **independent reviewer** under ADR-0117 and ADR-0134. The executor (Claude Opus 5.5) has finished every
repo-local criterion for the `core_kernel` scope. The criteria below are the **only** ones still open, and each needs
a non-Anthropic reviewer. This brief replaces the unfulfilled [`BRIEF-2026-09-24-codex-v-series.md`](BRIEF-2026-09-24-codex-v-series.md) and
[`BRIEF-2026-09-25-gpt-sol-architecture.md`](BRIEF-2026-09-25-gpt-sol-architecture.md): no review file came back for either. Read
[`AGENTS.md`](../../AGENTS.md), [`SNAPSHOT.md`](../../SNAPSHOT.md) and
[`docs/reports/2026-09-26-verification-executor-report.md`](../../docs/reports/2026-09-26-verification-executor-report.md) first.

## Review queue (answer every item; each one maps to a ledger criterion)

| # | Gate / criterion | Artifacts | Question |
|---|---|---|---|
| 1 | V-002 "model reviewed by someone other than its author" | [`verification/tla/AccountControl.tla`](../tla/AccountControl.tla), [`RecoveryObservation.tla`](../tla/RecoveryObservation.tla) (+ configs, `models*.json`), [`verification/bounded/acs_authorization_table.py`](../bounded/acs_authorization_table.py) | Do the models faithfully encode ACCOUNT_CONTINUITY_AND_CONTROL.md §3–§13, the §4.1 table, ADR-0111/0127 and ADR-0131 rule 6? Are the environment assumptions (`OwnerMustReact`, `ControllersReact`) and the T-041/T-090 residuals acceptable? Is the lift-row reading right (see report finding 4)? |
| 2 | V-003 "model reviewed by someone other than its author" | [`verification/tla/OneToOneSessions.tla`](../tla/OneToOneSessions.tla), `verification/prototypes/v003-sessions/` (esp. [`restore_redelivery.py`](../prototypes/v003-sessions/restore_redelivery.py), [`floor.py`](../prototypes/v003-sessions/floor.py)) | Are rules K1–K11 and the ADR-0131 restore semantics (fresh DeviceID, import merges) right and sufficient? Is the history-gap residual acceptable? |
| 3 | V-015 independent_reviewer | [`spec/metadata/w0-metadata-exposure-descriptors.json`](../../spec/metadata/w0-metadata-exposure-descriptors.json), [`verification/bounded/metadata_trace_model.py`](../bounded/metadata_trace_model.py), [`metadata_w0_rules.py`](../bounded/metadata_w0_rules.py) | Do the descriptors under-claim anything a W0 provider role actually sees (especially `ratchet_header_shape`, the key-material device id, co-located Home roles)? |
| 4 | V-016 independent_reviewer | [`verification/bounded/introgate_sybil_model.py`](../bounded/introgate_sybil_model.py), [`abuse_w0_rules.py`](../bounded/abuse_w0_rules.py), [`verification/tla/RoomModeration.tla`](../tla/RoomModeration.tla) | Do the models match INVITES_INTROGATE_AND_SPAM.md, DECENTRALIZED_ABUSE_MODERATION.md and ADR-0122/0128/0132? Is the RFC 9578 type-2 verification model (no blinding) adequate? |
| 5 | V-017 independent_reviewer | [`tools/continuity_bundle.py`](../../tools/continuity_bundle.py), [`docs/process/CONTINUITY_TABLETOP_DRILLS.md`](../../docs/process/CONTINUITY_TABLETOP_DRILLS.md) | Is the continuity bundle complete, and are the drills realistic? |
| 6 | Construction ADR (makes V-001/V-005 evidence meaningful) | [`decisions/ADR-0135-om-crypto-v1-construction-pinning.md`](../../decisions/ADR-0135-om-crypto-v1-construction-pinning.md), `verification/prototypes/v001-crypto/` | Does §1–§5 implement ADR-0131 rules 1–3 and 9 without weakening PQXDH's security argument? §2 replaces XEdDSA with a separately certified composite key. Is the composite signature layout (one entry, both halves required) sound? |
| 7 | Claude-authored design since 2026-09-24 | ADR-0110..ADR-0130, ADR-0135, spec/cddl + registries, the V-series prototypes | The unfulfilled 2026-09-24/25 briefs, merged: security flaws, authority leaks ("Where did authority go?"), contradictions, and rulings two honest implementations would read differently. |

V-001 and V-005 are already `passed` (their independence is upstream KATs / a second implementation, not review). A
finding against ADR-0135 or the V-005 codecs reopens them.

## Out of scope for this pass

Pre-release criteria, V-018 (human study), and later-wave gates (V-006, V-007, V-010, V-012, V-020–V-023). They are not
`core_kernel` blockers.

## How to return results

- Add **only** new files under `verification/reviews/`: `2026-09-2X-<reviewer>-core-kernel.md`.
- Header: reviewer, exact model id, date, the base commit from `OM_EXPORT.json`, and scope.
- For each queue item 1–7: a verdict (`accept`, `accept-with-findings` or `reject`) and findings as
  `### F-<n> — <title>` with `Severity:` blocker|major|minor, `Where:`, `Problem:`, `Proposed fix:` and an empty
  `Resolution:` line.
- Return the whole repository as a zip with the same layout (leave `OM_EXPORT.json` unchanged). Do not edit other
  files: the executor applies accepted findings. That is the only return trip this process allows.
