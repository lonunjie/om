# OM Identity Continuity, Aliases, and Human Confirmation

- **Status:** Phase 0 working design
- **Date:** 2026-08-18
- **Scope:** Gate A — identity, aliases, account replacement, phone-number churn, and social confirmation

## Core observation

Real users frequently abandon accounts, lose SIMs, replace phone numbers, and create new accounts instead of recovering old ones. A contact may accumulate multiple historical phone numbers, and a recycled number can later belong to an unrelated person.

OM therefore MUST model **people and cryptographic account identities separately from phone numbers and usernames**.

## Identity hierarchy

```text
PersonRecord                    local human-facing concept
  │
  ├── AccountIdentity A         cryptographic identity, historical
  ├── AccountIdentity B         cryptographic identity, current
  │
  ├── AliasClaim: @alice
  ├── AliasClaim: tel:+63...
  ├── AliasClaim: email:...
  └── Profile history           names/photos used for recognition
```

A `PersonRecord` is local contact state. It is not a globally authoritative identity object and does not need to be synchronized publicly.

An `AccountIdentity` is the cryptographic identity used for authentication and continuity.

An `AliasClaim` is a replaceable locator/discovery binding. A phone number, username, email address, QR/invite, or future directory identifier MUST NOT be treated as the person itself.

## Phone numbers are recyclable aliases

OM MUST assume that a phone number can stop belonging to Alice and later be assigned to Bob.

Consequences:

1. Same phone number MUST NOT imply same OM account.
2. A new cryptographic account appearing at a previously known number MUST NOT silently replace the old contact identity.
3. Phone alias bindings include an account identity and claim epoch/version.
4. Clients may mark old aliases as `historical`, `stale`, `superseded`, or `reassigned` without deleting the PersonRecord.
5. Historical numbers are hidden from normal UI by default to prevent contact cruft, but retained locally when useful for provenance or legacy SMS routing.
6. A phone number may be used for discovery if the user enables it, but established OM relationships route through cryptographic identity and private relationship capabilities, not through the phone number.

Example:

```text
2026
+63 917 111 1111 -> Alice / Account A

2027
Alice abandons that SIM.

2028
+63 917 111 1111 -> Carol / Account C

OM result:
Alice remains Alice.
Carol is a new person/account candidate.
The matching digits do not transfer trust.
```

## Contact cruft policy

Normal contact UI should present **one human record** rather than a pile of historical aliases.

```text
Alice Reyes
[profile photo]

Current OM identity: Account B
Current reachable aliases:
  @alice
  +63 9xx xxx xxxx

Historical aliases: 3   [show]
```

The client should prefer current signed profile/identity state. Historical phone numbers are collapsed unless the user needs them for SMS, forensics, or manual contact cleanup.

OM should not automatically mutate the operating system address book without explicit permission. It can maintain a cleaner OM-local PersonRecord and later offer safe export/cleanup tools.

## Identity continuity levels

OM distinguishes **cryptographic continuity** from **human belief**.

### Level 1 — Direct continuity proof (strongest normal path)

The old account retires itself and names the new account:

```text
Account A --AccountRetirement(reason_class = moved, successor_account_id = B)--> Account B
```

Existing contacts can automatically carry the PersonRecord forward and show a simple message such as "Alice moved to a new OM account."

This is the **only** cryptographic continuity proof that clients render as "moved" (ADR-0127). It must be authorized
under Account A's parent state like any retirement ([`ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md`](ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md) §2 rule 1): a majority
of A's controllers, where a RecoveryPolicy is one vote that alone acts only after the delay with no veto. A single
device, even a trusted one, cannot produce it, and A stops existing as it does, so A and B never coexist. Clients
reject any other successor pointer.

### Level 2 — Pre-authorized social recovery (strong, same account)

Alice may optionally designate recovery guardians and a threshold while Account A is healthy.

Example:

```text
Guardians: Bob, Carol, Diego, Eva, Fran
Threshold: 3 of 5
```

Guardians hold Shamir shares of a second RecoveryRoot ([`RECOVERY_AND_BACKUP_POLICY.md`](RECOVERY_AND_BACKUP_POLICY.md) §10.2). If Alice loses all devices, any 3
guardians can help her start a RecoveryRequest for **Account A itself** on a new device. It goes through the normal
activation rules (§10.3: delay, veto by any live controller), and Account A keeps its AccountID. Guardians never bind
a new Account B as a successor; ~~any 3 authorized guardians can co-sign a recovery/continuity statement binding
Account B as the successor~~ was removed by ADR-0127, because colluding guardians could otherwise redirect every
relationship while the owner still held working devices.

Guardians should not need access to Alice's message plaintext or master secrets.

### Level 3 — Direct contact attestation (useful evidence, not automatic authority)

If no continuity or pre-authorized recovery proof exists, Bob may confirm locally:

```text
"I know this new account is Alice."
```

Bob can cryptographically sign an attestation that Account B is believed to represent the same human as Alice/Account A.

Other users may optionally receive direct attestations from mutual contacts and see an explanation such as:

```text
New account for Alice
Confirmed by 3 people you already know
```

This is evidence, never equivalent to an Account-A continuity proof, even when the attesters are Alice's recovery guardians.

### Level 4 — Manual local merge

With no cryptographic or social evidence, the user may manually attach the new AccountIdentity to the existing local PersonRecord.

OM should preserve the fact that continuity was manually asserted rather than cryptographically proven.

## Why not a global Web of Trust

OM should NOT make a PGP-style transitive Web of Trust the default identity mechanism.

Problems include:

- normal users do not understand trust depth or certification semantics;
- transitive trust can turn "I trust Bob" into unintended assertions about strangers;
- Sybil accounts can manufacture apparent social support;
- publishing trust edges leaks social-graph metadata;
- compromise or coercion of highly connected introducers has disproportionate impact;
- trust scores invite social-ranking dynamics that are unrelated to messaging.

