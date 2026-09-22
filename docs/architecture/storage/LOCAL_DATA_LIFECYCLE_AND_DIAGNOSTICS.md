# Local Data Lifecycle and Privacy-Safe Diagnostics

## 1. LocalDataClass

Every persisted/derived artifact must declare a lifecycle class.

| Class | Examples | Required behavior |
|---|---|---|
| `Secret` | private keys, capability secrets | DeviceVault; never logs/backups unless explicitly wrapped by recovery design |
| `Persistent` | normal messages | encrypted local store; deletion semantics apply |
| `Recoverable` | backup-eligible content | encrypted manifest/store; never authoritative control state |
| `Ephemeral` | temp transfer chunks | bounded TTL + purge |
| `DerivedCache` | thumbnails/render cache | recreatable; purgeable; no independent authority |
| `ViewOnce` | view-once media | no backup/search/preview persistence; destroy promptly after lifecycle ends |
| `Diagnostic` | redacted errors/metrics | local, bounded, rotating, export only by user/action |

## 2. Deletion closure

Deleting/expiring content must remove or invalidate every derived representation permitted to exist:

- event/materialized row;
- encrypted search tokens/index record;
- reply/snippet cache;
- thumbnail/waveform;
- notification preview cache;
- temporary decrypted file;
- attachment key where cryptographic deletion is used.

Tests should ask: **where can this LocalDataClass exist?** and assert the answer.

## 3. Search

Search indexes are profile-scoped and encrypted at rest. Do not put plaintext global full-text indexes beside an encrypted message database.

## 4. Clipboard/screenshots

OM cannot guarantee deletion after users deliberately copy/screenshot/export content. UI should avoid automatic sensitive clipboard writes and clear temporary app-owned clipboard content only where platform semantics permit without surprising the user.

## 5. DiagnosticsStore

Default diagnostics are:

- local only;
- bounded/rotating;
- redacted before persistence;
- keyed by anonymous local error/category codes rather than contact identity.

Never log:

- plaintext messages/drafts;
- private keys/recovery phrases;
- capability tokens;
- auth/bearer tokens;
- push subscription URLs/keys;
- full AccountIDs/contact descriptors where a short local correlation ID suffices;
- attachment decryption keys.

## 6. Export diagnostics

User explicitly chooses `Settings -> Advanced -> Export diagnostics`.

Bundle may include:

- client/build/protocol version;
- platform/browser capabilities;
- feature flags;
- provider health/status codes;
- bounded redacted timings;
- state-machine error codes;
- recent non-secret logs.

Show the user a summary before share/save. Do not silently upload to the OM Project.

A particular commercial/client distribution may offer opt-in crash reporting, but it is distribution-local and must follow the same redaction rules.
