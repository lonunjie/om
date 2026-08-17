# ADR-0005 — Hot/Cold Storage and StoragePool

**Status:** Accepted

Delivery queue, hot sync, hot media, cold archive, and recovery are separate classes. Users may select different providers for each. StoragePool can mirror encrypted blobs across S3-compatible stores, Drive-class providers, NAS/WebDAV/local storage, etc.

Start with single + mirrored copies. Erasure coding is an architectural extension, not v1.
