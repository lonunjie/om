# ADR-0096 — Bridges preserve foreign provenance and explicitly terminate OM E2EE

- **Status:** Accepted
- **Date:** 2026-09-23

## Decision

External-network gateways use typed BridgePrincipals. Foreign actors are bridge-attested provenance, not synthetic AccountIDs. Continuous private-Room bridging is explicit Room state, and OM-native E2EE ends wherever the bridge obtains plaintext.

Adapters expose capability/fidelity limits and carry loop/dedup lineage. Foreign identity links are evidence, never authority to sign as the linked AccountPrincipal.
