# OM Design Rationale and Discussion Synthesis

This document preserves the reasoning behind the September 2026 architecture decisions so future implementers do not see only conclusions without knowing the tradeoffs that produced them.

## 1. Privacy is a power, not proof of good intent

Strong private communication benefits ordinary people and can also be used by people with harmful motives. That does not make privacy itself bad; the same dual-use property exists with locks, cash, private homes, legal privilege, and encryption.

The alternative — a universal platform operator that can inspect/intervene everywhere — moves risk rather than eliminating it. The system then depends on that operator remaining competent, benevolent, secure, politically independent, and unchanged through future leadership/acquisition/coercion.

OM therefore does not make private communication dependent on promises such as “we can read everything but will behave responsibly.” It prefers architectures where the operator lacks universal power in the first place.

## 2. The useful question is not “who is Big Daddy?” but “where did authority go?”

Decentralization does not automatically remove authority. Authority can migrate into:

- protocol rules;
- node/relay operators;
- directories;
- dominant client software;
- update-signing keys;
- group administrators;
- storage providers;
- media providers;
- maintainers/governance bodies.

Therefore every OM component must identify exactly what authority it receives and what it cannot do. The target is not “nobody has authority.” The target is “nobody has enough authority to become universal controller.”

This produced the rule: **scoped, replaceable, non-composable authority**.

## 3. Decentralize authority, not every mechanism

Making every operation peer-to-peer or blockchain-based would add complexity and often reduce reliability. Reliable offline messaging naturally benefits from servers. Group calls benefit from SFUs. Push on mobile uses platform services.

The architectural test is not whether a server exists; it is whether that specific server is indispensable or trusted with powers it does not need.

A relay may reliably hold ciphertext. It should not own identity.

An SFU may route media. It should not decrypt it.

A blob store may hold large encrypted files. It should not decide conversation state.

## 4. Stable identity is valuable; stable network handles are dangerous

OM needs a durable AccountID so users survive changed phone numbers, aliases, relays, and crypto algorithms. But using the same stable AccountID as a routing address would create a global correlation handle.

Therefore OM separates application identity from delivery addressing:

- AccountID is stable inside authenticated relationship/account state;
- relay delivery uses private random capabilities;
- capabilities can rotate and differ by peer/device/provider.

This combines continuity with transport metadata minimization.

## 5. Crypto agility must not become user complexity

The first thought was “make ciphers plug-and-play.” The refined conclusion is:

- implementation primitives may be modular;
- production wire negotiation must choose complete reviewed CryptoProfiles;
- users never choose ciphers;
- migration is background;
- once stronger security is established, a downgrade floor prevents silent fallback.

This avoids two opposite failures:

1. hard-coding today's algorithm forever;
2. offering arbitrary combinations that create an unreviewable protocol matrix.

## 6. Why PQ-first but hybrid

Long-lived private messages face harvest-now-decrypt-later risk. That justifies post-quantum key agreement early. But newer PQ constructions also have less deployment history than classical primitives.

Hybrid designs provide a hedge: compromise of only the classical or only the PQ component should not immediately break the combined secret where the construction is sound.

The architecture therefore treats PQ as a first-class requirement while refusing to fossilize current drafts into permanent OM semantics.

## 7. Why AccountID must survive algorithm replacement

If AccountID were literally one public-key fingerprint, then moving from today's signature algorithm to a future replacement could look like a new person/account. That would make cryptographic agility conflict with identity continuity.

Making AccountID an algorithm-independent lineage identifier lets signed credential epochs evolve underneath it. Future crypto migration becomes routine maintenance rather than account migration.

## 8. Why Cloudflare is attractive — and why it must stay optional

Cloudflare is attractive because it can make a global service easy for non-technical users and small operators:

- Workers/Durable Objects can host relay logic;
- R2 can hold encrypted blobs;
- Realtime can provide STUN/TURN/SFU.

This reduces operational friction, which is important for OM's mainstream-user goal.

But if the wire protocol assumes Cloudflare identifiers, storage semantics, or media APIs, OM simply trades one central operator for another. Therefore Cloudflare is a reference provider profile only.

For the initial deployment model, ordinary users may own a free Cloudflare account so their infrastructure is individually controlled, but OM must automate provisioning and hide Worker/DO/storage details. Owning the provider account is different from administering it. Cloudflare remains replaceable and outside canonical identity/protocol semantics. *(2026-09-24: superseded as the default path by Home tiers, ADR-0102/0112: most users start on a sponsored or hosted Home with no account; own-cloud is optional and deploys only verified releases, ADR-0130.)*

