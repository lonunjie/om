# ADR-0079 — Resolve architectural choices and split remaining work into typed gates

## Status
Accepted — 2026-09-23

## Decision
The former Q-001..Q-061 ledger is closed as a design-question list. [`ARCHITECTURE_RESOLUTIONS_20260923.md`](../docs/reviews/ARCHITECTURE_RESOLUTIONS_20260923.md) records the accepted choices. Remaining work is represented separately as architecture verification gates, public-release gates, deferred product measurements, and new architecture-design queue items.

Engineering remains locked while an architecture-design item is open or a blocking verification gate has not passed.

## Why
A proof obligation is not an undecided architecture. A store/legal release check is not a crypto design decision. Mixing them made “60 open questions” overstate uncertainty and made automation less precise.
