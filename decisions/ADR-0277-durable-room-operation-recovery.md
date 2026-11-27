# ADR-0277 — Durable Room operation recovery in the engineering host

- **Status:** Delegated design decision (additive). Independent review remains required.
- **Date:** 2026-11-27
- **Scope:** The optional engineering host and bridge and service Room reducers. Follows ADR-0271, ADR-0276 and UI-12–13. No wire or audited text changes.

1. Use the same operation ID for all attempts. The reducer binds the ID to the mutation, principal and review target and records the terminal result. Exact replay returns that result. Conflicting reuse is refused. Each Room reducer retains the latest 64 results in completion order.
2. Keep one device-local snapshot per profile, Room and reducer. Version 1 uses deterministic local JSON with a Core integrity tag bound to the profile, Room and reducer kind. The body is limited to 512 KiB and its state collections are bounded. Import checks offers and membership head references against independently verified current Room observations and discards entries whose basis is missing or stale. Restored bridge membership is a projection with no active export mapping until an authenticated Room admission path supplies that authority. Runtime timestamps are not restored.
3. The engineering host uses one writer per profile. It restores after signed observations are refreshed, saves a baseline before reserving a pending ID, and saves state and receipt together before clearing the reservation or reporting a terminal outcome. Recovery asks Core for the exact receipt. When it is absent, recovery retries under the same identity and current checks. A missing baseline with a pending operation leaves reconciliation pending.
4. Recovery covers local reducer mutations. Room authority, the source of current signed observations, and any future MLS or delivery transaction remain separate. Release construction and the existing deny-all engineering authority stay in force.
