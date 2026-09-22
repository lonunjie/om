# OM Agent Instructions

OM — an open-source, operatorless end-to-end-encrypted messenger (Open/Own/Our/One/Oh Messenger) by SeedLife. Architecture/design phase; engineering not yet authorized.

This repo lives at `~/dig/lonunjie/om`, nested inside the SeedLife control
plane (`~/dig/lonunjie`, repo `lonunjie/hq`). Read `../AGENTS.md` first for
org-wide rules, registry, and memory; this file adds only what is specific
to OM. Private on GitHub until the architecture freezes.

## Design status (read before doing anything)

**Architecture hardening is in progress. Engineering is NOT authorized.**
Design-package zips land in `design-drops/` (gitignored). They are working
snapshots of a design the operator is actively changing — use them to
understand what OM is, but do not implement from them, scaffold code from
them, or treat any document in them as settled until the operator
declares the architecture frozen. Summary of what OM is:
`../memory/topics/om-design-phase.md`.

When a freeze is declared, commit the frozen package into this repo as
real files (with `LICENSE.md` and SPDX headers per its licensing policy)
instead of reading it from a zip.

Template compliance: repo-template-standard.md v1.2.0, generated
2026-09-23. Deviation: memory lives in `../memory/` (see Memory). Check `~/dig/bin/new-agent-repo.d/repo-template-standard.md` for
the current version and re-review this repo against its changelog when
they diverge.

## Working rules

- **CLI tools before GUI tools.** Default to CLI/API tools -- more context
  density per token and composable in ways GUI automation isn't. Reach for
  GUI or browser automation only when the task genuinely has no CLI/API
  path.
- **UNIX philosophy + modularity.** Small, single-purpose tools over
  monolithic scripts; shared logic goes in a reusable `lib/`, promoted to
  a fleet-wide shared location once duplicated across repos instead of
  copy-pasted. See `repo-template-standard.md` item 10.
- **Artifacts stay local by default.** Reports, audits, and any other work
  product touching this repo's real/private data go in a local file, never
  a hosted `claude.ai` artifact, gist, or pastebin -- unless explicitly
  asked for a shareable link.
- **Push/deploy cadence: auto-push.** Private repo, no deploy target —
  push to `origin` after each verified commit.
- **Secrets:** never commit secrets or credentials. Keep a `.env.example`
  (or equivalent) in git and the real values gitignored.

## Memory

OM has no repo-local `memory/` (deliberate deviation from the template).
The one shared memory store for all lonunjie repos is `../memory/` in the
control-plane root — read `../memory/MEMORY.md` and write OM facts to
`../memory/topics/om-*.md`. Never store secrets or raw transcripts.

## Reports

Dated, evidence-backed writeups (audits, sweeps, plans, handoffs) go in
`docs/reports/` -- see `docs/reports/README.md`. Move a report to
`docs/reports/completed/` once fully executed, keeping an "Applied"
section (date, work-package id, commit) for lineage.

## Standing operator policy (cross-repo, do not duplicate here)

Check `codex/shared-agent-profile/memories/preferences.md` for policy that
applies to every repo, not just this one -- in particular:

- **Commit without asking** once a change is good and verified, in any repo.
  Push and deploy stay separate, ask-first decisions.
- **Token-efficient multi-agent operation** -- when to parallelize vs. stay
  sequential, task grouping/ordering, and keeping skills/plugins/MCP scope
  narrow. Read this before reaching for parallel subagents by default.
