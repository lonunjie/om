# OM Import, Export and User Escape

- **Status:** Canonical pre-engineering architecture
- **Checkpoint:** 2026-09-23 07:56 PHT
- **Resolves:** A-012
- **Scope:** client/provider escape, portable archives, history import, foreign messenger migration, provenance/authenticity, identity secrets, selective export and safe rehydration

## 1. Decision summary

OwnMessenger/OpenMessenger require a real exit path.

A user must be able to leave a client, provider or hosting stack without converting their history into a proprietary hostage format and without requiring SeedLife approval.

OM distinguishes four operations that must not be conflated:

```text
1. live provider migration
2. client/device migration
3. portable history/state archive export/import
4. foreign-messenger history import
```

A portable archive restores **data and evidence**, not cryptographic authority. Account authority/recovery follows the dedicated account-control/recovery protocol.

## 2. Live provider migration

Moving relays/storage/push/discovery/media providers uses the existing authenticated migration state machines and keeps the same AccountID/ConversationIDs where the protocol permits.

A provider cannot become the root owner of identity merely because it currently stores ciphertext or routing state.

If a provider refuses cooperation, clients should recover/rebuild from other authorized replicas, local state and recovery mechanisms where possible. OM cannot promise recovery of bytes that exist only on an unavailable/hostile provider and nowhere else.

## 3. Client/device migration

Moving to a new device/client is **not** implemented by copying a DeviceVault directory or cloning private device keys.

The new endpoint is enrolled/recovered through the existing AccountControlState/RecoveryPolicy flow, obtains fresh DeviceIdentity material and then syncs/restores allowed state.

This prevents a generic archive importer from silently becoming an authorized account device.

## 4. OM Portable Archive v1

OM defines a public logical archive format, `OM Portable Archive v1` (`om-archive-v1`).

The archive is implementation-independent and may be represented as a directory or a safely bounded package container. The normative layer defines paths/records/hashes and semantics; implementations may additionally offer human-readable exports.

Conceptual structure:

```text
om-archive-v1/
  manifest.cbor            (normative, ADR-0125; archive-manifest.json is an optional rendering)
  canonical/
    events/...
    control-state/...
    publications/...
  portable/
    conversations.jsonl
    contacts.jsonl
    local-state.json?          # optional selected client-local state
  blobs/
    <BlobID>...
  provenance/
    foreign-imports.jsonl?
    bridge-records.jsonl?
  evidence/
    public-authentication-material...
```

The exact container codec is not an identity/security primitive. Parsers must defend against path traversal, decompression bombs, duplicate paths, resource exhaustion and malicious media.

## 5. Preserve exact cryptographic objects when available

For native OM data, an archive should preserve exact canonical bytes for signed/hashed protocol objects where available, plus a portable decoded projection for independent readers.

This allows a future client to distinguish:

```text
cryptographically verifiable native object
locally observed/decrypted record
bridge-attested foreign record
user-entered/imported annotation
```

Export does not upgrade weak provenance into strong provenance.

## 6. ArchiveManifest

Conceptually:

```text
ArchiveManifest
  format_version
  archive_id
  export_scope
  source_account_hint?
  included_conversations[]
  included_time_ranges[]
  blob_inventory[]
  canonical_object_inventory[]
  provenance_profiles[]
  encryption_profile?
  generated_at_hint
  exporter_implementation?
  integrity_root
```

`generated_at_hint` is metadata, not authorization or message ordering authority.

## 7. Private keys and authority are separate

A normal history archive MUST NOT contain raw long-term Account, recovery, DeviceIdentity, MLS or Service/Bridge controller private keys.

Identity continuity uses the dedicated account backup/recovery/enrollment architecture.

If OM supports a user-requested sealed recovery artifact, it is a separately typed object with its own encryption/anti-rollback/recovery policy. Importing a history archive alone never grants authority to send as the account or alter governance.

## 8. Secure and readable export profiles

A client should offer at least:

### Portable secure export

- includes selected history/state/blobs;
- encrypted under a public documented archive-encryption profile;
- intended for migration/backup between conforming clients;
- keeps cryptographic canonical objects where available.

### Human-readable export

- explicit user-selected plaintext output such as JSON/HTML/text/media directory;
- warns that the resulting files are readable by whoever gets them;
- may omit internal protocol objects not useful to humans;
- never becomes the only portable migration format.

Exact password/KDF/AEAD choices follow OM cryptographic profile review rather than a proprietary client scheme.

## 9. Selective export

Users can choose:

- conversations/Spaces/Publications;
- date ranges;
- attachments/media;
- contact/person records;
- aliases/profiles;
- bridge/foreign provenance;
- local bookmarks/pins/preferences where meaningful;
- diagnostics/evidence separately.

Local drafts, notification tokens, provider secrets, caches, push subscriptions, device private keys and similar sensitive/ephemeral state are excluded by default.

Disappearing/expired content that has already been cryptographically/lifecycle-erased cannot be resurrected merely because an archive format exists.

### Disappearing and ViewOnce content (ADR-0129)

1. **ViewOnce is never exported**, in any profile. The archive records at most a placeholder that a ViewOnce item existed.
2. **Disappearing items that have not yet expired** are exported only in portable secure export, and each carries its canonical expiry: the envelope `expiry` (absolute wall clock, ADR-0128 rule 16). The `portable/` projection carries the same value as an `expiry` field on each such record.
3. **Importers enforce expiry.** On import (rehydrate or read-only) a client drops every item whose expiry has passed and schedules deletion of the rest at their original expiry. An importer that cannot enforce expiry does not import disappearing items.
4. **Human-readable export excludes disappearing items by default.** Including them is an explicit per-export choice with a warning that the files will not disappear.

