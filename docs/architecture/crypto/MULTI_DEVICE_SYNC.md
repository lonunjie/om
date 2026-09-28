# Multi-Device Sync and History — architecture direction

## No master phone

Every authorized device is a first-class cryptographic endpoint. The account is not “the phone plus mirrors.”

## Device state

Each DeviceIdentity has independent keys, capabilities, and revocation state. AccountControlState authenticates the current device set.

## Delivery

A sender encrypts/delivers to the recipient's eligible devices according to the selected 1:1/group protocol. For
established OM-Crypto-v1 1:1 traffic, each target device also has relationship-private `SealedDeliveryKeyRecord`s
scoped to its active relay routes; relay deposits carry only the corresponding `SealedDeliveryEnvelope`. Device
removal/quarantine or route revocation invalidates future use of the affected records. Relays hold per-device opaque
capability envelopes, not clear sender identity or Double Ratchet headers.

## Account-private sync

Preferences and account-local state that should appear on multiple devices are synchronized through a separate encrypted account-private mechanism, not by making chat events globally authoritative for every UI preference.

The settled architecture is an **account-private authenticated E2EE sync channel** carrying versioned sync records to currently eligible devices. It is separate from conversation delivery and from AccountControlState authority. The transport/storage realization may vary, but it cannot give a provider merge or authorization power and cannot turn settings/history sync into constitutional state.

**ADR-0148 V1 minimum.** V1 uses this channel for unsent drafts plus an explicit non-authoritative preference allowlist: mute; personal archive/folder/label state; personal bookmarks/pins; topic/thread follow state; and ordinary per-profile appearance/theme selection. Drafts include text/caption and reply/thread/topic compose context. Raw OS file handles are never synchronized; an unsent attachment may be referenced only if its encrypted account-private/blob representation is already portable to the receiving device.

Concurrent draft edits must not be silently lost to a single timestamp winner: preserve branches, merge deterministically, or surface a conflict until the resolved draft represents the content. Ordinary preferences use typed deterministic convergence. None of these records may send a message or mutate shared conversation/authority state merely because they merged.

The account-private preference channel explicitly excludes AccountControlState/RecoveryPolicy, security floors, credentials, session/ratchet/MLS authority, provider capabilities/deployment credentials, push tokens/endpoints, DeviceVault/app-lock/recovery secrets, per-device notification privacy posture and the active-profile selector. Profile A records never merge into profile B. Revoked/ineligible devices receive no new sync records after revocation propagation.

**ADR-0149 causal/restore hardening.** Every accepted record conceptually binds `profile_id`, origin DeviceID, authenticated origin ACS generation, a durable never-reused per-origin sequence, record type/version, stable item key, value-or-tombstone operation and causal context. The transport/codec may represent that context as a version vector/dotted vector or an independently equivalent form; provider arrival order is never merge authority. Receivers deduplicate without assuming in-order transport and re-check the best authenticated ACS they know before making a newly arrived record effective. Once a receiver knows an origin DeviceID is revoked/ineligible, a not-yet-accepted record from it cannot become newly effective merely because it claims earlier authorship.

For one stable item key, a tombstone wins against a causally concurrent stale value; recreation requires a value whose causal context descends from the tombstone. Compaction may summarize payloads but retains sufficient causal/tombstone high-water state to prevent offline resurrection. Concurrent draft heads remain distinct until a resolution record explicitly binds every head it supersedes; timestamp ordering alone cannot authorize compaction.

HLC is presentation/convergence assistance, not authority. A remote physical component more than the frozen default skew (300 s) ahead of local wall time is quarantined/not merged until it falls within the window or is superseded; it never advances a durable preference floor merely by being far in the future. Causal order and authenticated ACS generation precede HLC/device-id tie-breaking.

**ADR-0156 implementation namespacing.** Every sync record and HistoryBundle carries an implementation/codec identifier inside its authenticated E2EE framing. A device ignores whole records whose identifier it does not recognize and never interprets part of one. Cross-implementation draft/preference/history sync is not a V1 interoperability promise; the Devices screen should say when a linked device cannot share them. Conversation delivery, AccountControlState and device-link notices remain fully interoperable.

Historical backup restore uses the existing fresh-DeviceID rule. Imported draft/preference material is tentative local state until current ACS and reachable sync state are reconciled and is never automatically republished under the fresh DeviceID. Explicit user recreation after reconciliation emits a fresh current-generation causal descendant.

## History transfer

New device history access is distinct from permission to receive future messages. Existing device(s) may produce encrypted HistoryBundles/snapshots for the new device after explicit account authorization. Servers do not receive plaintext history.

## Revocation

Revoked device:

- stops receiving future envelopes;
- loses future group epochs after revocation propagates;
- cannot obtain new history bundles;
- remains potentially able to read plaintext it previously possessed (unavoidable endpoint property).

## Restore

Backup restore recreates local state and then performs protocol-supported reconciliation/migration. It must not require the original relay/storage vendor.

## Verification- and implementation-owned details

No architecture choice remains open in this section. The following details are deliberately owned by the named verification/implementation surfaces while preserving the boundaries above:

- the exact account-private sync record transport/codec is an implementation choice behind the authenticated E2EE sync interface; ADR-0148 freezes the V1 record classes/boundaries above without inventing a second generic CRDT protocol;
- the one-to-one per-device session-manager state machine is canonical in [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](ONE_TO_ONE_SESSION_MANAGEMENT.md) and verified by `V-003`;
- ordinary account-private preference conflicts may use typed record-specific convergence, but authority/security state may never use LWW/ordinary CRDT merge;
- new-device bootstrap requires the explicit account-authorized enrollment ceremony and non-suppressible device-link notice;
- HistoryBundle chunking/resume is a transport/storage detail and never changes history provenance or grants authority;
- offline revocation propagation is fail-closed for new governance/secret distribution until a current authenticated control head is reconciled.

## Authority convergence

Multi-device chat/history convergence and account-control convergence are distinct protocols. `AccountControlState` is not merged with CRDT/LWW rules. An offline/restored device must reconcile the authenticated control-state chain/head before it can exercise governance authority.

New messaging-device enrollment may be lighter-weight; promotion to control-device status is constitutional.
## 2026-09-22 hardening — session management is separate from self-sync

[`ONE_TO_ONE_SESSION_MANAGEMENT.md`](ONE_TO_ONE_SESSION_MANAGEMENT.md) is now canonical. Account-private preference/history sync does not solve per-correspondent cryptographic session selection. Every eligible remote DeviceIdentity may require its own active/inactive ratchet/session lifecycle, crossed-initiation handling, stale-device handling and logical-message deduplication.

Authorization comes from `AccountControlState`, never from a provider's bare device list.

## 2026-09-23 multi-device resolution

New-device bootstrap uses an explicit grant plus same-account authenticated E2EE sync. Immutable conversation events merge by EventID union; constitutional/group authority is never LWW/CRDT merged. One-to-one fanout is governed by the explicit per-device session manager selected in Q-050; verification gate `V-003` covers races/reset/dedupe/persistence.

## 2026-09-24 device-link visibility (R-166)

Every new device enrollment produces an authenticated, non-suppressible notice on every other active device of the account, derived from AccountControlState. Settings → Devices lists every device with enrollment time and last-seen class. A conforming client cannot complete a link silently. This is the primary defense against proximate-adversary device linking ([`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](../client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md) §2).
