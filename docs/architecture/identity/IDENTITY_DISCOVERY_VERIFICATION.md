# OM Identity Discovery, Public Names, and Verification

- **Status:** Phase 0 working design
- **Date:** 2026-08-18
- **Scope:** Gate A — frictionless discovery, mutual-contact context, public figures, businesses, domain identities, impersonation resistance, and verification semantics

## Design goal

OM should let ordinary users find and recognize people with nearly the same ease as mainstream social apps while keeping identity independent of profile photos, phone numbers, usernames, directories, or OM-operated infrastructure.

The user experience may be simple:

```text
[photo] Alice Cruz
        @alice
        Known by Maria, Joel + 5
```

But the security model underneath MUST distinguish **recognition hints** from **identity proofs**.

## Core rule: discovery is not authentication

The following can help a human recognize an account but MUST NOT, by themselves, authenticate a person:

- display name;
- profile photo;
- biography;
- number of mutual contacts;
- phone number;
- email address;
- a public username that has not been independently verified;
- account age or activity count;
- follower/friend count.

All of these can be copied, bought, recycled, manipulated, or Sybil-amplified.

OM therefore models identity evidence as separate composable claims.

## Identity evidence model

```text
AccountIdentity
    cryptographic account continuity

ProfileDocument
    name / photo / bio
    signed by AccountIdentity

AliasClaim
    @alice / phone / email / directory name
    maps a locator to AccountIdentity for an epoch

DomainClaim
    alice@example.com
    proves control/delegation from example.com

ContinuityProof
    old AccountIdentity -> new AccountIdentity

RelationshipEvidence
    direct contacts recognize or attest this account

VerifierAttestation
    optional real-world/business/public-figure verification

ManualVerification
    QR / fingerprint / in-person confirmation
```

A client may combine these to help the user make a decision, but MUST retain the source and strength of each item instead of collapsing everything into a single opaque trust score.

## Mutual contacts: useful context, not proof

Mainstream social products make heavy use of profile photos and mutual-contact counts. This is genuinely useful for recognition, but a fake account can copy a face and accumulate many contacts who accepted it casually.

OM MAY display social context such as:

```text
Alice Cruz
[photo]
Known by Maria, Joel + 5 of your contacts
```

However:

1. a larger mutual count MUST NOT automatically increase cryptographic trust;
2. the UI SHOULD prefer named direct contacts the user actually knows over a raw popularity number;
3. Sybil accounts MUST NOT be able to manufacture a global verification score;
4. mutual-contact computation SHOULD avoid uploading the user's complete social graph to a central service;
5. exact privacy-preserving mutual discovery is an implementation research item and may use private set intersection or another reviewed technique later;
6. a mutual-contact indicator is a **recognition hint**, not a verification badge.

Good UI vocabulary:

- `Known by 7 of your contacts`
- `Maria and Joel recognize this account`
- `New account; no continuity proof`

Bad UI vocabulary:

- `60 mutuals = verified`
- `Trust score: 92`
- `Highly trusted user`

## Frictionless default aliases

OM should support a simple global-looking username experience for normal users, while keeping the underlying namespace replaceable.

Conceptually:

```text
User sees:        @alice
Canonical alias:  alice@directory.om
Resolves to:      AccountIdentity X
```

The default OM client may hide the default directory suffix for convenience. A third-party directory can expose another namespace without changing the account identity.

Properties:

- usernames are optional;
- usernames are aliases, not login secrets and not account identity;
- changing/deleting a username does not break existing relationships;
- a directory may impose anti-squatting, reserved-name, abuse, and dispute policies;
- those policies belong to that directory provider, not the OM protocol;
- alias mappings are versioned/epoch-bound so recycled aliases do not inherit trust.

## Domain-backed identities

OM SHOULD support domain-scoped aliases as first-class identities for businesses, organizations, public figures, professionals, and users who want portable naming.

Examples:

```text
alice@example.com
support@example.com
newsroom@example.org
```

This does not mean email is the OM identity. The domain acts as an authority for a namespace and signs or publishes a binding from the human-readable alias to an OM AccountIdentity.

A domain claim can be proven through a reviewed mechanism such as DNS and/or HTTPS well-known records. OM should support existing domain-security infrastructure rather than inventing a new global naming root.

### Why this matters

A business should not have to ask OM Incorporated to permanently own its identity.

```text
example.com
    │
    ├── alice@example.com  -> AccountIdentity A
    ├── support@example.com -> AccountIdentity S
    └── billing@example.com -> AccountIdentity B
```

