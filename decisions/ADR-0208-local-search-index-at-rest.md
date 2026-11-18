# ADR-0208 — The local search index at rest

## Design sources read

[`OMNIBAR_INDEXING_AND_PERFORMANCE.md`](../docs/ux-design/specs/OMNIBAR_INDEXING_AND_PERFORMANCE.md) (all), [`DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`](../docs/architecture/client/DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md) sections 4 and 5,
[`MULTI_IDENTITY_AND_PROFILE_ISOLATION.md`](../docs/architecture/client/MULTI_IDENTITY_AND_PROFILE_ISOLATION.md) (all), [`UI_CORE_PLATFORM_CONTRACT.md`](../docs/architecture/ux/UI_CORE_PLATFORM_CONTRACT.md) (all), and the
relay-status deviation D25. The other mapped client, crypto, discovery, identity, metadata, delivery and
time documents were not read in full.

- **Status:** Accepted by delegation, 2026-11-18.
- **Scope:** fills a silence only. [`OMNIBAR_INDEXING_AND_PERFORMANCE.md`](../docs/ux-design/specs/OMNIBAR_INDEXING_AND_PERFORMANCE.md) section 12 requires the full-text
  index to be encrypted at rest and says key handling "must be reconciled with OM's main local-storage/security
  design before implementation". This ADR is that reconciliation. It amends no audited text and weakens no floor.

## Decision

1. **Key.** The index is sealed only under the profile's vault subkey for the `search-index` purpose
   (the `om:v1:search-index` branch of the ProfileVaultKey hierarchy). No new key, no new KDF label and no
   user-visible choice exist. The vault never hands the key out; a locked vault can neither read nor write the index.
2. **Place.** Records live in the identity's own vault database, in the encrypted key-value store, under the key
   prefix `ui/search/v1/`, which Core never reads. They are therefore partitioned per identity, deleted with the
   identity's vault, and subject to the same browser eviction as every other vault record. Eviction loses a cache
   that is rebuilt from Core's views; it loses nothing else.
3. **Content.** For each conversation (a one-to-one chat or a group): its display name, the position of its oldest
   kept message, and for each kept message its id, send time, text (at most 4,000 characters) and attachment file
   names. The folded matching form is recomputed on load and never stored. Group messages are indexed the same
   way as chat messages; the sender's name is not stored.
4. **Layout and incremental updates.** A manifest per conversation plus segments of 256 messages keyed by their
   position in the whole timeline. A new message rewrites the newest segment and the manifest; an edit or delete
   rewrites the segment holding it. Segments are written before the manifest that names them; a segment no
   manifest names is swept at the next load, so a crash at any point leaves a usable store. Writes are batched
   for under a second after a change; rebuilding everything is a recovery path, not startup.
5. **Bounds.** At most 200 conversations and the newest 5,000 messages of each (as before, D25), and at most
   24 MiB of text at rest per identity. Over budget, the conversations whose newest message is oldest are dropped
   whole; a conversation dropped in a visit is not written again until the next load.
6. **Versioning and migration.** The format version is in every key and in every manifest. A reader accepts only
   its own version: records of an older version are deleted (derived data is rebuilt from Core, never migrated),
   records of a newer version are left alone for the build that wrote them, and any record that is damaged, does
   not authenticate or has an unknown shape is deleted and rebuilt. Index records never carry anything Core needs.
7. **Lifecycle.** After unlock the stored index is read back and searchable at once; each conversation is then
   re-read from Core and replaces its stored copy (a live read wins). A conversation absent from Core's lists
   after the stored copy was loaded is forgotten at rest, except that stored groups are kept while the group
   module is not loaded. A message Core no longer shows disappears from the stored copy at the next write; if the
   vault locks before that write, the stale ciphertext remains until the next unlock reconciles it.
8. **Never leaves the device.** No part of the index is uploaded, synced between devices, sent to a relay,
   analytics or OS search. No new relay-visible signal exists. Opening a hit opens the conversation at that message.

## Consequences

D25 (the index lives in page memory only) is retired: the index is encrypted at rest and persists across reloads.
D25's cost "the index is rebuilt at every unlock" no longer applies. Messages longer than 4,000 characters are
searchable only in their first 4,000 characters, in memory and at rest alike.
