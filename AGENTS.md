# OM Agent Instructions

OM — an open-source, operatorless end-to-end-encrypted messenger (Open/Own/Our/One/Oh Messenger) by SeedLife. Architecture/design phase; engineering not yet authorized.

<!-- Fill in: what this workspace owns, where it sits relative to other
     repos, and the working rules that matter here. Keep this the ONE
     source of truth -- CLAUDE.md and GEMINI.md are pointer stubs only. -->

Template compliance: repo-template-standard.md v1.2.0, generated
2026-09-23. Check `~/dig/bin/new-agent-repo.d/repo-template-standard.md` for
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
- **Push/deploy cadence:** <!-- TODO: state explicitly -- auto-push,
  batched/reviewed push, or manual-only. Don't leave this implicit. -->
- **Secrets:** never commit secrets or credentials. Keep a `.env.example`
  (or equivalent) in git and the real values gitignored.

## Local Memory

**This is the ONE shared memory system for this workspace -- used
identically by Claude, Codex, and Antigravity/Gemini.** No agent should
keep its own separate private memory store for OM content instead of
writing here.

- `memory/MEMORY.md` is the ONLY file to read by default -- a small
  index, one line per topic, hard-capped ~200 lines. `memory/topics/*.md`
  holds the actual entries; open a topic file only when the current task
  touches that subject. Full contract: `memory/README.md`.
- Use `memory/inbox.jsonl` for durable candidate notes that need later
  curation.
- Compaction is a gate, not a suggestion: before adding a new line to
  `MEMORY.md` at/over ~200 lines, move at least one existing topic to
  `memory/archive/` first.
- Do not store raw transcripts, secrets, credentials, tokens, or sensitive
  personal content in memory.

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
