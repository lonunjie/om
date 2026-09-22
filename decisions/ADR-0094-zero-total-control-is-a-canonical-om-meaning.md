# ADR-0094 — 0M means zero total control

- **Status:** Accepted
- **Date:** 2026-09-23

## Decision

OM gains a seventh canonical meaning: **0M**, where the `O` is read as zero. It means **zero parties have total control of OM**.

This is a decentralization/anti-capture principle, not a claim that every deployment/component is peer-to-peer. Bounded centralized operators are allowed; universal indispensable control is not.

## Consequences

- bot/service creation cannot require one issuer;
- directories/catalogs remain replaceable;
- dominant clients/providers receive no protocol privilege;
- local communities/providers may govern their own scope without becoming universal OM authorities;
- continuity/forkability remains a security property, not only a governance preference.
