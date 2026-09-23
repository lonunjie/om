# OM Censorship and Network Resilience

- **Status:** Architectural direction
- **Checkpoint:** 2026-09-21 10:08 PHT

## 1. Goal and limit

OM should not promise to be impossible to censor. A sufficiently powerful state/network adversary may block app distribution, known IPs/domains, push providers, hosting networks, protocols, or Internet access itself.

The realistic goal is:

> **No single technical or administrative switch should disable the entire OM network.**

Censorship resilience is treated as a portability and transport-diversity property, not as a claim of guaranteed anonymity or reachability.

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

OM core should expose a stable byte-stream/datagram transport contract so new circumvention mechanisms can be added without changing AccountID, contact relationships, history, or event encodings.

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
