# ADR-0098 — Metadata privacy is surface-specific, not implied by E2EE

- **Status:** Accepted
- **Date:** 2026-09-23

## Decision

OM treats metadata privacy as a per-surface property. Stable canonical IDs are not default provider-routing identifiers; private relay delivery uses scoped opaque capabilities and independently randomized outer transport envelopes so relay providers do not need provider-readable AccountID/ConversationID/inner group identity.

Provider/path metadata exposure is declared separately from content E2EE. Recommended/Hardened/Maximum-Privacy policy may progressively add padding, provider separation, private discovery, relayed calls, batching or anonymity transports, but OM makes no blanket metadata-anonymity claim against collusion/global traffic analysis.