## 9. Why R2 is storage, not “the OM server”

R2 is useful for encrypted files because blob storage and egress economics fit attachments/archive. Tiny message queues have different access/expiry/write patterns and belong in relay queue storage.

Separating these roles keeps the architecture portable and lets each provider type be optimized independently.

## 10. Why direct/P2P/LAN matters for calls

Initially it was tempting to relay calls by default to hide peer IP addresses. That maximizes network privacy but makes the service pay for every media byte and creates avoidable infrastructure dependence.

The better solution is adaptive routing:

- trusted accepted peers: try LAN/direct P2P when policy permits;
- strict privacy or unknown caller: use relay-only candidates;
- difficult NAT/firewall: TURN fallback;
- larger groups: SFU where mesh becomes inefficient.

This makes infrastructure a safety net instead of the default data plane.

## 11. Why keep SFrame even for direct calls

WebRTC already encrypts transport, so direct calls might appear to need no extra media layer. But if encryption depends on route, switching from P2P to TURN/SFU changes the trust model.

SFrame gives OM one E2EE media property above the transport. The call can move among LAN, P2P, TURN, and SFU without granting the intermediary media keys.

That consistency is more valuable than optimizing away one encryption layer.

## 12. Why LAN/local-first is more than cost optimization

Keeping local traffic local saves bandwidth and latency, especially in regions with expensive or unreliable Internet. It also increases resilience: a local router can remain functional during WAN outages.

Long-term OM should allow established peers on the same LAN to continue communicating without cloud media transit. The difficult part is private local rendezvous: ordinary mDNS device-name broadcasting can leak identity, so discovery should eventually use rotating relationship-derived identifiers or another reviewed privacy-preserving design.

## 13. Why group calls use adaptive mesh/SFU

Full mesh avoids infrastructure but scales poorly: every participant uploads multiple copies and mobile CPU/battery suffer. An SFU scales better but consumes provider egress.

Therefore group topology should be automatic based on participant count, uplink, CPU, battery, packet loss, and privacy/cost policy. Very small groups can mesh; larger/constrained groups move to E2EE SFU.

## 14. Why provider cost becomes part of routing policy

A “free tier” is not a uniform concept:

- some free services hard-fail after limits;
- some begin billing overages;
- some have daily resets, others monthly;
- reads/writes/egress can have different accounting.

OM must not assume the provider will protect the operator from charges or failure. Provider adapters expose cost/quota state and local RoutePolicy/StoragePool/RelayPolicy acts before thresholds.

“Spend $0” should be enforceable behavior.

## 15. Why SMS/RCS moved later

SMS/RCS interoperability remains valuable, especially for an eventual unified Android messenger. But access is constrained by app-store, platform, carrier, and provider policies. Making it a launch prerequisite would allow external approvals to stall the secure OM-native system.

The solution is architectural preparation without implementation dependency:

- define TransportAdapter now;
- ship OM-native first;
- add SMS/MMS later where approved;
- add RCS when APIs/provider access make it practical;
- never silently downgrade an encrypted OM message to a weaker carrier transport.

## 16. Why calls can come before SMS/RCS

Calls use open WebRTC-class technology and can build directly on OM identity/relationships/groups. They do not require becoming a carrier's default messaging client. Once messaging/groups/sync are stable, calls provide major user value without waiting for telecom/store approvals.

## 17. Why abuse controls belong partly on servers even in open source

If OM only implements rate limits in the official UI, an attacker can remove them from a modified client. Therefore expensive-resource protections and first-contact quotas must be enforceable at relay/recipient/provider boundaries.

This is compatible with privacy: a relay can reject oversized/rate-excess ciphertext without reading message content.

## 18. Why reporting is deliberate disclosure rather than ambient scanning

A recipient already possesses plaintext they received. A report can intentionally disclose selected abusive messages and authenticated evidence to a chosen service. This is materially different from building a mechanism that makes every private conversation inspectable at all times.

The report cryptographic design still needs expert treatment because authorship proof and messaging deniability can conflict.

## 19. Why no global reputation score

A universal trust/reputation score would create a new cross-provider identity authority and could become difficult to escape or correct. OM instead allows local/provider/community decisions and optional subscribed safety data without making one global score necessary for protocol validity.

## 20. Why OM itself must be replaceable

Even perfect decentralized infrastructure can be centralized in practice if OM's maintainers control the only usable client, the only update key, or the only compatibility registry.

Public specifications, test vectors, conformance suites, independent clients, transparent profile/version governance, and eventually stronger release-signing controls are part of the decentralization story.

The real test is whether compatible communication can continue if the original OM organization disappears.