If the business changes relay, storage, directory provider, or OM client, the domain can remain its public identity anchor.

## Organization identities and delegated roles

For businesses, governments, media organizations, schools, and other institutions, OM should support an `OrganizationIdentity` separate from employee accounts.

The organization can delegate role credentials:

```text
OrganizationIdentity: example.com

  signs -> alice@example.com   role: employee
  signs -> support@example.com role: support
  signs -> bot@example.com     role: bot/support-agent
```

Properties:

- employees keep their own AccountIdentity;
- the organization can revoke the role/domain claim without erasing the person's identity;
- business bots remain visibly bots even when organization-authorized;
- role credentials have expiry/version/revocation semantics;
- compromise of one employee account does not automatically compromise the organization root.

## Public figures and well-known names

Display names MUST remain non-unique. OM should not try to globally reserve every human name.

For high-impersonation targets, clients should rank stronger evidence prominently:

1. domain-backed identity from an already-known official domain;
2. continuity proof from a previously verified OM account;
3. organization delegation;
4. reciprocal proof from an official website;
5. optional `VerifierAttestation` from a trusted verification provider;
6. direct manual verification.

The default OM directory MAY reserve or adjudicate high-risk handles inside its own namespace, but that is a convenience/service policy and MUST NOT become protocol-level ownership of a person's name.

A person or business should be able to leave the default directory and retain stronger domain/cryptographic identity evidence.

## Verification should state WHAT was verified

Avoid one ambiguous universal blue check.

Better:

```text
Alice Cruz
[photo]

✓ example.com controls this identity
✓ Account continuity intact since 2026
Maria and Joel recognize this account
```

or:

```text
Example Bank Support
✓ Authorized by examplebank.com
BOT  Automated support agent
```

Possible verification labels:

- `Domain verified`
- `Organization authorized`
- `Account continuity verified`
- `Verified in person`
- `Verified by <provider>`
- `Recognized by 3 of your contacts`

Each has different semantics and MUST remain distinguishable internally.

## VerificationProvider abstraction

OM should allow optional real-world verification without making one company the global identity authority.

```text
VerificationProvider
    verify(subject, evidence)
    issueAttestation(...)
    revokeAttestation(...)
    status(...)
```

Potential providers can include:

- OM-operated verification;
- government/business registry integrations where lawful and appropriate;
- professional associations;
- enterprise directories;
- community organizations;
- independent verification services.

Clients may ship with a default trust policy for mainstream usability, while high-assurance users can inspect or alter which verifier roots they accept.

The protocol should carry signed attestations and their provenance, not a hard-coded global `verified=true` bit.

## Reciprocal website proof

For individuals and organizations with websites, OM should support a reciprocal proof pattern:

```text
Official website
    says: OM AccountIdentity X represents us

OM profile
    says: official website = example.com
```

The client verifies both sides and can display `example.com verified`.

This is conceptually similar to existing web/fediverse identity-verification approaches and avoids depending exclusively on OM staff.

## Alias transparency / equivocation resistance

A malicious or compromised directory must not be able to show different users different keys for the same public alias without a chance of detection.

Gate A should evaluate a reviewed key/alias-transparency mechanism with properties such as:

- append-only or auditable alias-to-AccountIdentity history;
- epoch/version numbers;
- signed alias changes;
- client consistency checks;
- monitoring/witness support;
- privacy considerations for non-public aliases.

OM should adopt a reviewed transparency design rather than inventing a bespoke cryptographic scheme.

## Public contact endpoints are not private relationship queues

Well-known people and businesses need public discoverability, but a public alias cannot become an unlimited public write endpoint.

Resolution should produce a constrained introduction/contact endpoint:

```text
@publicperson / support@example.com
        │
        ▼
PublicContactEndpoint
        │
        ├── strict size/rate quotas
        ├── message-request queue
        ├── optional challenge/invite policy
        ├── no large attachments by default
        ├── no calls by default
        └── no access to private relationship queues
```

After acceptance, ordinary users establish private relationship capabilities.

Businesses may intentionally operate high-volume support endpoints, but those are separate scalable services with explicit abuse budgets and bot/agent permissions.

## Suggested discovery UX

### Ordinary personal search

```text
Alice Cruz
[photo]
@alice
Known by Maria, Joel + 5

[Message]
```

No security vocabulary is needed unless there is contradictory evidence.

