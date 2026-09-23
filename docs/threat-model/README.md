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

## Discovery / invite / censorship threats added 2026-09-21

- **Global-directory coercion/censorship:** mitigated by no mandatory global registry, multiple namespaces/providers, direct invites, cached descriptors and accepted relationships independent of discovery.
- **Directory equivocation/impersonation:** signed short-lived records plus planned transparency/witness/gossip; discovery remains separate from AccountID authentication.
- **Lookup surveillance:** optional oblivious relay/resolver separation; do not upload full social graph as default.
- **Leaked public handle/descriptor:** descriptor is not a reusable private mailbox capability; IntroGate issues constrained tickets.
- **Leaked QR/link:** independent InviteID/capability is revocable/expiring; accepted relationships use unrelated capabilities.
- **Introduction spam/DDoS:** ticket expiry/replay defense, quotas, rate limits, optional privacy-preserving admission proof/proof-of-work, provider DDoS controls.
- **Cross-relay correlation:** separate capability identifiers and independently randomized outer envelopes; multi-relay redundancy used selectively.
- **LAN tracking:** rotating rendezvous tokens derived from purpose-specific relationship/invite secrets; no stable AccountID/name broadcast.
- **State/network censorship:** provider and transport diversity, alternate relays, direct/LAN paths and optional pluggable transports reduce choke points; no guarantee against a sufficiently powerful censor/global observer.

## Continuity / supply-chain threats added at 21:32 checkpoint

- **One compromised controller revokes everyone:** quarantine cannot shrink its own constitutional denominator; permanent mass control changes require parent-state quorum/recovery.
- **Attacker adds devices to manufacture quorum:** messaging-device enrollment is separate from control-device promotion; promotion is constitutional.
- **Mutual/concurrent revocation:** no LWW; enter control conflict/safety freeze until stronger authorization resolves.
- **Stale provider/backup state:** AccountControlHead gossip/witness observations detect rollback; restored authority remains quarantined until current state is reconciled.
- **Recovery provider becomes master key:** there is no recovery provider. Default recovery custody is the user's own synced passkey or recovery key. Identity recovery always waits out a delay that any live device can veto, and it counts as one controller vote. Identity, provider-metadata, relationship and history recovery use separate keys (ADR-0111, T-041).
- **Guardian collusion/loss:** threshold selection is explicit risk; future threshold/MPC methods remain upgradeable.
- **CI/forge compromise:** signed mirrored release source, role-separated update metadata, provenance and reproducible-build verification where feasible.
- **Vendor-push metadata:** push sees an opaque device wake/timing event but not normal message plaintext/canonical conversation identity; Maximum Privacy may disable it with availability cost.
- **Contact-sharing forgery:** subject signs ContactCapsule; introducer referral is optional context, not subject authentication.
- **Cloudflare bootstrap control:** provisioning OAuth client and workers.dev location are replaceable; neither is AccountID or protocol authority.
- **Cost exhaustion:** reject before expensive storage/media work and enforce hard local budget policy.
## UI / theme / social-engineering threats added 2026-09-22 13:42

- **Malicious theme code execution:** prohibited structurally; theme bundles are declarative data/assets only and validated before use.
- **Theme tracking beacon:** remote theme assets are disabled by default; imported bundles are self-contained and must not access account/provider/conversation identifiers.
- **Security-warning camouflage:** critical semantic states/focus/destructive actions are constrained by OM-owned accessible rendering and cannot be removed/spoofed by themes.
- **System-dialog phishing:** theme/UI content must not imitate native biometric/permission/password surfaces or create overlay hit targets that obscure canonical actions.
- **DOM XSS in PWA:** strict CSP, Trusted Types where supported, no `eval`, minimal third-party script and sanitized rich-content rendering are defense-in-depth requirements.
- **Accessibility-induced security failure:** critical actions remain keyboard/screen-reader operable, non-color-only, readable under text scaling/high contrast and independent of theme quality.
- **Motion confusion/vestibular harm:** motion is semantic, interruptible and reduced-motion compatible; security meaning never depends on animation.
- **Platform semantic drift:** security-sensitive actions go through shared `om-ui` typed intents and `om-core`; native shells do not independently implement authorization semantics.

## Engineering-boundary threats added 2026-09-22 14:34

