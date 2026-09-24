# OM Censorship and Network Resilience

- **Status:** Architectural direction
- **Checkpoint:** 2026-09-21 10:08 PHT

## 1. Goal and limit

OM should not promise to be impossible to censor. A sufficiently powerful state/network adversary may block app distribution, known IPs/domains, push providers, hosting networks, protocols, or Internet access itself.

The realistic goal is:

> **No single technical or administrative switch should disable the entire OM network.**

Censorship resilience is treated as a portability and transport-diversity property, not as a claim of guaranteed anonymity or reachability.

**Claim discipline (ADR-0130 rule 6, R-269).** The goal above is an architecture target, not a property of W0. Until
PR-013 passes (a second reference Home with no cloud-vendor dependency passes provider conformance, and onboarding
offers a vendor-neutral zero-signup path), no public text, store listing or UI may use this section's or BLUEPRINT
§18A's resilience wording as a claim about a release. The V-020 vendor-block model shows why: with Cloudflare-first
hosting and no fallbacks, a Cloudflare-wide block leaves about 2% of relationships reachable.

### 1.1 Blocking threats (T-085, T-086)

- **Network-level blocking** by a state or network operator: vendor IP ranges, SNI or ECH-less hostname filtering,
  protocol fingerprinting, or blocking `*.workers.dev`. This differs from T-020 (a vendor's own action): the vendor
  keeps serving, the user's network cannot reach it.
- **Discovery-suffix blocking:** for T2 users, locators and invite landing pages under one suffix (for example
  `workers.dev`) are a single choke point for first contact, even when established relationships have other routes.
- **Distribution removal:** forge takedown or app-store removal of the reference client (§9).

Mitigations are the Home tiers and second-family fallbacks (R-194), descriptors that list locators on more than one
suffix or family where the user has them, invites that carry more than one locator, the NetworkTransport ladder (§5),
and mirrored releases. The V-020 pre-engineering criteria include a blocked-vendor and blocked-suffix simulation
that measures how many established relationships and first contacts survive.

## 2. Separate OM semantics from transport

OM messages, identity, crypto, events, and accepted relationships sit above a pluggable `NetworkTransport` / `TransportProvider` layer.

```text
OM encrypted protocol
      │
      ▼
NetworkTransport
      │
 ┌────┼──────────────┐
 ▼    ▼      ▼       ▼
HTTP  WS   QUIC     TCP
                
plus optional:
proxy/MASQUE-style paths
community bridges
Tor/pluggable-transport-style adapters
LAN/direct
future transports
```

No transport may change OM's canonical message semantics or E2EE requirements.

## 3. Avoid global choke points

OM should support diversity across:

- directory providers;
- discovery privacy relays;
- introduction services;
- message relays;
- media providers;
- blob/storage providers;
- push providers where platform allows;
- network transports.

Clients may use several providers concurrently.

## 4. Pluggable anti-censorship transports

Borrow the architectural lesson from Tor's pluggable transports: circumvention mechanisms evolve independently from the protected application protocol.

OM core should expose a stable byte-stream/datagram transport contract so new circumvention mechanisms can be added without changing AccountID, contact relationships, history, or event encodings. That contract is `NetworkTransport` in [`IMPLEMENTATION_INTERFACES.md`](../providers/IMPLEMENTATION_INTERFACES.md): a transport is an untrusted byte carrier, and failover never changes semantics (R-268, conformance cases NT-01..NT-05).

## 5. Ordinary-first behavior

Do not route every user through heavy anti-censorship infrastructure by default. Normal paths should remain efficient and inexpensive.

Escalate only when policy/reachability requires:

```text
normal HTTPS/QUIC/direct path
         ↓ blocked/fails
alternate provider/relay
         ↓ blocked/fails
proxy/bridge transport
         ↓
optional specialized circumvention
```

## 6. Discovery resilience

A censor that blocks one directory must not break established relationships. Clients can:

- use cached signed descriptors until expiry;
- query alternate configured directories;
- use domain discovery;
- use QR/contact-card invites;
- receive authenticated endpoint updates from existing contacts;
- use LAN rendezvous locally.

## 7. Relay resilience

Private `RelaySet` entries can span unrelated networks. If one endpoint is unreachable, the sender attempts another according to policy. Capability identifiers remain distinct across relays.

## 8. Traffic-analysis caveat

Encryption does not hide all traffic metadata. Active probing, traffic fingerprinting, endpoint blocking, global observation, and device compromise remain relevant threats.

Specialized obfuscation/circumvention transports require their own threat models and should not be marketed as universal anonymity.

## 9. App/update resilience

Long-term design should allow:

- reproducible/release-transparent builds;
- multiple compatible clients;
- independently distributed clients where platforms permit;
- signed protocol/profile registries mirrored by multiple parties;
- migration away from the original OM organization.

This prevents the software-distribution layer from becoming OM's hidden central authority.

## High-anonymity / nation-state mode remains an adapter

Tor, xx-network-style mix/routing systems, MASQUE/proxy paths, padding/cover traffic, and future pluggable transports may implement a Maximum Privacy or censorship-resilience `NetworkTransport`/`RoutePolicy` later. They are not required for ordinary v1 operation and must not force identity/history migration when added.

## 2026-09-23 08:21 continuity clarification

The 0M longevity pass strengthens section 9: a published OM protocol generation must be reconstructible from independently mirrored Protocol Continuity Bundles containing its normative semantics/registries/vectors/licenses. Blocking the original project domain/forge therefore harms discovery and updates but does not become a protocol-validity oracle or erase existing user identities.
