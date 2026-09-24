# Verification prototypes

Feasibility prototypes that produce gate evidence (see [`../GATE_PASS_CRITERIA.md`](../GATE_PASS_CRITERIA.md)).
**They are not product code.** Each one lives in `verification/prototypes/<slug>/` with a README, a `run.sh`
and a deterministic `verification/results/<slug>-latest.json`. CI runs every `run.sh`.

The rest of this file is the brief given to agents (human or AI) who build a prototype. It is the single source for
these conventions: point an agent at this file instead of pasting it into a prompt.


## Orientation
Read first:
- [`AGENTS.md`](../../AGENTS.md) (Part 2 = architecture rules; they bind you)
- [`verification/GATE_PASS_CRITERIA.md`](../GATE_PASS_CRITERIA.md) (your gates' exact pre-engineering criteria and independence rules)
- [`verification/README.md`](../README.md), [`verification/FEASIBILITY_RECONNAISSANCE_20260923.md`](../FEASIBILITY_RECONNAISSANCE_20260923.md) (prior reconnaissance for your gates)
- the design docs named in your task (find any doc by basename: `git ls-files | grep NAME`)

## Rules
1. Put everything under `verification/prototypes/<your-slug>/` (TLA+ models may go in `verification/tla/<Name>.tla` + `<Name>.cfg`; run them with `tools/tlc.sh verification/tla/<Name>.tla`, which uses the pinned TLC 1.7.4; TLC must terminate, so bound the state space with small constants/constraints; list every config and its expected outcome in `verification/tla/models-<your-slug>.json`, same format as [`models.json`](../tla/models.json), so `make tlc` checks it). Start every prototype README with: "Feasibility prototype for <gates>. Not product code. Not for production."
2. Do NOT edit `machine/`, `docs/`, `decisions/`, `tools/`, `.github/`, or other agents' areas unless your task explicitly allows it. Put any proposed doc/ledger change in your README under "Proposed design updates", and put design bugs or contradictions you found there as well.
3. Never claim a gate passed. Report each criterion as met / partial / unmet with evidence paths. Independence matters. Write implementations from the **public specifications** (cite sections), not by translating existing OM code. State authorship honestly: "written by a Claude agent; the design package was authored by a ChatGPT design agent and Claude agents; ADR-0117 (proposed) decides whether an AI reviewer counts as independent."
4. Provide `verification/prototypes/<slug>/run.sh`. It builds everything, runs all checks, writes **deterministic** results JSON (no timestamps or absolute paths) to `verification/results/<slug>-latest.json`, and exits non-zero on failure. It must finish in under ~10 minutes on a laptop; put long runs such as fuzzing behind a flag like `run.sh --long`. The results JSON has: `gates`, `criteria` (list of {gate, criterion, status: met|partial|unmet, evidence, note}), `independence`, `tool_versions`, `summary`.
5. Keep committed files few and small: no fuzz corpora directories or huge vector dumps (compress or regenerate), no build outputs. `target/`, `node_modules/`, `.venv/` are gitignored.
6. Toolchains on this Mac: Rust via rustup (`export PATH=/opt/homebrew/opt/rustup/bin:$PATH`; stable has wasm32-wasip1 and wasm32-unknown-unknown; `rustup toolchain install nightly` is fine if you need cargo-fuzz), wasmtime 49, Node 24 + pnpm, Python 3.14 + uv (make a uv project in your dir with a lockfile), Go, OpenJDK 21 (`/opt/homebrew/opt/openjdk@21/bin/java`), Homebrew. Network works. Download only from official sources (crates.io, PyPI, npm, the official GitHub org of the project). Pin versions (rust-toolchain.toml, lockfiles).
7. Development/test crypto or RNG must be unmistakably test-only (feature-gated, named `insecure_*`/`test_*`). Never write a novel cryptographic primitive. Implementing a reviewed published protocol (Signal PQXDH/Double Ratchet specs, RFC 9420, RFC 9605, RFC 8949) as a feasibility prototype is allowed.
8. Commit your work on your worktree branch in logical commits, each message ending with a blank line and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Do not push. Do not merge anything.
9. Be pragmatic about scope. If a criterion is infeasible here (it needs human reviewers, real phones or study participants), say exactly what is missing and do the maximum preparatory work instead. Do not stop early on anything that is feasible.

## Token discipline

Read only the docs your gate names. Commit as you go, so an interruption loses little. Do not re-read files you have already read.
Keep the final report short.

## Final report (your last message; under 500 words)
- branch name and commit list
- per-gate criterion table: met / partial / unmet + evidence path + one-line reason
- how to run (`run.sh`), runtime, dependencies installed
- measured performance numbers if relevant (for V-022 budgets in docs/architecture/ux/MOTION_AND_PERFORMANCE.md)
- design bugs/contradictions found and proposed design updates
