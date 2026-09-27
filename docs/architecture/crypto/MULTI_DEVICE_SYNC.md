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

- the exact account-private sync record transport/codec is an implementation choice behind the authenticated E2EE sync interface;
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
