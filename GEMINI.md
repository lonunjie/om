# OM — Antigravity/Gemini entry point

This file exists only because Antigravity/Gemini tooling auto-loads
`GEMINI.md` by convention. All actual instructions live in `AGENTS.md`
(shared across every agent that works in this repo — Claude, Codex, or any
future agent) so there is one source of truth instead of per-agent forks
that drift apart.

Do not add Gemini-specific instructions to this file. Add them to
`AGENTS.md` — if a rule matters, every agent needs it, not just this one.
That includes memory: the one shared store is `../memory/` in the
lonunjie control-plane root (see `AGENTS.md`'s "Memory" section) — do
not keep OM context in an agent-private memory store instead.

@AGENTS.md
