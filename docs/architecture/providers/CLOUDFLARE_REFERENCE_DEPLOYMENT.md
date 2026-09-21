# Cloudflare Reference Deployment Profile

## Why Cloudflare is interesting

OM wants normal users and small communities to get reliable infrastructure with almost no operational knowledge. Cloudflare currently offers several components that can make a reference deployment inexpensive and globally reachable.

Cloudflare is **not** an OM dependency. This document is one provider profile.

## Mapping

| OM capability | Cloudflare option | Notes |
| --- | --- | --- |
| Relay API / queue coordinator | Workers + Durable Objects | SQLite-backed DO available on Free plan |
| Offline small-envelope persistence | Durable Object SQLite | bounded queue, not archive |
| Encrypted attachments/blobs | R2 | S3-compatible, free egress |
| STUN | Realtime STUN | NAT discovery; no media relay when direct succeeds |
| TURN | Realtime TURN | privacy/connectivity fallback |
| Group-call SFU | Realtime SFU | routing only; OM retains SFrame E2EE |
| Edge ingress / rate limiting | Workers/Cloudflare edge | implementation-specific |

## Free-plan behavior verified for 2026-09-21

### Durable Objects

Workers Free supports SQLite-backed Durable Objects. Current documented limits include daily request/compute/row limits and 5 GB total SQLite storage. Importantly, Cloudflare documents that on Workers Free, exceeding a free-tier limit causes further operations of that type to fail rather than transparently billing them.

Operational consequence: OM should monitor usage and fail over or shed nonessential work **before** a hard limit causes message-delivery failures.

### R2

Standard R2 currently includes a monthly free allocation of storage and Class A/B operations, with no Internet egress fee. Use it for encrypted blobs, not tiny message rows.

### Realtime

Cloudflare documents a shared 1,000 GB/month free allowance for TURN + SFU egress, then $0.05/GB. STUN is documented as free/unlimited.

Operational consequence: unlike Free Durable Object hard limits, Realtime is a potential paid overage path. A zero-budget deployment needs its own hard budget routing guard.

## Reference zero-cost policy

```text
policy.monthly_paid_budget = 0

messages:
  use DO while projected safe
  fail over to configured secondary relay before hard limit

attachments:
  use R2 while within configured allowance/policy
  choose alternate BlobStore otherwise

calls:
  LAN/P2P first when privacy policy permits
  TURN only when needed
  stop allocating paid-overage Cloudflare media before free budget is exhausted
  use alternate TURN/SFU if configured
```

The provider plugin, not the wire protocol, contains quota interpretation.

## One-click operator UX

An advanced user/community operator may eventually choose:

```text
Deploy OM infrastructure → Cloudflare
```

and an installer/automation provisions the required Worker, Durable Object namespaces, R2 bucket, secrets, DNS, and media credentials. Ordinary end users never need Cloudflare accounts.

## Failure independence

Where possible, recommend diversity across provider/failure domains. For example:

- primary relay on Cloudflare;
- secondary relay on a small VPS provider;
- encrypted backup on a user-controlled NAS/other cloud;
- direct P2P calls when possible.

This is more meaningful than “three relays” that all run in one cloud account.

## Discovery and introduction roles

Cloudflare can also host optional OM discovery/first-contact components—for example a DirectoryProvider endpoint, `/.well-known/om` static/delegation resource, or IntroGate—but none is protocol-mandatory. A user may use a Cloudflare relay while using a different directory and IntroGate, or vice versa.

Public directory records should lead to an IntroductionService, not publish a permanent message mailbox capability. Cloudflare-native DDoS/rate-limit features may protect that service operationally while OM-level IntroductionTickets and capability semantics remain provider-independent.

## Multi-relay deployment

A Cloudflare relay should be treated as one entry in a possible private `RelaySet`. Reliability/privacy-sensitive users or communities should prefer a second relay in an independent failure domain rather than two endpoints that ultimately share the same provider.
