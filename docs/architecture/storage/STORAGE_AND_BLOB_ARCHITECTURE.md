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
3. upload same ciphertext or re-encrypted ciphertext according to policy;
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
