# ADR-0051 — One Shared OM UI Across Official Clients

- **Status:** Accepted
- **Date:** 2026-09-22

## Context

Separate SwiftUI/Compose/web implementations would drift in security-sensitive wording, navigation and interaction semantics and multiply the audit surface.

## Decision

Maintain one shared `om-ui` implementation above `om-core`. PWA, iOS, Android and desktop official distributions render the same application experience; native shells expose platform capabilities/system surfaces through `PlatformAdapter`.

## Consequences

- official clients stay visually and behaviorally coherent;
- security-sensitive actions reach the same Core commands;
- platform-native UI remains for OS-owned surfaces;
- third-party clients remain free to implement the public protocol independently.