- **PWA ClientHost compromise:** malicious origin can deliver malicious executable code; mitigate by per-user ownership, public/app origin separation, strict CSP/no remote runtime code, verifiable releases and higher-assurance native option. Do not claim impossible origin-compromise resistance.
- **Cross-profile correlation:** same provider account/subdomain/network/device can correlate identities; Core forbids hidden global person IDs and profile key/capability reuse, while UI documents stronger-isolation options.
- **Profile data bleed:** every query/cache/push/local store is profile-namespaced; dedicated isolation tests are mandatory.
- **Hostile decrypted content:** attachment/media/rich-text bytes remain attacker-controlled; bounded parsing, allowlisted previews, sandboxed platform decoders and fuzzing are required.
- **Link-preview beacon:** direct URL preview can reveal recipient network activity; use policy/provider abstraction and off-by-default behavior in Maximum Privacy.
- **Identifier spoofing:** apply current Unicode UTS #39 confusable/mixed-script detection at first-contact/discovery surfaces without banning legitimate scripts.
- **Encoding malleability/parser DoS:** signed/wire objects use strict deterministic CBOR, no duplicate keys/floats/indefinite lengths and bounded depth/size; reject non-canonical signed encodings.
- **Diagnostic leakage:** default logs remain local/redacted and exclude secrets/capabilities/plaintext/push endpoints.
- **Dependency/supply-chain compromise:** pin dependencies, generate provenance/SBOM, minimize runtime third-party code and enforce update trust floor.
- **Authority-state implementation bug:** AccountControlState TLA+ model and model-derived regression scenarios gate production authority code.
## 2026-09-22 architecture-hardening threats

- **Key-material split-brain:** the same nominally one-time prekey/MLS KeyPackage is consumed independently through mirrored providers. Default mitigation: disjoint consumable pools per provider/failure domain.
- **Multi-device session confusion:** stale/provider-invented device records or crossed initiations create encryption to unauthorized/wrong sessions. Authority must trace to AccountControlState; session management is explicit.
- **MLS same-epoch fork:** two valid Commits race and different members accept different successors. Application-level canonical ordering/conflict handling is required.
- **Group-policy/MLS divergence:** human role/membership state commits without matching MLS state or vice versa. Bind transitions atomically/cryptographically.
- **Total known-route loss:** a peer misses RelaySet migration before every known route disappears. Use overlap/recovery rendezvous or explicitly require rediscovery; never leak normal capabilities globally.
- **Stale blob locator:** late recipient knows BlobID but only dead provider location. Provide authenticated location repair independent from canonical message identity.
- **Clock manipulation/skew:** TTL/HLC/wall-clock values accidentally become authority or resurrect expired state. Apply explicit clock classes; security sequencing remains cryptographic.
- **Compromised PWA origin:** malicious same-origin code abuses DeviceVault operations/update checks. Treat origin as executable security boundary; migrate through an independently trusted path.
- **Link-preview SSRF:** attacker URL reaches internal/private/metadata services or bypasses filters through redirects/rebinding. Enforce destination validation and strict resource bounds.
- **False failure-domain diversity:** several providers declared “independent” share one account/operator/cloud/control plane. Treat declarations as hints and allow policy to require independently evidenced diversity.
- **Steward capture:** project/business/forge ownership changes. Irrevocable licenses, public protocol artifacts, mirrors and forkability prevent project governance from becoming protocol authority.


## 2026-09-24 structured register and new adversary classes

The narrative above is now backed by a structured register: `../machine/threat_register.jsonl` (T-001..). Each entry names adversary, asset, mitigation documents, requirements and verification/release gates, and the validator checks every reference. Architecture additions must add or update register entries (R-178).

Adversary classes added in this checkpoint:

- **Proximate adversary** (intimate partner, family, employer, anyone with brief device access or passcode knowledge): silent device linking, reading unlocked history. Mitigation: non-suppressible device-link notices, device list, quick lock ([`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](../architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md)).
- **Coercive adversary** (seizure, compelled unlock, border inspection): mitigation is minimization (app/profile lock, disappearing defaults, travel posture). OM explicitly does not promise hidden-volume deniability.
- **Platform leakage** (OS/cloud backups, snapshots, keyboards, cloud spellcheck, browser extensions, notification history): per-platform control matrix, V-021.
- **Hosting-account attacker** (phished cloud credentials for a PWA user's ClientHost): equivalent to endpoint compromise for that user. Mitigation: separate least-privilege deploy credentials, phishing-resistant auth, release-hash observation, native option (R-164).
- **Vendor-wide provider action** (one cloud vendor blocking all OM Homes): practical monoculture choke point. Mitigation: Home tiers, second reference provider, diversity drill (V-020, PR-013).
- **Harvest-now-decrypt-later against groups:** v1 MLS groups are classical. Mitigation: honest per-type PQ status, ReInit path, objective adoption triggers (V-019).
- **Uncoordinated vulnerability disclosure / straggler implementations:** mitigation is signed SecurityAdvisories and peer-local floors, with no kill switch (PR-012).
- **Self-verifying design author:** evidence produced only by the design's author. Mitigation: gate independence rules (`../verification/GATE_PASS_CRITERIA.md`).
