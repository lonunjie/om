# ADR-0090 — Content packs are portable inert releases, not app-store objects

## Status
Accepted — 2026-09-23

## Decision
A `ContentPackID` identifies a logical pack across immutable canonical `ContentPackRelease`s. Optional signed control-state lineage authorizes updates; forks receive a new PackID and may preserve provenance. Packs contain data and expressive-asset references only, can be directly shared/hosted/mirrored/installed, and do not require a SeedLife catalog. Publisher signatures prove provenance/update continuity rather than copyright ownership, safety or endorsement.
