# OM Identity Protocol — Gate A Frozen Direction

- **Status:** Phase 0 architecture decision draft; conceptual direction frozen
- **Date:** 2026-08-18
- **Scope:** Account identity, devices, profiles, aliases, discovery, recovery, continuity, verification, and local human contact records

## Decision summary

OM uses a **passwordless-by-default, device-first identity model**. The protocol identity is cryptographic. Names, photos, usernames, phone numbers, email addresses, domains, directories, relays, and storage providers are replaceable claims/services around that identity.

Normal users can create an OM identity with only a display name and photo. High-assurance users get more control without using a different or stronger protocol.

## Frozen invariants

1. A phone number, email address, username, profile photo, domain, directory, relay, or storage provider is never the root identity.
2. `PersonRecord` is local human contact state and is not globally authoritative.
3. Recycled phone numbers and reused aliases cannot inherit cryptographic trust from prior holders.
4. Account state is versioned and hash-linked.
5. Device keys are independent and individually revocable.
6. Recovery preserves an account only through authority committed before loss; otherwise a truly new account is created.
7. Recognition hints are not authentication.
8. Direct continuity and pre-authorized recovery outrank social attestations.
9. No global trust score or transitive Web of Trust.
10. Directory resolution is replaceable and needs anti-equivocation/transparency.
11. Security detail is progressive: invisible by default, inspectable on demand.

## Core protocol objects

### AccountID

`AccountID` is a stable, provider-independent identifier for one cryptographic account lineage.

Recommended construction shape:

```text
AccountID = HASH(
  "om-account-v1" ||
  genesis_device_signing_public_key ||
  random_nonce
)
```

Exact hash, encoding, and cryptographic primitives are deferred to the Crypto ADR.

### AccountState

```text
AccountState
  account_id
  sequence
  previous_state_hash
  authorized_devices[]
  recovery_policy_ref
  profile_ref
  capability_policy
  created_at
  signatures[]
```

State transitions are authorized by the prior state.

OM SHOULD NOT copy one universal long-lived root private key onto every device merely for convenience. Authority is expressed through the currently authorized device/recovery policy.

### DeviceIdentity

```text
DeviceIdentity
  device_id
  account_id
  signing_public_key
  key_agreement / encryption public material
  capabilities
  created_at
  authorization
  revoked_at?
```

Each device has independent cryptographic material and can be revoked independently.

### ProfileDocument

OM separates:

- **DiscoveryCard** — minimal user-selected information shown before acceptance, typically name and optional avatar/photo.
- **ContactProfile** — richer profile shared after a relationship is accepted.

### AliasClaim

Aliases are locators/provenance claims:

```text
@alice
tel:+63917...
email:alice@gmail.com
domain:alice@example.com
```

Claims include issuer/directory, AccountID binding, epoch/version, proof, and expiration when applicable.

Phone-number claims are time-bounded/re-verifiable claims because numbers are recyclable.

### DirectoryEntry

```text
DirectoryEntry
  alias
  account_id
  account_state_hash
  introduction_endpoint
  claim_epoch
  expiry
  directory_signature
```

The directory is not the account authority.

Directory mappings require append-only transparency or an equivalent reviewed anti-equivocation mechanism.

### IntroductionCapability

Public discovery exposes only a constrained first-contact capability:

- small payload;
- strict rate/size limit;
- no unsolicited large files;
- no unsolicited calls by default;
- rotatable/revocable;
- abuse policy may evolve independently.

After acceptance, contacts establish private relationship delivery capabilities.

### RelationshipBinding

An accepted relationship binds known OM identities and private delivery state. It survives changes to usernames, phone numbers, directories, relays, and storage.

### RecoveryPolicy

Supported architecture classes:

1. trusted-device recovery;
2. encrypted user-selected RecoveryStore;
3. recovery key / QR / phrase;
4. hardware security key;
5. pre-authorized threshold guardians;
6. combinations of the above.

OM never requires a provider-held plaintext master decryption key.

### ContinuityProof

A continuity proof links an old AccountID to a successor AccountID only when a truly new account must be created.

Strong proof comes from:

1. old account authorization;
2. pre-authorized recovery authority.

Friend attestations and manual merges are human evidence, not equivalent cryptographic continuity.

### VerificationAttestation

Verification is typed evidence, not `verified=true`.

Examples:

- domain control;
- organization authorization;
- direct in-person verification;
- external verifier attestation;
- cryptographic account continuity.

The UI states what was verified.

### OrganizationIdentity and RoleClaim

Organizations can own a cryptographic organization identity, prove domain control, and authorize roles such as:

```text
support@example.com
alice@example.com
bot@example.com
```

Removing the role does not destroy the employee's personal AccountID.

### PersonRecord — local/private

```text
PersonRecord: Alice
  current AccountID
  historical AccountIDs
  current aliases
  historical aliases
  local notes
  recognition history
  continuity evidence
```

A PersonRecord is how the user remembers a human. It is not a global claim that two accounts are the same person.

## Default authority policy

Normal mode favors low friction:

- create account: local genesis device;
- add device: one trusted authorized device or valid recovery flow;
- revoke device: authorized device, propagated quickly;
- update profile: authorized device;
- add/remove alias: authorized device plus alias-provider proof;
- recovery-policy replacement: explicit confirmation under current account authority.

High-assurance users may require M-of-N approvals for selected critical transitions.

## Lifecycle

### New account

```text
install
 -> generate genesis device keys
 -> create AccountID + AccountState v1
 -> create signed DiscoveryCard/ProfileDocument
 -> optional alias
 -> progressively offer recovery
```

No password, email, or phone number is required by the OM protocol.

### Add device

```text
new device request
 -> trusted device/recovery approval
 -> AccountState advances
 -> account/contact state sync
 -> secure groups enroll the new device according to group policy
```

### Revoke device

```text
AccountState revokes device
 -> stop new per-device delivery
 -> remove from secure groups
 -> rotate affected delivery/session state
```

### Phone/username change or reuse

The alias changes; the AccountID does not.

A different cryptographic account appearing at a recycled phone number is a new identity unless a valid continuity proof says otherwise.

### Total loss with valid recovery

The committed RecoveryPolicy can preserve the AccountID lineage and authorize new devices.

### Total loss without valid recovery

Create a new AccountID. Friends may attach it to the existing local PersonRecord, but OM preserves that continuity was human-asserted rather than cryptographically proven.

## Recognition and mutual contacts

Name, photo, mutual contacts, and familiar aliases help recognition but do not prove identity.

OM MUST NOT compute a universal trust score.

A v1 implementation may omit server-computed mutual counts entirely rather than centralize the social graph. If mutual-contact context is added later, it should minimize graph disclosure and never become an authenticator.

## Gate-A implementation work remaining

1. choose reviewed signature/hash/KEM primitives;
2. canonical serialization;
3. exact AccountState authorization/signature rules;
4. recovery cryptographic construction;
5. directory transparency mechanism;
6. privacy-preserving phone/email discovery if shipped;
7. golden test vectors and adversarial state-transition tests.

## Gate-A exit criterion

Independent implementations can create/verify accounts, add/revoke devices, migrate aliases/directories/providers, recognize recycled aliases, validate recovery/continuity, and preserve relationships using only public specifications and test vectors.
