# OM UX Design Reference

`docs/ux-design/` contains detailed reference-client design artifacts integrated by ADR-0159.

## Authority

These files are subordinate to:

1. constitution / accepted ADRs / machine truth;
2. canonical `docs/architecture/` domain documents;
3. these design references;
4. prototypes and research evidence.

A prototype never creates protocol, recovery, provider, group/publication or account-control authority.

## Current canonical design references

- [`SCREEN_MAP_AND_JOURNEYS.md`](SCREEN_MAP_AND_JOURNEYS.md) — reconciled shell/journeys.
- [`COMPONENT_CATALOG.md`](COMPONENT_CATALOG.md) — semantic shared-UI component set.
- [`specs/`](specs/) — detailed interaction, Omnibar, messaging, calls, security, responsive, localization and visual-system specifications.
- [`contracts/`](contracts/) — machine-readable design contracts; not wire schemas.
- [`tokens/`](tokens/) — reference visual/theme token sets.
- [`assets/`](assets/) — functional icon/media/identity-study assets.
- [`prototype/`](prototype/) — current PWA reference fixture (same snapshot as `prototypes/pwa-reference-v3`).
- [`validation/`](validation/) — local/static/browser design validators and benchmark harnesses.
- [`research/`](research/) — evidence notes; non-normative.
- [`reconciliation/`](reconciliation/) — R14 audit and R15 integration trace.
- [`design-decisions/`](design-decisions/) — historical standalone UI/UX-track decisions. Accepted architecture ADRs in `decisions/` are authoritative when they differ.

## Open evidence

V-018 security UX comprehension and V-022 performance budgets remain open. Final brand/logo and human icon/ergonomic validation are also deferred. Existing browser/static evidence must not be represented as those gates passing.
