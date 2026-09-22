# OM Local Memory

This folder is the ONE canonical memory store for OM work — used
identically by **Claude, Codex, and Antigravity/Gemini** (and any future
agent). It is not any one tool's private store. `AGENTS.md` is the single
source of truth that governs this; `CLAUDE.md` and `GEMINI.md` at the
repo root are one-line pointers to it. Same contract as `ags`,
`lydia-v1`, and `xx-network`.

## Files

- `MEMORY.md` is the ONLY file every agent loads by default. It is an
  INDEX ONLY — one line per topic file. Hard cap: **~200 lines**.
- `topics/*.md` holds the actual memory entries — one topic per file:
  ```
  ---
  name: kebab-case-slug
  description: one-line summary (used to judge relevance)
  metadata:
    type: user | feedback | project | reference
  ---
  ```
  Cross-link related topics with `[[other-topic-name]]`. Open a topic
  file only when the current task actually touches that subject.
- `inbox.jsonl` is an append-only trail for new memory candidates not yet
  curated into a real topic file.
- `archive/` is for retired/superseded topic files — a topic that's no
  longer current gets MOVED here.

## The compaction rule (enforced, not aspirational)

Before adding a new line to `MEMORY.md` at/over ~200 lines, first move at
least one existing entry's topic file to `archive/` (and remove its index
line).

## What belongs here

Facts about OM specifically. Cross-repo operator preferences belong in
Codex's shared global memory
(`/Users/maj/dig/codex/shared-agent-profile/memories/preferences.md`,
edited directly — no package/apply tool, that pattern was retired
2026-08-10); facts owned by another workspace's domain get written into
THAT workspace's memory directly. Never store secrets, credentials, tokens,
or raw transcripts.

## Memory hygiene (learned the hard way, 2026-08-10)

A sweep across every agent workspace found the same fact duplicated
verbatim across multiple memory files, and four blocks in global Codex
memory still pointed at a hub/tool retired months earlier — nobody had
touched them because nothing forced a re-read. Don't let that recur here:

- **Single source of truth.** If a fact could be read from two different
  files, that's a bug — put it in ONE canonical file and make every other
  reference a pointer (a link, a one-liner, an `@AGENTS.md`-style
  include), never a second copy of the actual text.
- **When you retire a tool, workflow, or path, grep for it immediately** —
  every `AGENTS.md`, `CLAUDE.md`/`GEMINI.md`, `memory/README.md`, and
  `memory/topics/*.md` in this repo, not just the file you were already
  editing. Fix live (present-tense) references; leave historical
  (`memory/topics/*.md`, `inbox.jsonl`) records describing what was true
  at the time exactly as they are — a memory topic saying "X was retired
  on <date>" is correct forever, not stale.
- **Before writing a new note into global Codex memory, check whether it
  already lives in `00-bootstrap.md` or `preferences.md`** — those two
  files drifted into duplicating each other over months of independent
  edits. `preferences.md` is canonical; `00-bootstrap.md` is Codex's own
  session-start file and should only get a one-line pointer, never a copy.
