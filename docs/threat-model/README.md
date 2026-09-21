# OM Threat Model — master direction

## Assets

- message/media plaintext;
- account/device private keys;
- identity continuity;
- relationship graph and metadata;
- group membership/roles;
- local history and backups;
- recovery capability;
- provider credentials;
- software update integrity.

## Adversaries

- passive network observer;
- active MITM;
- malicious/compromised relay;
- malicious/compromised directory;
- malicious/compromised storage provider;
- malicious/compromised TURN/SFU;
- colluding providers;
- hostile modified client;
- abusive sender/spammer;
- malicious group member/admin within their legitimate permissions;
- stolen endpoint;
- compromised endpoint;
- compromised build/update infrastructure;
- coercive future operator/governance actor;
- future cryptanalytic/quantum attacker recording traffic today.

## Major invariants

1. Providers do not receive message/media plaintext unless explicitly designed as an endpoint/bridge.
2. AccountID survives provider and algorithm replacement.
3. Relay compromise cannot forge canonical sender authorization or force crypto downgrade.
4. Storage compromise reveals ciphertext, sizes/timing/object metadata only within minimized design.
5. SFU/TURN compromise cannot decrypt SFrame media.
6. Directory equivocation should be detectable through transparency/witness mechanisms.
7. Unknown senders cannot force expensive unbounded work.
8. No single provider is required to restore account history/identity where the user configured redundancy/recovery.
9. App update compromise is treated as catastrophic and mitigated with strict release/supply-chain practices.

## Metadata reality

OM cannot promise zero metadata. Providers may observe IP addresses, timing, byte sizes, and local resource usage. The architecture minimizes durable stable identifiers, uses opaque capabilities, permits route/provider diversity, and can relay calls to hide peer IPs, but does not claim anonymity against a global passive adversary without additional anonymity-network design.

## Endpoint reality

E2EE does not protect plaintext on a compromised unlocked endpoint. Revocation cannot make an already-compromised device forget plaintext it previously learned. Reporters can intentionally disclose plaintext they possess.

## PQ threat

Harvest-now-decrypt-later motivates PQ/hybrid key establishment and continuous key evolution. CryptoProfile agility is itself a mitigation against future algorithm failure.

## Denial of service

Provider independence improves survivability but does not make any single endpoint/provider immune to DoS. Multi-provider capability and bounded first-contact work are required.

## Required red-team scenarios

- directory serves split identity views;
- relay correlates stable queue IDs;
- two relays collude on group fanout;
- attacker strips new CryptoProfile support;
- malicious client floods introductions;
- compromised SFU attempts media substitution/injection;
- network switch causes accidental P2P IP disclosure under strict privacy mode;
- Cloudflare/free provider quota exhaustion;
- storage provider deletes/corrupts blobs;
- restore from stale backup reintroduces revoked device/profile;
- official update channel compromised.
