# Directory and Key Transparency

## Goal

Human-friendly discovery must not let a directory silently equivocate about a user's account/device key state.

## Directory is not identity root

A directory answers queries such as:

```text
alias -> signed ContactDescriptor (alias-specific intro identity + request key)
```

It can control whether an alias exists in that directory, but it cannot redefine AccountID continuity. A directory
record carries no AccountID and no account state hash.

## What is logged

The transparency subject is the binding

```text
alias -> intro identity public key (+ ContactDescriptor digest)
```

not the AccountID or the AccountControlState head, which stay private until a relationship is accepted. Split-view
detection therefore compares intro keys for an alias.

Protection has three layers:

1. **Key transparency**, where the directory offers it, detects a directory that shows different intro keys to
   different people.
2. **IntroIdentityBinding** (ADR-0126): inside the accepted relationship, a device active in the recipient's
   AccountControlState signs the intro key. A substituted intro key fails this check for a sender who knows or later
   learns the recipient's account.
3. **AccountControlHead gossip** (below) then detects stale or forked account state.

**Trust on first use.** First contact through a directory without key transparency, including a workers.dev personal
publisher ([`DISCOVERY_FABRIC.md`](DISCOVERY_FABRIC.md)), is trust-on-first-use: a publisher that substitutes the intro identity for a sender
who has never met the recipient is not detected until the two compare safety numbers or meet through another path.
Clients say so in the verification UI.

## Equivocation threat

A malicious/compromised directory could show Alice one key/account state and Bob another. OM should make such split views detectable.

## Direction

Use an append-only transparency structure and independent observation:

```text
Directory publishes commitments/log roots
        |
 independent witnesses / auditors
        |
 clients verify inclusion/consistency and gossip/checkpoints
```

A witness gains detection power, not identity-rewrite power.

## Multiple directories

OM permits multiple directories or direct invites. A user may have multiple aliases with separate provenance. A QR/invite can bypass public directory lookup entirely.

## Optional transparency-profile release gate

The architecture question is closed: OM v1 has **no global transparency authority**. Relationship-scoped AccountControlHead gossip, direct verification and the reciprocal IntroIdentityBinding remain sufficient for private continuity. A DirectoryProvider may additionally publish an independently witnessed transparency mechanism for its own namespace.

OM does not freeze a bespoke Merkle construction here. Advertising a public `key_transparency` claim is gated by `PR-011`, which requires selection and external review of a reviewed per-directory mechanism. Until that gate passes, clients describe a directory without such a mechanism as trust-on-first-use for first contact. Choosing the concrete mechanism is therefore a release-claim verification task, not an unresolved architecture decision.

## Discovery privacy and resolver separation

Directory search and identity transparency are separate functions. A DirectoryProvider may return signed ContactDescriptors while an independent transparency/witness layer detects equivocation. OM SHOULD support an optional OHTTP/ODoH-inspired split lookup mode: a DiscoveryRelay sees the client connection but not the alias query; a DirectoryResolver sees the query but not the client's transport address. This minimizes metadata without making either party an identity root.

## AccountControlHead observations

In addition to directory/key transparency, OM can gossip compact `AccountControlHead` observations through established relationships or optional witnesses. This detects stale-but-valid provider views and forks without letting observers author account state. Relationship-scoped identifiers should be preferred when they reduce public correlation.

This direction is consistent with current IETF Key Transparency architecture work, but OM does not freeze a work-in-progress Internet-Draft into the protocol until reviewed/stable.
