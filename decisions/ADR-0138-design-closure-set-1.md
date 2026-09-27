# ADR-0138 — Design closure set 1: legacy, privacy profiles and provider-scoped administration

- **Status:** Accepted architecture design; no engineering authorization implied
- **Date:** 2026-09-26
- **Author:** GPT-5.6 Sol architecture-closure pass
- **Closes:** A-030, A-031, A-036, A-054
- **Amends:** ADR-0105, ADR-0111, ADR-0126, ADR-0127, ADR-0128, ADR-0133, ADR-0136, ADR-0137
- **Scope:** design/specification only; no TLC, Rust, Wasm or product-build evidence is claimed

## Context

Four later-wave architecture questions remained open after the core-kernel equivalence pass. A cross-boundary review of those questions also exposed two W0 design contradictions that should not be handed to engineering:

1. routine Home administration used `device_auth`, forcing providers to receive stable DeviceCredential/ACS material even though provider roles are supposed to carry service state without gaining account authority; and
2. the recovery architecture said a restricted recovering device can fetch its encrypted `RecoveryRecord`, while Home API v1 had no fetch operation and indexed writes by an account-like reference.

The decisions below prefer least authority, explicit residual leakage, replaceability and fail-closed behavior. They do not create a new mandatory operator or a new cryptographic primitive.

## Decision

### 1. A-030 — LegacyPolicy is verifier-local, threshold-signed and terminally narrow


> **ADR-0141 criticality amendment:** `om.acs.legacy_policy` is a **critical** extension. An implementation that does not understand it rejects the containing ACS; W0 still rejects `authorization_kind=legacy_policy`.

Legacy support remains **off by default** and is not enabled in W0. A future profile that enables it uses the critical AccountControlState extension `om.acs.legacy_policy`. The committed policy contains:

- a version;
- a set of dedicated legacy guardian verification credentials and a threshold;
- `owner_inactivity_wait_seconds`;
- `minimum_contest_window_seconds`; and
- optionally one pre-designated successor AccountID.

Legacy guardians are a separate authority class. Their credentials do not count as AccountControlState controller votes, RecoveryPolicy votes or messaging credentials.

OM does not pretend that a provider timestamp can prove that an owner has been inactive. The wire claim is the signed `AccountRetirement(reason_class=legacy, authorization_kind=legacy_policy)` itself; `LegacyRetirementPending` is verifier-local derived state, not a second top-level wire object. A valid threshold-signed legacy claim starts a **verifier-local dormancy interval**. Each verifier records its own first observation using monotonic elapsed time. Provider timestamps, wall clocks supplied by guardians and statements such as “the owner was inactive for N days” cannot shorten that interval.

After the full committed inactivity interval has elapsed without a valid live-controller veto or newer owner-authorized account activity, the claim may enter `LegacyRetirementPending`. It then remains non-terminal for at least the committed minimum contest window. Any controller authorized in the parent state may veto during either interval. A quarantined parent controller may use only its narrow quarantine/freeze challenge credential to veto this irreversible legacy action; that signature grants no ordinary controller authority.

Only after both intervals may the verifier accept terminal `AccountRetirement(reason_class=legacy)`, and only if the guardian threshold signatures remain valid against the committed LegacyPolicy. Legacy authority may retire, carry a memorial note and use the policy's pre-designated successor. It cannot enroll/revoke devices, change policy, recover history, send as the account, choose a new successor or mutate any other constitutional field.

This intentionally favors false-negative retirement over false-positive death/incapacity claims. A malicious guardian quorum can start the waiting process, but cannot bypass the owner's local veto opportunity or convert legacy credentials into ordinary account authority.

### 2. A-031 — superseded by ADR-0140: the privacy design ships in AsyncKeyBundle v1

**2026-09-27 amendment:** ADR-0140 supersedes this section's defer-to-v2 cut. Before any deployed OM v1 client exists, object type 9/version 1 is re-frozen without `acs_generation` or `acs_state_hash`; numeric keys 3/4 are retired/reserved and never reused. The fetcher validates the subject DeviceCredential against the relationship-pinned trusted peer ACS, and `subject_device_id` remains an explicitly disclosed stable per-device correlator. No replacement stable account-wide pseudonym is introduced.

ADR-0140 also adds recipient-side signed-prekey generation anti-rollback/equivocation state and binds one-time prekeys to DeviceID + CryptoProfile + bundle generation + provider pool + kind + key id + public key.

### 3. A-036 — cryptographic deniability for groups is not an OM v1 property

OM v1 groups use MLS authentication and therefore do **not** promise cryptographic deniability of group sender authentication. This is a deliberate, documented non-goal rather than an unresolved architecture question.

A future deniable-group construction would be a separate research CryptoProfile/conversation mode under absolute rule 3. It must not weaken or ambiguously reinterpret the authentication guarantees of ordinary MLS Rooms. Until such a separately reviewed construction exists, UI/privacy claims must not describe group messages as deniable.

### 4. A-054 — sealed-sender delivery is a later profiled outer envelope, not a W0 retrofit

