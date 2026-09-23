# ADR-0026 — Recovery is optional and off by default

- **Status:** Superseded by [ADR-0111](ADR-0111-identity-recovery-on-by-default.md) (2026-09-24). No-recovery identities remain valid as an explicit choice.
- **Checkpoint:** 2026-09-21 21:32 PHT

## Decision

OM treats no-recovery/disposable identities as valid. Recovery is a versioned RecoveryPolicy selected by the user, not a mandatory provider service.
