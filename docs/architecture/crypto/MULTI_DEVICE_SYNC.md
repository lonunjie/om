# Multi-Device Sync and History — architecture direction

## No master phone

Every authorized device is a first-class cryptographic endpoint. The account is not “the phone plus mirrors.”

## Device state

Each DeviceIdentity has independent keys, capabilities, and revocation state. AccountState authenticates the current device set.

## Delivery

A sender encrypts/delivers to the recipient's eligible devices according to the selected 1:1/group protocol. Relays hold per-device/opaque-capability envelopes.

## Account-private sync

Preferences and account-local state that should appear on multiple devices are synchronized through a separate encrypted account-private mechanism, not by making chat events globally authoritative for every UI preference.

Candidate design direction: an account self-group or equivalent authenticated device-sync channel carrying versioned sync records.

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

## Open details

- exact self-sync protocol;
- conflict semantics for settings/account-private state;
- new-device bootstrap authentication;
- history bundle chunking/resume;
- offline revocation propagation constraints.

## Authority convergence

Multi-device chat/history convergence and account-control convergence are distinct protocols. `AccountControlState` is not merged with CRDT/LWW rules. An offline/restored device must reconcile the authenticated control-state chain/head before it can exercise governance authority.

New messaging-device enrollment may be lighter-weight; promotion to control-device status is constitutional.