## 21. The user-facing architecture

Engineers see:

```text
AccountID
DeviceIdentity
CryptoProfiles
PQ/hybrid ratchets
MLS
capability relays
StoragePool
SFrame
ICE/STUN/TURN/SFU
provider cost policy
transparency witnesses
transport adapters
```

The user should see:

```text
John
────────
Hi 👋

📞  🎥
```

That difference is intentional. Complexity is justified only when it buys a property users receive automatically.

## Discovery checkpoint rationale — 2026-09-21 10:08 PHT

The discovery discussion began with a practical question: if two users independently run Cloudflare-backed OM infrastructure and one knows the other's username, how does one device find the other without recreating a central controller?

The key realization was that the useful analogy to DNS is architectural rather than literal. DNS demonstrates the value of separating human naming from network location, separating resolvers from authoritative sources, delegating operation, caching bounded-lived answers, advertising multiple endpoints, and supporting local discovery. OM can adopt those properties while avoiding a mandatory single global OM root and improving privacy with cryptographic descriptors and oblivious lookup.

This produced the `ContactDescriptor` abstraction: every discovery mechanism—directory handle, domain alias, QR/link, contact card, NFC, or LAN rendezvous—converges on a validated rendezvous descriptor. The descriptor gets peers to a constrained introduction plane; it is not identity and not the normal mailbox.

A second problem was revocation/spam. A self-contained public QR that permanently grants write access cannot be safely published by a high-profile user. OM therefore separates revocable `Invite` capabilities from public `IntroductionServiceDescriptor` records. Public discovery obtains a short-lived/single-use `IntroductionTicket` from an `IntroGate`, allowing the recipient/provider to rate-limit or challenge abusive traffic without decrypting normal conversations. Accepted peers receive unrelated private relationship delivery capabilities.

A third refinement was redundancy. OM now treats multiple simultaneous relays as a first-class private `RelaySet`, with distinct capability identifiers per relay and automatic failover. This borrows the resilience idea of multiple DNS/service endpoints without exposing a stable mailbox across providers.

Finally, local-network and censorship resilience were pulled into the same architectural family. Existing contacts can derive rotating LAN rendezvous tokens instead of broadcasting stable identity. OM application semantics remain above a pluggable transport layer so future anti-censorship mechanisms can evolve independently. The goal is graceful resilience and removal of single choke points, not a claim that OM can defeat every state-level censor.

## Continuity / trust rationale — 2026-09-21 21:32 PHT

The recovery discussion clarified that OM should not impose durability on people who want throwaway identities. Making recovery optional avoids creating a universal provider backdoor. *(2026-09-24: ADR-0111 later made identity recovery on by default, as one vote with delay and veto, with an explicit opt-out; it is still never a provider master key.)* A random offline root and guardian shares are understandable recovery paths; making recovery itself a versioned policy keeps room for future threshold/MPC research.

The revocation discussion exposed a deeper issue: if any ordinary device can permanently revoke all others, compromising one device converts immediately into account ownership. OM therefore separates emergency containment from constitutional authority. Quarantine can stop future secrets; it cannot change its own quorum denominator. Likewise, linking a new device cannot silently create a new controller. This chooses temporary denial-of-service risk over silent permanent takeover.

The initial deployment premise also became clearer *(and was later replaced by Home tiers, ADR-0102)*: each user owns their Cloudflare account and OM automates it. This changes Cloudflare from "central default infrastructure" into a mass-market self-hosting substrate. `workers.dev` removes the domain-purchase requirement for individuals, while organizations/FQDN owners naturally form the first grouped directories. Research showed that requiring R2 would undermine a truly no-payment onboarding path, so the core was reduced to Worker + SQLite Durable Object and R2 became an optional blob expansion.

Contact sharing becomes the human analogue of authenticated DNS caching: Carol can forward Alice's Alice-signed bounded-lived ContactCapsule to Bob. Carol can say "I know this person" without becoming Alice's certificate authority or creating a global reputation graph.

Software distribution was elevated because endpoint code is the practical master key to an E2EE system. Open source alone does not stop a compromised build pipeline. Trust therefore starts with role-separated update authorization, provenance and reproducible artifacts where platforms allow it, while mirrors ensure no forge owns protocol/source truth.

Finally, the product posture was refined: OM is a general messenger with strong defaults, not a product that forces every user to behave like a dissident or cryptographer. Privacy presets tune operational metadata/convenience; they do not select weaker cryptography. Vendor push remains usable through opaque wakeups. Users who disable it are told the real platform tradeoffs rather than promised impossible background behavior.
