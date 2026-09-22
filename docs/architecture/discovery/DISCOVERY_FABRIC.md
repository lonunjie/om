# OM Discovery Fabric

- **Status:** Canonical architecture checkpoint
- **Checkpoint:** 2026-09-21 10:08 PHT (UTC+08:00)
- **Scope:** Human discovery, resolver architecture, directory providers, domain aliases, direct invites, privacy, caching, delegation, transparency, and rendezvous.

## 1. Goal

OM needs mainstream-messenger ease without creating a mandatory global registry that can become a censorship, surveillance, or identity-control point.

The governing rule is:

> **Identity is cryptographic. Names are aliases. Discovery is rendezvous. Location is replaceable.**

A discovery mechanism is successful when it produces an authenticated `ContactDescriptor`. The directory, domain, QR code, contact card, or LAN mechanism that produced the descriptor is not part of the subsequent relationship.

## 2. What OM learns from DNS — and what it does not copy

Useful DNS architecture lessons:

- separate human names from network location;
- separate resolver from authoritative source;
- use delegation rather than forcing every namespace to run identical infrastructure;
- support multiple authoritative/service endpoints;
- cache signed/validated data with bounded lifetime;
- allow local-network resolution where appropriate;
- avoid making one application provider own all endpoints.

What OM deliberately does **not** copy:

- a mandatory single global root namespace;
- plaintext lookup privacy assumptions;
- unauthenticated/local broadcast of stable user/device names;
- the idea that the resolver itself authenticates a human identity;
- permanent globally enumerable identity records.

DNS is decentralized in lookup/operation but its global namespace still has a coordinated root. OM therefore uses DNS as an architectural inspiration, not as a blueprint for a mandatory global OM root.

## 3. Resolver model

Every OM client contains a local `Resolver` component:

```text
human input
   │
   ├── @friendly-name
   ├── name@example.org
   ├── om:<invite>
   ├── QR / NFC / contact card
   └── LAN rendezvous
           │
           ▼
      OM Resolver
           │
   ┌───────┼────────┐
   ▼       ▼        ▼
Directory  Domain   Local/Invite
Provider   Resolver Resolver
   │       │        │
   └───────┴────────┘
           │
           ▼
   ContactDescriptor
           │
           ▼
      Introduction
           │
         Accept
           │
           ▼
 private relationship
```

The Resolver is not an authority. It queries or validates authorities and direct proofs.

## 4. No protocol-global bare username

A bare handle such as `@juan` can be shown as a convenience, but the protocol must retain provenance.

Conceptually:

```text
UI:        @juan
Canonical: juan@directory-a
```

Another provider may independently have:

```text
juan@directory-b
```

The official client may search a default set of directories and present disambiguated results. The user should not normally need to understand namespaces, but OM must never pretend that a globally unique bare handle exists without an authority deciding collisions.

## 5. Domain-scoped aliases

A domain alias such as:

```text
alice@example.org
```

means that `example.org` is authoritative for the `alice` alias within that namespace. The domain does not become Alice's root identity.

Recommended v1 discovery pattern:

```text
https://example.org/.well-known/om
```

That resource delegates to one or more OM discovery providers/endpoints. The domain can host it itself or delegate operation to another provider.

DNS SVCB/HTTPS records may be supported later for optimized service binding, but OM should not encode its whole identity model into DNS records.

## 6. Direct invitations are protocol-native discovery

A QR/link is the most registry-independent mechanism:

```text
om:<encoded invite>
```

It can carry enough information to find one or more `IntroductionService` endpoints and authenticate the invitation. It does not need a username directory or domain.

The invite does **not** reveal the accepted relationship's permanent delivery capabilities. It is only permission to attempt first contact.

## 7. ContactDescriptor

All discovery paths converge on a signed/validated logical object:

```text
ContactDescriptor
  descriptor_id
  subject_hint / alias provenance
  protocol_versions
  introduction_services[]
  optional relay_hints[]
  optional verification claims[]
  crypto/profile capabilities
  not_before
  expires_at
  issuer/provenance
  authentication / signature / transparency proof
```

The descriptor should contain the minimum information required to begin an introduction. Where practical, a public descriptor should use alias-specific introduction keys instead of exposing a stable AccountID.

### AccountID reveal

Preferred privacy flow:

```text
public alias
   ↓
alias-specific introduction identity
   ↓
message request
   ↓
recipient accepts
   ↓
E2EE AccountID proof / binding
```

