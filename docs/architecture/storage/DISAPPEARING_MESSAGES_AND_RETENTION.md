# Disappearing Messages and Retention

## 1. Threat model

Disappearing messages are retention controls, not DRM. A recipient can copy, photograph, screenshot, export, or modify their client.

OM should avoid server-visible "this message disappears" metadata when it is not required for transport/resource cleanup.

## 2. Client-enforced lifecycle

Conversation policy defines encrypted/authenticated retention semantics. Each authorized device tracks expiry locally.

Possible timers:

- after send;
- after first successful delivery;
- after first view/read where the protocol can define this consistently;
- explicit absolute expiry.

The exact v1 set should remain small and testable. For v1 the only interoperable timer is absolute after-send
expiry, carried in `ConversationEvent.expiry` (key 12, wall clock; see below).

## 3. Relay behavior

Relay queues already expire undelivered envelopes under ordinary queue TTL. They do not need to know a human-facing disappearing-message policy.

## 4. Backup behavior

Recommended rules:

- `view-once`: never included in history backup;
- expired disappearing event/content: never restored;
- unexpired disappearing content: excluded by default, or stored only with encrypted authenticated absolute expiry and deleted immediately if expired on restore;
- derived thumbnails/search indexes/previews must obey the same deletion lifecycle.

## 5. Multi-device convergence

Expiry semantics and deletion tombstones must converge across devices without allowing an old offline device/backup to resurrect expired content into the active materialized view.

The canonical event log may retain minimal authenticated tombstone/control evidence where needed for convergence while deleting recoverable plaintext/ciphertext payload material according to policy.

## 6. Attachments

Attachment content keys can be destroyed/forgotten at expiry. BlobStore deletion is best-effort because remote storage may have provider backups, but deleting the client-side decryption key provides the strongest portable guarantee available under the model.

## 2026-09-23 expiry resolution

v1 interoperable disappearing messages use absolute after-send expiry, carried in **`ConversationEvent.expiry`** (absolute wall-clock seconds, [`events.cddl`](../../../spec/cddl/events.cddl) key 12). `MessageCreate.expires_after` is only an optional local hint for a receipt/read-based timer: a receiver MAY delete earlier because of it, but MUST NOT use it to keep content past `expiry` (ADR-0126; [`om-time-fields.json`](../../../spec/registries/om-time-fields.json)). “After read” remains local-retention behavior until a later explicit extension because making it canonical would require additional receipt/convergence semantics. Expiry destroys local attachment keys/removes locations, leaves anti-resurrection tombstones and excludes ephemeral content from ordinary backup by default.