### Possible impersonation

```text
Alice Cruz
[recognizable copied photo]
@alice-news
Known by 12 contacts

New account
No link to the Alice you already know

[View details] [Ignore]
```

The mutual count does not override broken continuity.

### Domain-backed public figure/business

```text
Example News
[logo]
newsroom@example.com
✓ example.com verified

[Message]
```

### New account with continuity

```text
Alice Cruz
[photo]

✓ Moved from Alice's previous OM account
Known by Maria, Joel + 5

[Message]
```

## Discovery ranking guidance

Search ranking MAY use convenience signals, but security labels MUST be computed separately.

Possible ranking inputs:

- exact alias match;
- local address-book match;
- profile name/photo similarity;
- known PersonRecord relationship;
- direct mutual-contact context;
- domain relevance;
- previous conversation history.

Security/identity evidence is a separate channel:

- continuity proof;
- domain proof;
- organization credential;
- verifier attestation;
- manual verification;
- direct contact attestations.

Never turn ranking score into identity confidence.

## Design lessons adopted from existing systems

### Bluesky-style domain handles

Domain-backed handles demonstrate that DNS ownership can be used as decentralized public identity evidence and can remain portable across service providers.

OM should use the same principle while keeping the AccountIdentity independent of the domain alias.

### Mastodon-style namespaced addresses and reciprocal links

`user@domain` naming gives each domain an independent namespace, while reciprocal website links provide a practical proof that an account controls a known website.

OM should support both ideas without requiring users to understand federation during routine use.

### Signal-style contact aliases

A username can be a temporary contact-initiation mechanism rather than the permanent identity shown in conversations. OM should preserve this separation.

### Key-transparency lesson

Automatic cryptographic key verification can detect inconsistencies in directory/key mappings, but it does not prove real-world identity. OM must preserve that distinction in both protocol and UI.

## Gate-A invariants added by this design

1. Profile recognition and social familiarity are UX signals, not cryptographic proof.
2. Mutual-contact count never becomes a global trust/reputation score.
3. Display names are non-unique.
4. Default `@username` is a convenience alias in a replaceable directory namespace.
5. Domain-backed aliases are first-class and can provide stronger public/business provenance.
6. Organizations can cryptographically delegate and revoke role identities.
7. Verification records what evidence was verified rather than exposing one ambiguous boolean badge.
8. Optional verification providers are pluggable; no single verifier is protocol-mandatory.
9. Public identities use constrained public-contact endpoints, not unlimited private relay capabilities.
10. Directory alias/key mappings require an auditable anti-equivocation story.
11. Security confidence and search ranking are separate systems.
12. The normal UI remains name/photo-first; high-assurance evidence is available progressively.

## Open questions

- Exact canonical syntax for namespaced aliases: `alice@example.com`, `@alice@example.com`, or URI form underneath a simpler presentation.
- Whether the OM default directory offers bare `@alice` and how its hidden canonical namespace is represented.
- Reserved-name and dispute policy for the default OM directory.
- Exact DNS/HTTPS proof format for DomainClaim.
- Whether DNSSEC is required, preferred, or optional when HTTPS proof also exists.
- Organization root-key recovery and rotation.
- VerificationProvider trust-store governance.
- Privacy-preserving mutual-contact discovery mechanism.
- Alias/key transparency protocol and witness model.
- Search-ranking resistance to Sybil/profile-copy attacks.
- UX distinction between `domain verified`, `real-world identity verified`, and `cryptographic continuity verified`.

## 2026-09-21 discovery checkpoint: Resolver + ContactDescriptor

The earlier alias model is refined into the canonical `DISCOVERY_FABRIC.md` design.

Key changes:

- no protocol-global bare username registry;
- a local OM Resolver queries replaceable DirectoryProviders/domain discovery/direct invitations;
- all discovery methods yield a signed/validated `ContactDescriptor`;
- public descriptors preferably expose alias-specific introduction identity, not stable AccountID;
- optional OHTTP/ODoH-inspired lookup separates requester network identity from plaintext alias query;
- public directory entries expose an IntroductionServiceDescriptor, not a reusable mailbox capability;
- descriptors are authenticated, expiring, cacheable, and may carry transparency proofs;
- accepted relationships cease depending on directories and exchange private RelaySets/capabilities directly.

See `DISCOVERY_FABRIC.md`, `INVITES_INTROGATE_AND_SPAM.md`, and ADR-0019/0020/0021/0025.
