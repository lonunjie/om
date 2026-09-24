# Independent reviews

Records of reviews that satisfy `independent_reviewer` under [ADR-0117](../../decisions/ADR-0117-ai-agent-review-and-gate-independence.md).
There is one file per review: `<YYYY-MM-DD>-<reviewer>-<scope>.md`.

A record names the reviewer and model id, the brief it was given, each finding, and how each finding was resolved.
The reviewer writes the findings. The integrator adds the resolutions after triage. A reviewer must be from a different
vendor or model family than the artifact's author. The V-series prototypes (Claude-authored) may be reviewed by
OpenAI Codex. The design documents (ChatGPT-authored) may not.
