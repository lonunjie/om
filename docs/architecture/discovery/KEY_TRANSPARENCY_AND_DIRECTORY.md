# Directory and Key Transparency

## Goal

Human-friendly discovery must not let a directory silently equivocate about a user's account/device key state.

## Directory is not identity root

A directory answers queries such as:

```text
alias -> signed Account/Discovery record
```

It can control whether an alias exists in that directory, but it cannot redefine AccountID continuity.

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

## Open question

Select a reviewed key-transparency protocol/implementation appropriate to OM AccountState/device updates and privacy constraints. Do not invent a bespoke Merkle-log protocol without expert review.

## Discovery privacy and resolver separation

Directory search and identity transparency are separate functions. A DirectoryProvider may return signed ContactDescriptors while an independent transparency/witness layer detects equivocation. OM SHOULD support an optional OHTTP/ODoH-inspired split lookup mode: a DiscoveryRelay sees the client connection but not the alias query; a DirectoryResolver sees the query but not the client's transport address. This minimizes metadata without making either party an identity root.

## AccountControlHead observations

In addition to directory/key transparency, OM can gossip compact `AccountControlHead` observations through established relationships or optional witnesses. This detects stale-but-valid provider views and forks without letting observers author account state. Relationship-scoped identifiers should be preferred when they reduce public correlation.

This direction is consistent with current IETF Key Transparency architecture work, but OM does not freeze a work-in-progress Internet-Draft into the protocol until reviewed/stable.
