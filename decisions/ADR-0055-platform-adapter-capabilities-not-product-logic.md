# ADR-0055 — PlatformAdapter Exposes Capabilities, Not Product Logic

- **Status:** Accepted
- **Date:** 2026-09-22

## Decision

Native/web platform modules implement narrowly scoped capabilities such as secure storage, push, system calls, files, sharing, biometrics and background execution. Product/security semantics remain in shared Core/UI.

UI should query capabilities rather than scattering `isIOS`/`isAndroid` behavior checks.
