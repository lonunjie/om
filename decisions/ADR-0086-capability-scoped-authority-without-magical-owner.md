# ADR-0086 — Roles bundle scoped capabilities; no magical owner or unrestricted Administrator is required

## Status
Accepted — 2026-09-23

## Decision
Room/Space authority is capability-scoped. Roles are named bundles of grants, explicit restrictions override ordinary grants, and delegation ceilings prevent a role manager from granting authority outside its authorized scope.

OM does not require a permanent unique owner or universal Administrator bypass. Creation may start with simple 1-of-1 governance; transfer is an authenticated governance transition. If all governance authority is lost without a pre-authorized succession path, the object becomes administratively frozen rather than giving SeedLife/providers a hidden takeover power.