W0 retains its disclosed sender-device and ratchet-header exposure. A later CryptoProfile may enable the canonical design in [`SEALED_SENDER_DELIVERY_PROFILE.md`](../docs/architecture/delivery/SEALED_SENDER_DELIVERY_PROFILE.md):

- each accepted relationship/device route has a rotating, provider-distinct `DeliveryWrapKey` and opaque key id delivered inside authenticated relationship state;
- the sender HPKE/encrypts the complete ordinary OM 1:1 envelope (including sender `IK`, ratchet header and inner ciphertext) to that recipient delivery key;
- the outer envelope carries only format/profile/key-id/routing-expiry material needed by the recipient and Home;
- the Home routes on the existing opaque deposit capability and does not receive sender AccountID, DeviceID, identity key, DR header or ConversationID from the outer envelope;
- sender authentication remains an end-to-end inner property checked after outer decryption under the mandatory `RelationshipContext`; and
- anti-spam/admission remains at IntroGate/referral/capability boundaries rather than reintroducing sender identity at the Home.

No W0 decoder is told that W0 already has sealed sender. The new format requires an explicit CryptoProfile/object-version transition with normal anti-downgrade rules.

### 5. Routine provider administration uses opaque resource capabilities, not AccountControl credentials

Home/provider administration is changed from `device_auth` to **provider-resource capabilities**. A provider receives only a high-entropy bearer capability scoped to the resource and operation family it already stores. At minimum the classes are:

- `relay_admin_cap` — mint/revoke deposit capabilities for one mailbox/queue family;
- `directory_publish_cap` — publish/replace one descriptor namespace entry;
- `key_publish_cap` — publish/rotate one device's key-bundle pool;
- `recovery_record_write_cap` — write one encrypted RecoveryRecord; and
- `blob_repair_cap` — repair the explicitly scoped BlobLocationRecord set.

Capabilities are independently random/derived per provider and resource, independently rotatable, never AccountID, never a DeviceCredential, and never accepted as AccountControlState or RecoveryPolicy authority. A provider can still correlate repeated operations on the **same provider resource**; OM does not mislabel that as unlinkable. The design prevents that local handle from becoming a cross-provider/global device/account credential.

Canonical objects remain end-to-end signed. Providers may perform cheap syntactic/signature checks where they have the public material, but clients are authoritative for AccountControlState/DeviceCredential validity. A stale or malicious provider write therefore cannot make an invalid descriptor/bundle/control object valid to clients.

A provider capability stolen from a revoked device may allow service-level nuisance within its exact scope until rotated. It cannot sign as the account, decrypt E2EE content, promote controllers or change canonical account state. Surviving controllers rotate provider-resource capabilities as part of device-compromise cleanup where available.

### 6. RecoveryRecord access is explicit, opaque and recoverable without AccountID lookup

A RecoveryStore exposes both put and get operations for the encrypted RecoveryRecord. It does not index the record by AccountID. **ADR-0141 amends this section:** the record is authenticated advisory state with a monotonic record generation, recovery epoch and last-known authenticated ACS head; stale snapshots cannot lower a surviving client high-water mark, and recovered routes/capabilities are revalidated before reuse.

For each RecoveryStore scope, clients derive independent high-entropy read and write capabilities from `RecoveryRoot` using domain-separated HKDF labels and the store's public `recovery_scope_id`. The provider indexes the record by a hash of the read capability and receives the corresponding bearer capability only when servicing the operation. The read and write capabilities are distinct.

Consequences:

- a restricted recovering device that reconstructs `RecoveryRoot` and knows the RecoveryStore endpoint/scope can fetch its encrypted RecoveryRecord without first regaining account authority;
- the RecoveryStore cannot map the opaque record capability to AccountID by protocol design;
- possession of the read/write capability grants only ciphertext read/write for that record and never counts as a recovery/controller vote; and
- loss of the RecoveryStore can lose provider/relationship convenience metadata but cannot destroy AccountID recovery authority, which remains in RecoveryRoot/RecoveryPolicy.

`RecoveryRequest` fan-out is separate. The signed request contains AccountID and therefore a mailbox Home that participates in fan-out can observe that AccountID and a recovery attempt for that operation. W0 metadata descriptors must say so; no `no_by_design` claim survives on that path.

## Consequences

- A-030, A-031, A-036 and A-054 are resolved as architecture questions.
- W0 sealed-sender and W0 provider/account unlinkability are not falsely claimed; the later profiles have concrete compatibility/security targets.
- Provider roles lose an unnecessary stable DeviceCredential/ACS authentication surface for routine service administration.
- RecoveryRecord retrieval becomes specified and does not require an account-identified lookup.
- Metadata accounting is amended: the former `device_auth` stable-device observations disappear where the provider-resource capability is the only auth material; mailbox recovery fan-out gains explicit `stable_account_id` and `recovery_attempt` exposure for that operation.
- This ADR does not pass any V-* gate, authorize engineering, or replace the independent review obligations created by earlier ADRs.
