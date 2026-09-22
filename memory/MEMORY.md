# OM Memory Index

This is the ONLY file every agent (Claude, Codex, Antigravity/Gemini, or
any other) should load by default. Keep it under ~200 lines — one line per
topic file, index-only. Open a `topics/*.md` file only when the task
actually touches that subject. See `memory/README.md` for the full
read/write contract, size cap, and compaction rule.

(no topics yet)
