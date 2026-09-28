# Storage and Blob Architecture

## Separate storage classes

OM must not collapse all persistence into “the server.” These concerns have different security, cost, availability, and retention requirements:

1. **local working database** — canonical materialized local state and cryptographic state;
2. **delivery queue** — short-lived encrypted envelopes awaiting devices;
3. **hot account/device sync** — account-private state transfer/synchronization;
4. **hot media/blob store** — encrypted attachments/files referenced by events;
5. **cold archive** — optional long-retention encrypted history/media;
6. **recovery store** — minimal encrypted material required for recovery policy;
7. **backup snapshots** — encrypted portable restoration packages/manifests.

## Blob model

Large content is encrypted before upload:

```text
plaintext attachment
  -> random content key
  -> client-side AEAD
  -> ciphertext blob
  -> one or more BlobStores
```

The message/event carries an authenticated attachment descriptor with content hash/length/type and one or more opaque locators plus encrypted/wrapped content-key material appropriate to the conversation security context.

## StoragePool

`StoragePool` chooses and may mirror providers by class and policy:

```text
StoragePool
  hot_media: [provider A, provider B?]
  archive: [provider C]
  backup: [provider D, local NAS]
  recovery: [provider E]
```

Initial redundancy should use full-copy mirroring because verification/repair/restore are simple. Erasure coding remains possible later behind the same abstraction.

## R2

Cloudflare R2 is one attractive `BlobStore` implementation because it is S3-compatible, has a useful free allocation, and does not charge Internet egress on standard storage as of the snapshot date. It is **not** OM's permanent database and should not be required for protocol correctness.

Suitable uses:

- encrypted photo/video/file attachments;
- encrypted history bundles;
- encrypted backup/archive objects;
- replicated ciphertext objects.

Not preferred as:

- tiny per-message offline queue;
- root identity store;
- plaintext search/index.

## Provider migration

Blob locators are replaceable. Migration can:

1. fetch ciphertext from old provider;
2. verify authenticated digest/length;
3. upload the same ciphertext. Events carry a fixed `ciphertext_digest` and content key, so re-encrypted ciphertext would fail verification; re-keying means a new blob referenced by a new event;
4. update encrypted/private manifests;
5. retain multiple locators during transition;
6. delete old copy after verification and retention window.

No message meaning or AccountID changes.

## Backup

A backup is self-describing enough to restore without the original storage vendor. It must include versioned encrypted manifests and authenticated metadata. Recovery keys must not be recoverable by the storage provider alone.

## Cost policy

Providers expose operational metadata such as free-tier allowance, hard/soft quota behavior, expected operation classes, and configured budget. Core routing asks for abstract cost/health signals rather than hardcoding vendor numbers.

## Required tests

- restore without original provider;
- mirror repair after one provider loses objects;
- corrupt-object detection;
- provider migration while clients are active;
- old locator expiry;
- no plaintext reaches BlobStore;
- backup made under old CryptoProfile restores then migrates safely;
- zero-budget policy never causes billable provider usage unless explicitly allowed.

## Zero-payment personal bootstrap

The baseline Cloudflare profile no longer requires R2. Worker + SQLite Durable Object can cover relay/control state and a tightly bounded temporary encrypted blob spool. R2/other BlobStore is enabled for larger/offline media or backup.

V1 online/LAN file transfer prefers direct encrypted paths where policy permits. Direct transfer carries the same BlobID/ciphertext_digest-bound encrypted object as provider storage; later mirroring/backfill never rewrites canonical attachment identity. Local receipt is not presented as satisfying configured StoragePool durability until the normal replication policy is actually met (ADR-0150). Exact spool/blob cutoffs are operational benchmarks.

## Restore invariant

Backups restore history, not authority. Current AccountControlState, revocations, security floors and live provider capabilities must be reconciled before restored material becomes operational.
## 2026-09-22 hardening — immutable BlobID, mutable location knowledge

Attachment/object identity is separated from provider location. Canonical events refer to a random, stable BlobID, and integrity comes from the event's `ciphertext_digest` (ADR-0119 ruling 6); `BlobLocationRecord` carries authenticated replaceable locators. Provider migration therefore never rewrites canonical message identity.

[`REACHABILITY_AND_OBJECT_REPAIR.md`](../delivery/REACHABILITY_AND_OBJECT_REPAIR.md) defines overlap and late/offline-recipient repair when an old locator disappears before a replacement reaches every authorized device.

## 2026-09-23 blob resolution

BlobID is a random identifier that stably names the inner encrypted object; integrity comes from `ciphertext_digest`; provider locators are mutable. E2EE events carry decryption material and an opaque repair capability; authenticated BlobLocationRecord repair permits relocation/mirroring without changing canonical message identity.

## CONFORMANCE-ATTACHMENTS

The following are normative pass/fail cases for the registered W0 conformance suite. Passing these cases is evidence; it does not by itself authorize a scope.

| ID | Case | Pass condition |
|---|---|---|
| AT-01 | blob fetched from alternate store | digest/authentication yields identical plaintext or rejects |
| AT-02 | locator tamper/truncation | detected before rendering; no partial trusted content |
| AT-03 | moved/repaired BlobLocationRecord | only signed valid newer location state is followed |
| AT-04 | expired/deleted attachment | lifecycle policy prevents normal re-materialization from OM-managed stores |
| AT-05 | expressive asset ref | digest-bound blob, not provider locator/blob id, is rendering authority |
| AT-06 | encrypted blob received directly over LAN, then provider mirror/backfill appears | BlobID and ciphertext_digest identify the same authenticated object; provider location changes do not rewrite message identity, and UI/storage state distinguishes local receipt from configured durability until replication succeeds (ADR-0150) |
