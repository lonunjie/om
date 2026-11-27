# ADR-0275 — Where the engineering host's export history comes from (v1)

- **Status:** Delegated design decision (additive; independent review remains required).
- **Date:** 2026-11-27
- **Scope:** the portability adapter in `apps/web/src/host/scope/` (`HistorySource`).
- **Authority:** delegated by the operator on 2026-11-26 and 2026-11-27. It follows ADR-0256, ADR-0263 and ADR-0271 and adds no wire object, no Core authority and no format.

## Decisions

1. **History is read through Core's existing queries, for the captured profile only.** The default `HistorySource` of the engineering host lists conversations and reads message projections through the public Core queries that the reference client already uses, under the captured `ProfileExecutionContext`. It reads nothing from storage, other profiles or other engines, and never writes.
2. **Export is a projection, not canonical evidence.** The messages become plain archive records labelled as a local projection of this device's history (the archive's own record kind, not a canonical object). The export report says canonical evidence is not included (the existing `canonical_evidence` detail). No signature, key, device list or control material is exported.
3. **Exclusions stay Core's.** View-once and unexpired disappearing items are excluded and counted exactly as the archive writer already does; blobs are included only when the host holds the decrypted bytes and the user selected media, otherwise reported as omitted.
4. **Availability is honest.** If the profile has no identity, no conversations, or a query is refused, `state()` reports export `device_unavailable` or an empty history choice; it never fabricates history.
5. **Not in v1:** canonical-object export with signatures, cross-device history merge, encrypted archives.
