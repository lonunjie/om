# ADR-0100 — Published protocol generations are independently preservable and incompatible evolution forks explicitly

- **Status:** Accepted
- **Date:** 2026-09-23

## Decision

Every released OM protocol generation/profile must be interpretable from mirrorable self-contained public artifacts rather than a live project registry/server. Existing registry IDs never change meaning; future incompatible lineages use new profile/registry identifiers.

Protocol/client/provider/project forks are explicit. A dominant client or proprietary extension gains no special protocol authority from market share, and proprietary permission-gated semantics cannot become mandatory baseline OM conformance. Crypto/project/provider failure uses authenticated migration where possible and honest re-verification/fork semantics where cryptographic continuity can no longer be proven.
