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

Canonical construction:

```text
AccountID = 256 bits of cryptographically secure random data
```

`AccountID` deliberately contains no public key, algorithm identifier, provider, domain, username, device identifier, or other structure whose replacement would force an identity migration. Encoding/domain rules are frozen separately from cryptographic credential choices.

### AccountControlState

Account authority is defined by [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](ACCOUNT_CONTINUITY_AND_CONTROL.md) and the canonical `AccountControlState` object. There is no separate legacy account-authority object.

Conceptually:

```text
AccountControlState
  account_id
  generation
  previous_state_hash
  active_control_devices[]
  quarantined_control_devices[]
  messaging_devices[]
  permanently_revoked_devices[]
  constitutional_policy
  recovery_policy_commitment
  minimum_crypto_profile
  control_key_epochs
  authorization_signatures[]
```

State transitions are authorized by the **parent AccountControlState**, never by a larger timestamp/generation alone.

OM SHOULD NOT copy one universal long-lived root private key onto every device merely for convenience. Authority is expressed through currently authorized control credentials and configured recovery policy.

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
```

Each device has independent cryptographic material and can be revoked independently. Revocation is recorded only in
AccountControlState (`permanently_revoked_devices`), never as a timestamp on the device record. The wire object is
`DeviceCredential` in [`account.cddl`](../../../spec/cddl/account.cddl).

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

A directory maps an alias to the recipient's signed `ContactDescriptor` ([`discovery.cddl`](../../../spec/cddl/discovery.cddl)), which names an
alias-specific intro identity and a request key. A directory record carries **no AccountID and no account state
hash**: both would make a public, enumerable alias-to-account map (see §"Recovery/control boundary" below).

```text
DirectoryEntry
  alias
  signed ContactDescriptor (or its digest + locator)
  claim_epoch
  expiry
  directory_signature
```

The directory is not the account authority.

Directory mappings require append-only transparency or an equivalent reviewed anti-equivocation mechanism. The logged
binding is `alias -> intro identity public key (+ descriptor digest)` ([`KEY_TRANSPARENCY_AND_DIRECTORY.md`](../discovery/KEY_TRANSPARENCY_AND_DIRECTORY.md)). The
AccountID is bound to the intro identity only inside the accepted relationship, by an `IntroIdentityBinding`
(ADR-0126).

### IntroductionCapability

Public discovery exposes only a constrained first-contact capability:

- small payload;
- strict rate/size limit;
- no unsolicited large files;
- no unsolicited calls by default;
- rotatable/revocable;
- abuse policy may evolve independently.

The first-contact message is a `MessageRequest` sealed to the recipient's request key; the IntroGate never learns the
sender's AccountID or device ([`INVITES_INTROGATE_AND_SPAM.md`](../discovery/INVITES_INTROGATE_AND_SPAM.md) §5.1). After acceptance, contacts verify the
`IntroIdentityBinding`, establish PQXDH sessions to each other's devices and exchange private relationship delivery
capabilities.

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

The only cryptographic continuity proof is `AccountRetirement(reason_class = moved, successor_account_id)`,
authorized under the old account's parent state (majority, with a RecoveryPolicy counting as one vote under the delay
and veto rules; ADR-0127). Guardian or recovery-root recovery keeps the **same** AccountID and needs no continuity
proof.

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

Every AccountControlState change follows the normative authorization table in
[`ACCOUNT_CONTINUITY_AND_CONTROL.md`](ACCOUNT_CONTINUITY_AND_CONTROL.md) §4.1 (ADR-0127). In short: one control device links a messaging device;
promotion, permanent revocation of a control device, recovery-policy change and retirement are constitutional
(majority of the parent state's controllers, with recovery as one vote); quarantine is one controller and
reversible. Changes outside AccountControlState:

- create account: local genesis device;
- update profile: any active device;
- add/remove alias: an active device plus the alias provider's proof.

High-assurance users may raise the constitutional threshold (`constitutional_policy`), never lower it below the
majority default without a majority.

## Lifecycle

### New account

```text
install
 -> generate genesis device keys
 -> create random AccountID + AccountControlState v1
 -> create signed DiscoveryCard/ProfileDocument
 -> optional alias
 -> establish default recovery custodian (ADR-0111; synced passkey, else recovery key)
