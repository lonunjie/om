# ADR-0166 — Consolidate the parallel 2026-09-29 design lines

- **Status:** Accepted
- **Date:** 2026-09-29
- **Scope:** repository/ledger consolidation and semantic conflict resolution; no new protocol, cryptographic or authority rule
- **Consolidates:** `checkpoint/20260929-0634` → `checkpoint/20260929-0902` (independent-review line) with `checkpoint/20260929-0900` (Run line)
- **Does not authorize engineering.**

## Context

Two OpenAI design sessions started from the same reviewed export (`76ad8f2`, ADR-0156) and worked in parallel on 2026-09-29:

1. **Independent-review line:** a different-vendor review of ADR-0154–0156 (ADR-0157 critical-prefix convergence,
   DeviceHistorySnapshot/GroupHistoryBundle split, scheduled-send late binding; ADR-0158 `LanDeliveryProfile-v1`), then
   R15 UI/UX integration (ADR-0159), R16A/R16B gate evidence and R16C V-008 wire freeze (ADR-0160).
2. **Run line:** Runs 1–7 (ADR-0157..0162 in that line): a follow-up semantic audit, protocol/security convergence, UI
   hardening, release-candidate cleanup, implementation-readiness contracts, dangerous algorithms and protocol evolution.

Both lines allocated the same identifiers (ADR-0157..0160, D-179..D-181, R-346..R-360, T-136..T-138) to different
content, and Run 1 resolved the same three review findings as ADR-0157/0158 differently. A plain merge would have produced
duplicate IDs and contradictory normative rules.

## Decisions

### 1. The independent-review line is the trunk

It carries the different-vendor review record, the full UI/UX design track integration and the R16 gate evidence, so its
IDs and semantics are kept unchanged.

### 2. Run-line IDs are renumbered after the trunk

| Run line | Consolidated |
|---|---|
| ADR-0158 (Run 2) | ADR-0161 |
| ADR-0159 (Run 3) | ADR-0162 |
| ADR-0160 (Run 5) | ADR-0163 |
| ADR-0161 (Run 6) | ADR-0164 |
| ADR-0162 (Run 7) | ADR-0165 |
| D-180..D-184 | D-182..D-186 |
| R-349..R-374 | R-361..R-386 |
| T-138..T-154 | T-139..T-155 |

Evidence files named after Run-line ADR numbers were renamed to match (for example [`protocol_semantics_adr0161.py`](../verification/bounded/protocol_semantics_adr0161.py)).
The Run line's `docs/checkpoints/CHECKPOINT_20260929_*.md` files remain as historical records.

### 3. Run 1 (Run-line ADR-0157) is not adopted

The trunk's reviewed ADR-0157/0158 already govern the same findings, and R16A/R16B evidence is built on them:

- critical extensions use the reserved `om.critical.` prefix, not a `!` ID class;
- same-owner history is `DeviceHistorySnapshot` and the interoperable group object is `GroupHistoryBundle`
  (not `AccountHistorySnapshot` / Room `HistoryBundle`);
- LAN delivery is ADR-0158's seed-encrypted WebRTC DataChannel profile with `LanFrameV1`, not a TLS-exporter channel
  proof with pull/range blob frames.

Later Run-line text that depended on Run 1 was rewritten to these terms: ADR-0161's GroupHistoryBundle binding,
ADR-0164 §8 LAN admission, and ADR-0165's criticality rule. Run 1's extra V-005 `!` vectors and registry labels were
dropped. The Run-1 ADR text is preserved only in `checkpoint/20260929-0900`.

### 4. Run 3 (ADR-0162) is a hardening layer on the ADR-0159 shell

ADR-0159 governs shell presentation (Chats-first, user-composable destinations). ADR-0162 adds Omnibar privacy/safety
boundaries, semantic color/icon roles, theme icon/path/extraction safety, user-owned density/motion/alert policy and
pinned call participants. ADR-0159's OM Clear/Paper/Iris/Graphite presets replace Run 3's OM Calm/Evergreen/Warm
Slate/Violet palettes, which conflicted with the rule against status-hue brand accents. Run 4's Q-048/DP-011 change was
reverted with them; its DP-002/DP-007 clarifications were kept.

### 5. Gate ledger merge

V-005 and V-024 keep the trunk's criteria and evidence. V-004 keeps the trunk's R16B progress and gains the ADR-0161
GroupHistoryBundle binding/isolation criterion plus the ADR-0163 crash/retry note. V-002, V-003 and V-021 take the Run
line's added criteria/evidence. No gate is newly passed by this consolidation.

## Consequences

- One tree carries both lines, with unique IDs. `design_open=0`, and engineering remains unauthorized.
- Review routing: ADR-0157–0165 are OpenAI-authored and need a non-OpenAI independent reviewer. This ADR is
  Anthropic-authored and needs a non-Anthropic one. The rewrites under §3 and §4 are part of this ADR's review surface.
- [`tools/validate_architecture.py`](../tools/validate_architecture.py) guards the §3 choices so the superseded variants cannot return silently.
- [`machine/checkpoint_lines.json`](../machine/checkpoint_lines.json) lists `checkpoint/20260929-0900` as a non-adopted line, so
  registry-immutability checks use the trunk checkpoint as their baseline. The Run-2 crypto-profile rule is appended
  rather than inserted, so existing rule indices stay unchanged.
