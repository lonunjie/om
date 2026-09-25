# ADR-0110 — The repository is canonical; design drops are transport

- **Status:** Accepted
- **Date:** 2026-09-24
- **Decided by:** operator

## Context

Until 2026-09-24 the design lived only in zip packages. Each zip copied every earlier snapshot under `history/`
(0229 held 2,637 history files, 19 MB), and nothing in git reviewed changes between checkpoints. Several agents
edit the design, and without diffs their edits could not be reviewed or merged.

## Decision

1. `lonunjie/om` is the single source of truth. The 15 prior checkpoints were replayed as commits tagged
   `checkpoint/YYYYMMDD-HHMM`, and git history replaces `history/`.
2. The layout is organized for public readers (docs by domain, `decisions/`, `spec/`, `machine/`, `verification/`).
   Basenames did not change. [`tools/layout.json`](../tools/layout.json) maps the legacy flat layout.
3. Zips remain the transport to and from external design agents, through a gitignored `design-drops/` zone.
   `tools/om_drop.py export` packages a committed ref with its base commit. `import` commits a returned drop on a
   branch from that base and **merges** it, so concurrent work is kept and conflicts are explicit.
4. [`MANIFEST.json`](../MANIFEST.json) is generated only inside exports. In the repository, git is the integrity record.

## Consequences

Checkpoints become reviewable diffs. External agents must keep [`OM_EXPORT.json`](../OM_EXPORT.json) and the layout (AGENTS.md
"Design drops"). Legacy drops still import through the layout map.
