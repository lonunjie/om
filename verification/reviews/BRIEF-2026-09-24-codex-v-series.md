# Review brief: V-series models and prototypes (for OpenAI Codex)

You are the independent reviewer under ADR-0117 for artifacts written by Claude agents. Read [`AGENTS.md`](../../AGENTS.md),
[`SNAPSHOT.md`](../../SNAPSHOT.md), ADR-0109, ADR-0117 and ADR-0120..0125 first.

## What to review

| Gate | Artifacts | Criterion to judge |
|---|---|---|
| V-003 | `verification/tla/OneToOneSessions*.{tla,cfg}`, `verification/prototypes/v003-sessions/` | Does the model faithfully cover crossed initiation, add/revoke races, stale/reset sessions, dedupe and persistence? Are rules K1–K11 (ADR-0120) the right ones? |
| V-005 | `spec/cddl/`, `spec/registries/`, `verification/prototypes/v005-encoding/`, [`verification/cbor/node_strict_harness.mjs`](../cbor/node_strict_harness.mjs) | Are the Rust and Node codecs really independent, and correct against the CDDL and OM-CBOR-v1? Are the 540 vectors adequate? |
| V-008 | `verification/tla/ScopedPermissions*`, `verification/prototypes/v008-permissions/` | No self-escalation or transitive-authority counterexample within bounds; bounds adequate? |
| V-009 | `verification/prototypes/v009-materializer/`, [`verification/bounded/conversation_convergence.py`](../bounded/conversation_convergence.py) | Do the two materializers agree for the right reasons? Is the shared authorization adapter hiding disagreement? |
| V-011 | `verification/tla/ServiceAuthority*`, `verification/prototypes/v011-services/` | Host migration, revocation, alias replacement; prompt-injection and loop bounds |
| V-013 | `verification/prototypes/v013-bridges/` incl. [`BRIDGE_ADAPTER_CONTRACT.md`](../prototypes/v013-bridges/BRIDGE_ADAPTER_CONTRACT.md) | Is the contract complete and consistent with ADR-0124? Is the loop model adequate? |
| V-014 | `verification/prototypes/v014-archive/` incl. [`OM_ARCHIVE_V1_SPEC.md`](../prototypes/v014-archive/OM_ARCHIVE_V1_SPEC.md) | Is the spec ready to freeze given ADR-0125? Is the hostile corpus adequate? |
| V-016 | `verification/tla/RoomModeration*`, `verification/prototypes/v016-room-moderation/` | Timeout/remove/ban races and the evidence model |

For each gate, look for: the model not matching the docs, invariants that are too weak or vacuous, bounds too small
to find the bugs that matter, fault-injection mutations that don't test what they claim, vectors that don't replay,
and design rules (ADR-0120..0125) that are wrong or incomplete.

Run each `verification/prototypes/<slug>/run.sh` if your environment allows. They need Java 21, Rust (pinned
toolchain), `uv`/Python 3.14, Node and the Ruby `cddl` gem. Say which ones you could not run.

## How to return results

- Add **only** new files under `verification/reviews/`, one per gate:
  `2026-09-24-codex-<gate>.md`. Don't edit any other file; propose changes inside your review instead.
- Each file starts with: reviewer (Codex), exact model id, date, and the commit in `OM_EXPORT.json`.
- Then a verdict: `pass`, `pass with findings`, or `fail`, for the gate's review criterion.
- Then the findings, one per heading: `F-<gate>-<n>`, severity (`blocker`, `major`, `minor`), the file and line,
  the problem, and a proposed fix. Leave a `Resolution:` line empty; the integrator fills it in.
- Return the whole repository as a zip with the same layout.
