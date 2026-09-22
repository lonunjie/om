# ADR-0095 — Service creation is permissionless; agency is authority-based

- **Status:** Accepted
- **Date:** 2026-09-23

## Decision

A ServiceID is created locally with an initial ServiceControlState and signed definition; no global BotFather/store/cloud/registry issues service identity. Hosting and discovery are optional replaceable roles.

OM does not try to classify messages as human versus AI. User-confirmed Account actions remain Account-authored regardless of drafting assistance. Autonomous services are ServicePrincipal-authored. A service acting for a person uses a scoped revocable RepresentationGrant and remains visibly service-authored.

## Consequences

- easy one-click service UX can exist without a central identity issuer;
- service endpoint compromise does not replace the ServiceID;
- cheap service identity creation grants no unsolicited reach or plaintext access;
- personal AI agents can represent people without becoming hidden Account devices.
