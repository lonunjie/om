# ADR-0035 — BlobStore expansion and direct file paths

- **Status:** Accepted
- **Checkpoint:** 2026-09-21 21:32 PHT

## Decision

Large/offline blobs and backups use a pluggable BlobStore such as R2. Online/LAN P2P transfer is preferred where policy permits; bounded DO spool may cover small temporary encrypted blobs.