This reduces passive correlation between multiple aliases belonging to one person.

## 8. Caching and expiry

Descriptors are cacheable only until their authenticated expiry/TTL.

Benefits:

- temporary directory outage does not break discovery immediately;
- normal messaging never requires repeated directory lookups;
- alias/relay changes propagate within bounded time;
- censorship or provider outage has less immediate effect.

Clients should refresh opportunistically and keep a last-known descriptor for diagnostics, but must not silently use an expired descriptor if that would violate revocation/security policy.

## 9. Multiple directory providers

A client may configure:

```text
DirectorySet
  default OM directory
  community directory
  enterprise directory
  user-selected directory
```

Search can query more than one provider. Results must preserve namespace/provenance. The protocol must not require the official OM directory.

A directory has narrow authority:

- assign/search names in its own namespace;
- enforce its own anti-squatting/abuse policy;
- refuse service;
- return descriptors.

It cannot:

- replace a user's AccountID;
- decrypt conversations;
- control accepted relationships;
- prevent existing contacts from communicating through alternate relays;
- make itself the mandatory network-wide identity root.

## 10. Private lookup mode

OM should support an optional privacy-preserving lookup architecture inspired by Oblivious HTTP / Oblivious DoH:

```text
client
  │ encrypted query
  ▼
DiscoveryRelay
  │ sees client network address
  │ cannot read alias query
  ▼
DirectoryResolver
  │ sees alias query
  │ does not see client IP
  ▼
encrypted response
```

The two roles should preferably be operated by independent organizations/failure domains.

This is not anonymity against a colluding relay+resolver or a global passive adversary; it is a practical metadata minimization layer.

## 11. Transparency and equivocation resistance

Directory records should be signed and short-lived. For high-assurance directories, OM should additionally support key-transparency-style append-only commitments, independent witnesses, and/or client gossip.

The goal is to detect a directory that tells:

```text
Alice:  @maj -> real Maj
Bob:    @maj -> attacker
```

without making the transparency service itself the identity authority.

## 12. Search vs authentication

Search results are recognition hints. Authentication comes from cryptographic identity continuity, domain/organization claims, manual verification, continuity proofs, or other explicit evidence.

OM must not collapse search ranking, popularity, mutual-contact count, and cryptographic verification into one global trust score.

## 13. Provider interface

```text
DirectoryProvider
  resolve(alias)
  search(query)
  publish(claim/descriptor)
  update(claim/descriptor)
  revoke(claim/descriptor)
  get_transparency_proof(...)
  capability_policy()
```

```text
Resolver
  resolve(input, policy) -> ContactDescriptor[]
  verify(descriptor, policy)
  cache(descriptor)
  refresh(descriptor)
```

## 14. Non-goals

- no mandatory blockchain/global ledger;
- no mandatory OM-owned global username root;
- no permanent public mapping from alias to private delivery capability;
- no requirement that every user own a domain;
- no requirement that every user run a server;
- no upload of the complete address book/social graph as a prerequisite for discovery.

## 15. Product UX

Ordinary user flows remain simple:

```text
Search a name → tap result
Scan QR → tap Connect
Open invite link → tap Connect
```

All resolver/provider/transparency mechanics are automatic unless the user explicitly enters advanced settings.

## 21:32 checkpoint: workers.dev personal publishers and shared contacts

For v1 personal deployments, a user's Cloudflare Worker FQDN (`<worker>.<account-subdomain>.workers.dev`) may act as a `PersonalDiscoveryPublisher`. It serves a signed ContactDescriptor/IntroductionServiceDescriptor and is not a global directory.

Grouped searchable directories begin naturally at organizations/communities that own FQDN namespaces. `alice@example.org` can delegate to any DirectoryProvider without making OM Inc. the root.

### ContactCapsule forwarding

A signed, expiring ContactCapsule is another resolver input. Carol can forward Alice's capsule to Bob; Bob verifies Alice's signature and goes through Alice's IntroGate. Carol may attach a referral attestation, but she does not become an identity authority and does not learn Alice/Bob pairwise delivery capabilities.

Cached/forwarded capsules are authenticated bounded-lived discovery answers, preserving one of DNS's useful caching/delegation properties without copying its root model.
## 2026-09-22 discovery/key-material separation

Finding an AccountID/ContactDescriptor and obtaining current authenticated per-device handshake material are separate operations. A DirectoryProvider may advertise appropriate KeyMaterialProvider endpoints/capabilities, but does not become device authority by doing so.