```

No password, email, or phone number is required by the OM protocol.

### Add device

```text
new device request
 -> linking ceremony with an active control device (or a completed recovery, §10.3)
 -> AccountControlState advances (authorization per ACCOUNT_CONTINUITY §4.1)
 -> account/contact state sync
 -> secure groups enroll the new device according to group policy
```

### Revoke device

```text
AccountControlState revokes device
 -> stop new per-device delivery
 -> remove from secure groups
 -> rotate affected delivery/session state
```

### Phone/username change or reuse

The alias changes; the AccountID does not.

A different cryptographic account appearing at a recycled phone number is a new identity unless a valid continuity proof says otherwise.

### Total loss with valid recovery

The committed RecoveryPolicy can preserve the AccountID lineage and authorize new devices, through a RecoveryRequest that completes only after the activation delay with no veto, or immediately with a controller co-sign ([`RECOVERY_AND_BACKUP_POLICY.md`](RECOVERY_AND_BACKUP_POLICY.md) §10.3).

### Total loss without valid recovery

Create a new AccountID. Friends may attach it to the existing local PersonRecord, but OM preserves that continuity was human-asserted rather than cryptographically proven.

## Recognition and mutual contacts

Name, photo, mutual contacts, and familiar aliases help recognition but do not prove identity.

OM MUST NOT compute a universal trust score.

A v1 implementation may omit server-computed mutual counts entirely rather than centralize the social graph. If mutual-contact context is added later, it should minimize graph disclosure and never become an authenticator.

## Gate-A implementation work remaining

1. choose reviewed signature/hash/KEM primitives;
2. canonical serialization;
3. exact AccountControlState authorization/signature rules;
4. recovery cryptographic construction;
5. directory transparency mechanism;
6. privacy-preserving phone/email discovery if shipped;
7. golden test vectors and adversarial state-transition tests.

## Gate-A exit criterion

Independent implementations can create/verify accounts, add/revoke devices, migrate aliases/directories/providers, recognize recycled aliases, validate recovery/continuity, and preserve relationships using only public specifications and test vectors.


## 2026-09-21 canonical addendum — algorithm agility and routing privacy

The following supersedes any implication that AccountID is permanently defined by one hash/public-key algorithm:

- AccountID is a stable provider- and algorithm-independent account-lineage identifier.
- Account/device credentials evolve through authenticated epochs and can migrate to new algorithms/CryptoProfiles.
- AccountID is never used as a relay queue or media routing address.
- Accepted relationships use private opaque delivery capabilities.
- CryptoProfile migration should be invisible to ordinary users and must advance downgrade protections.

See [`CRYPTOGRAPHY_AND_AGILITY.md`](../crypto/CRYPTOGRAPHY_AND_AGILITY.md) and [`RELAY_AND_DELIVERY_ARCHITECTURE.md`](../delivery/RELAY_AND_DELIVERY_ARCHITECTURE.md).

## Recovery/control boundary

`AccountID` does not imply that every linked device has equal governance authority. Device credentials carry explicit roles/capabilities. Account-control changes are authenticated by `AccountControlState`; recovery is a separate policy that is **on by default** (ADR-0111): an automatically chosen custodian, a delayed and vetoable activation, and one vote. `none` is an explicit user choice.

Discovery/contact sharing may use alias/share-specific introduction identity so the stable AccountID need not be public before acceptance. Stable identity is proven/bound inside the accepted relationship.

## Local IdentityProfile is not a protocol identity

`IdentityProfile` is a client-local isolation/container concept for hosting multiple independent AccountIDs in one application. It MUST NOT appear as a global person/device identifier, and Core must not create an automatic public binding among profiles.

### First-contact binding state

The accepted relationship has an explicit intermediate `ACCEPTED_UNBOUND` state. Recipient and requester exchange
`IntroIdentityBinding` in that order after acceptance; stable-account session fanout, contact auto-merge and durable
AccountID attribution begin only after both bindings verify (`ACTIVE`).
