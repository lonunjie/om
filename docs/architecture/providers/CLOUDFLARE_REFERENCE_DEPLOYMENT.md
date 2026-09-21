# Cloudflare Reference Deployment Profile

## Purpose

The initial OM implementation deliberately uses **user-owned Cloudflare accounts** as a frictionless personal infrastructure substrate while keeping Cloudflare outside canonical identity/protocol semantics.

Cloudflare is a reference deployment and bootstrap convenience, not OM's central relay fleet.

## v1 personal model

```text
Alice owns CF account -> Alice Worker / SQLite DO
Bob owns CF account   -> Bob Worker / SQLite DO
```

OM can automate provisioning through Cloudflare's public OAuth + Authorization Code/PKCE flow. Users should not paste long-lived API tokens.

## workers.dev discovery

Workers accounts have configurable `workers.dev` subdomains and every Worker receives a `<worker>.<account-subdomain>.workers.dev` FQDN. That gives a personal user a zero-domain HTTPS locator for:

- signed `ContactDescriptor` / `IntroductionServiceDescriptor`;
- IntroGate endpoints;
- relay API ingress;
- minimal human install/contact landing page.

The FQDN is a replaceable alias/locator, not AccountID. Cloudflare recommends custom domains/routes for production/business-critical uses, so organizations/FQDN owners should publish domain-scoped OM discovery under domains they control.

## Minimal zero-payment core

| OM capability | Cloudflare option | v1 status |
| --- | --- | --- |
| HTTPS API / personal discovery / IntroGate | Worker | core |
| bounded mailbox/queue/rate-limit/ticket state | SQLite Durable Object | core |
| small temporary encrypted blob spool | SQLite DO, tightly bounded | optional core optimization |
| encrypted large/offline blobs | R2 | optional expansion |
| richer organizational/search directory | D1 | optional |
| richer landing/site content | Pages | optional |
| STUN/TURN/SFU | Realtime | optional media provider |

SQLite Durable Objects are available on Workers Free. Cloudflare documents that exceeding a Workers Free DO limit causes further operations of that type to fail until reset, so OM monitors and sheds/fails over before the hard edge.

## Why R2 is not mandatory

R2 includes free monthly usage but Cloudflare requires an R2 subscription/checkout to enable it, and usage-based products can require billing/payment setup. Therefore the baseline "set up OM without entering a card" path must not depend on R2.

R2 remains a good `BlobStore` for users who enable it. Alternatives remain equally valid.

## File/data routing

```text
small message -> DO queue
online file   -> P2P/LAN where policy permits
small temporary offline blob -> bounded DO spool if implementation benchmarks allow
large/offline media -> configured BlobStore (R2/S3/WebDAV/etc.)
archive/backup -> configured encrypted StoragePool
```

Exact DO blob thresholds are implementation benchmarks, not wire-protocol constants.

## Free-plan and billing guardrails

Provider adapters expose quota/cost semantics. A policy such as `monthly_paid_budget = 0` is enforced before billable resource use.

Cloudflare product behavior differs:

- Free SQLite DO: hard failure after documented Free limits;
- R2: subscription/usage-based product with included free allowance;
- Realtime TURN/SFU: provider-specific allowance/overage behavior that must be rechecked before deployment.

OM never assumes "free tier" means one universal billing model.

## Provisioning independence

The official OM Cloudflare OAuth client is replaceable. `ProvisioningProfile` supports:

- official OAuth client;
- third-party OM distribution OAuth client;
- CLI/manual deployment;
- future provider-native provisioning.

If the official provisioning client is blocked or revoked, existing identities and communication continue.

## Failure independence

Users may configure multiple relays/providers. Diversity across provider/account/ASN/region is more meaningful than several endpoints sharing one failure domain.

Cloudflare-specific resource IDs never appear in AccountID, canonical events, or required wire semantics.