OM should instead use **bounded, direct, purpose-specific trust**:

- constitutionally authorized moved-account retirements;
- pre-authorized threshold recovery guardians (same AccountID);
- direct attestations from contacts;
- manual local confirmation.

Do not compute a universal reputation/trust score for people.

## Privacy rule for social confirmation

Attestations and guardian relationships SHOULD be private by default.

The system must not require publication of "Alice trusts Bob" or a global social graph. Proofs may be exchanged directly, encrypted to the relevant parties, or disclosed only when needed for recovery/continuity.

## Profile photos and human recognition

Profile names and photos are important recognition aids and should be prominent in the UI, but they are not authenticators because they can be copied.

The security layer tracks cryptographic identity continuity invisibly. A normal user mostly sees the profile they recognize; OM surfaces stronger warnings only when identity continuity actually breaks.

## UX examples

### Number recycled

```text
+63 917 111 1111 is now associated with a different OM account.

This will NOT replace Alice.

[View new account]  [Ignore]
```

### Strong account transfer

```text
Alice moved to a new OM account.
Your conversation and contact stay the same.
```

### Socially confirmed replacement

```text
Alice may be using a new account.
Confirmed by Bob, Carol, and Diego — people you already know.

[This is Alice]  [Not sure]
```

High-assurance users can expand these notices to inspect fingerprints, signatures, guardian proofs, timestamps, and device history.

## Gate-A invariants added by this design

1. OM remembers people independently of aliases.
2. Phone numbers are recyclable discovery aliases, never root identity.
3. Reassignment of an alias cannot transfer cryptographic trust.
4. Account replacement can preserve human continuity without pretending a lost key still exists.
5. Direct cryptographic continuity (a constitutionally authorized `AccountRetirement(moved)`) is preferred over social confirmation.
6. Social recovery recovers the same AccountID through the pre-authorized guardian set; it never names a successor.
7. Ad-hoc friend attestations are evidence, not automatic authority.
8. No global social trust score is required.
9. Historical aliases are retained as provenance but hidden from mainstream UI by default.
10. Security detail is progressive: normal users see people; high-assurance users can inspect the proof chain.

## Disposition of former continuity questions — final design closure

These items are no longer architecture-open. Their current dispositions are:

- **`ContinuityProof` schema/signature:** resolved by ADR-0127. The only cryptographic "moved" proof is the constitutionally authorized `AccountRetirement(reason_class=moved)` object in [`account.cddl`](../../../spec/cddl/account.cddl).
- **Guardian identity form:** the protocol does not create a privileged `GuardianPrincipal`. Recovery authority comes from the current `RecoveryPolicy` commitment and the proof for its selected method. A guardian may be represented locally as a contact, account, hardware custody route or offline holder, but that representation never changes its vote weight or grants ordinary account authority.
- **Guardian replacement/rotation:** rotate the RecoveryPolicy/RecoveryRoot under an authorized AccountControlState transition and increment the recovery rotation epoch. Proofs/requests bound to an obsolete policy generation do not acquire authority against a newer validated head.
- **Stale guardian policy/lockout:** clients reconcile the current authenticated AccountControlState head before accepting recovery authority. Backups or old guardian material cannot resurrect an obsolete RecoveryPolicy; recovery remains deliberately fail-closed if current policy cannot be proven.
- **Direct attestations from mutual contacts:** optional recognition evidence only. Implementations may request/share them privately, but W0 correctness and account authentication never depend on a social-graph query or attestation count.
- **Phone/alias expiration and re-verification:** alias ownership/expiry is provider-namespace policy expressed through expiring signed discovery descriptors/claims. Expiry or reassignment affects discovery only; it never rewrites an accepted relationship or transfers cryptographic trust.
- **Privacy-preserving phone discovery:** no mandatory phone-number directory exists in W0. A future provider may offer a privacy-reviewed mechanism, but clients may omit phone discovery entirely and no such provider becomes identity authority.
- **SMS mapping after number reassignment:** this is local PersonRecord/transport-adapter UX. A recycled number starts as a new untrusted candidate; historical SMS provenance may remain attached locally but cannot transfer OM AccountIdentity continuity.

## Account-control continuity refinement — 2026-09-21 21:32

Identity continuity now explicitly separates `AccountControlState` from chat events. Control state is a monotonic authenticated hash chain. Quarantine, permanent revocation, controller promotion, RecoveryPolicy, minimum CryptoProfile and control-key epochs are state transitions under constitutional authorization.

A messaging device is not automatically a control device. Promotion to control status is itself constitutional, preventing one compromised device from adding attacker devices and fabricating a quorum.

Backups restore historical data but never make historical control state authoritative. See [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](ACCOUNT_CONTINUITY_AND_CONTROL.md) and [`RECOVERY_AND_BACKUP_POLICY.md`](RECOVERY_AND_BACKUP_POLICY.md).

## CONFORMANCE-IDENTITY

The following are normative pass/fail cases for the registered W0 conformance suite. Passing these cases is evidence; it does not by itself authorize a scope.

| ID | Case | Pass condition |
|---|---|---|
| ID-01 | provider/Home changes | AccountID and valid ACS lineage remain unchanged |
| ID-02 | device add/quarantine/revoke | only parent-state-authorized transitions validate; revoked DeviceID never re-used |
| ID-03 | recovery request withheld from a verifier | verifier waits its own full activation interval before accepting recovery-alone successor |
| ID-04 | conflicting ACS branches | client enters conflict/freeze behavior and never silently chooses provider ordering |
| ID-05 | account moved | only valid AccountRetirement continuity proof produces `moved` identity UX |