## 10. Importing an OM archive

Import has two distinct modes:

### Rehydrate into the same cryptographic account

Allowed only when the importing client is already an authorized/recovered device for the relevant AccountID and independently validates current live authority/membership where needed.

Archive data can repopulate local history/indexes/blobs but cannot override newer AccountControlState, Room control, revocations, membership or retention state.

### Read-only / historical import

Any client may import data into a local archive/history view without gaining live protocol authority.

It can display old conversations and verify evidence, but it cannot send into a Room merely because the archive contains that Room's old events.

## 11. Foreign messenger imports are not retroactive OM history

A Telegram/Signal/WhatsApp/Matrix/email/etc export does not become canonical OM ConversationEvents allegedly authored by those foreign people.

OM represents foreign imports as local historical records:

```text
ImportedHistoryRecord
  imported_record_id
  source_network_namespace
  source_export_fingerprint
  source_conversation_ref?
  source_event_ref?
  source_actor_ref?
  display_metadata
  content
  attachments[]
  source_time_hint?
  provenance_level
  importer_notes?
```

These records may populate an `ImportedConversation` local history container, but they are non-routable and have no OM Room authority.

This prevents an importer from fabricating cryptographic OM authorship for messages that predate OM participation.

## 12. Bringing foreign history into an active OM conversation

A user may deliberately share selected imported content with an active OM Room.

That creates a **new** OM event authored by the importing AccountPrincipal/Service/Bridge as appropriate, containing a quotation/attachment/provenance reference.

The old foreign message does not acquire a fake OM EventID or AccountPrincipal signature.

## 13. Mapping foreign people to OM contacts

An importer may associate a foreign actor with a local `PersonRecord` or verified `ForeignIdentityLink`, but the association remains local/evidence-based.

Name/avatar/phone/email similarity alone is never sufficient to silently merge cryptographic AccountIDs.

If the same person later joins OM and proves a link, clients can present continuity while preserving the historical provenance distinction.

## 14. Import adapters are untrusted parsers

Each foreign format is processed through a bounded `ImportAdapter`:

```text
ImportAdapter
  format_id
  detect()
  parse_bounded()
  normalize_to_ImportedHistoryRecord()
  enumerate_blobs()
  emit_warnings/fidelity_report()
```

Adapters must treat archives, HTML, JSON, SQLite databases, media and metadata as hostile input.

They run with no account keys/network authority unless separately granted and must not execute embedded scripts/macros/plugins.

## 15. Fidelity and provenance report

Every import/export operation can produce a `PortabilityReport`:

```text
PortabilityReport
  source_format
  destination_profile
  preserved_semantics[]
  lossy_semantics[]
  unverifiable_authorship[]
  missing_blobs[]
  timestamp_limitations[]
  unsupported_features[]
```

Examples:

- reactions flattened;
- edits unavailable;
- sender identity based only on source export label;
- attachment missing;
- topic hierarchy lost;
- source timestamps are untrusted hints;
- deletion history unknown.

## 16. Exporting from a Room does not export Room authority

A member may export plaintext/history they legitimately possess subject to local law/policy, but the archive does not carry transferable admin/governance rights.

Room control remains live authenticated state. A future client must validate current membership/authority rather than trusting an old archive snapshot.

## 17. Provider-side portability

OM provider roles should expose documented export/migration interfaces for user-owned encrypted objects/capabilities they store where technically possible.

Because many providers hold only ciphertext/opaque routing state, the most useful portable plaintext export normally occurs on an authorized client.

Provider export is never a substitute for client-local escape, and a provider-specific export format cannot be the only canonical format.

## 18. No project-operated migration gate

The OM Project may publish conversion tools or reference adapters but is not a required cloud migration service.

Third parties can implement archive readers/writers/importers from public specifications and test fixtures. Users can move directly between independent clients.

## 19. Security invariants

1. History/archive import never grants Account/Room/Service/Bridge authority.
2. Raw long-term private keys are excluded from ordinary archives.
3. Native cryptographic evidence is preserved exactly where available.
4. Foreign imports retain foreign provenance and never become fabricated native authorship.
5. Archive/import parsers treat all input as hostile and bounded.
6. Provider/client migration does not require SeedLife.
7. No one client/provider format is the sole representation of canonical user-owned history.
8. Disappearing/erased content is not resurrected by migration semantics.
9. Live authority always wins over stale archived control state.
10. Human-readable export is optional convenience, not the portability protocol itself.

## 20. Verification obligation

V-014 must include:

- round-trip archive export/import across at least two independent implementations;
- exact preservation/verification of canonical signed objects;
- stale control state that attempts to override newer revocation;
- malicious archive paths/decompression/resource attacks;
- malformed foreign exports and hostile HTML/media/SQLite inputs;
- missing blobs and partial archives;
- foreign-authorship spoof attempts;
- read-only import attempting to send or acquire Room rights;
- encrypted archive wrong-key/corruption handling;
- proof that ordinary archive import cannot clone a DeviceIdentity/account authority.

## 2026-09-24 verification rulings

Normative, from the V-series models. The ADR is authoritative where this summary is shorter.

- [ADR-0125](../../../decisions/ADR-0125-portable-archive-format-rulings.md): the normative manifest is canonical CBOR `manifest.cbor`, and the JSON and `portable/*.jsonl` renderings are non-normative. Strong provenance levels need `link_evidence`, or they are downgraded. [`om-secret-object-types.json`](../../../spec/registries/om-secret-object-types.json) lists the forbidden secret types. `OMA-ENC-1-candidate` is the provisional encryption profile, pending crypto review. Newer archived control state is never applied; it is re-validated live.
