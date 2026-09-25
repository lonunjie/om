# ADR-0116 — Verification runs in CI with pinned tools

- **Status:** Accepted
- **Date:** 2026-09-24
- **Decided by:** operator ("put a JDK and pinned TLC in verification CI")

## Decision

`.github/workflows/verify.yml` runs, on every push and pull request, the same checks as `make verify`: the
architecture validator, the docs link check, all executable evidence ([`verification/run_phase1.py`](../verification/run_phase1.py)) and TLC on every
TLA+ model. TLC runs through `tools/tlc.sh`, which refuses any `tla2tools.jar` whose SHA-1 differs from the pin in
[`verification/TLAPLUS_TOOL_PIN.md`](../verification/TLAPLUS_TOOL_PIN.md). Toolchains are pinned where CI installs them (JDK major version, Rust toolchain
file, Node major version). Evidence that CI regenerates must match the committed results, or CI fails, so recorded
evidence cannot silently drift from what the tools produce.

## Amendment (2026-09-25)

Decided by operator ("restrict CI"). CI no longer runs on every push and pull request: each run takes about 20 minutes
against the free plan's 2,000 private-repo Actions minutes per month, and one day of design commits consumed about 600.
The workflow now runs only on `checkpoint/*` tag pushes and on manual dispatch. Local `make verify` is the per-commit
check; the pinned tools, the checks run and the evidence-drift rule are unchanged.
