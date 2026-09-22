# Architecture Question Resolutions — 2026-09-23 05:45 PHT

- **Status:** Canonical accepted architecture decisions for former Q-001..Q-061
- **Engineering:** still not authorized; proof gates are tracked separately

The former `open_questions.jsonl` mixed design choices, proofs, release checks and product measurements. This checkpoint closes the architectural choice portion and moves the remaining work into typed machine ledgers.

## Q-001

Freeze OM-Crypto-v1 around dual durable Ed25519 + ML-DSA-65 constitutional signatures where durable account/control authentication is required; asynchronous PQXDH using X25519 + ML-KEM-768; the published Triple Ratchet design (Double Ratchet + SPQR/ML-KEM Braid class); HKDF-SHA-256/SHA-256; a misuse-resistant AEAD profile. Current PQXDH mutual authentication remains classically grounded rather than inventing an unreviewed PQ-authentication construction. Exact implementation path remains a pre-engineering proof gate.

## Q-002

Private groups use RFC 9420 MLS. OM-Crypto-v1 mandates the standard X25519/AES-128-GCM/SHA-256/Ed25519 MLS suite; OpenMLS is a candidate adapter, never protocol authority. PQ/hybrid MLS remains experimental until standardized and implementation-mature.

## Q-003

Recovery is off by default. Enabled recovery uses a random 256-bit OfflineRecoveryRoot with domain-separated HKDF derivation. Optional guardians use reviewed Shamir N-of-M shares bound to AccountID, recovery-set generation and share index. Authority recovery and history recovery remain separate; MPC/threshold signing is reserved, not required.

## Q-004

v1 has no global OM transparency authority. Use relationship-scoped AccountControlHead gossip, direct verification and optional independent witnesses; public directory transparency is per-directory and replaceable.

## Q-005

Signed/hashed protocol objects use deterministic CBOR per the OM RFC-8949 profile plus CDDL/golden vectors; no floats in canonical signed objects; small integer field IDs; exact absent/null semantics; signatures cover canonical bytes with signature fields excluded by schema.

## Q-006

Relay write/deposit and recipient drain/read capabilities are separate high-entropy scoped rotatable bearer capabilities. Relays handle opaque envelopes; migrations overlap old/new capabilities; delivery IDs, padding/jitter and outer wrapping avoid exposing inner EventID.

## Q-007

New devices receive explicit DeviceBootstrapGrant and use a same-account authenticated E2EE sync channel. Immutable events merge by EventID union; authoritative account/group state uses its dedicated state machine, never generic LWW/CRDT merge. History snapshots are encrypted chunked/hash manifests with resumable idempotent transfer.

## Q-008

BlobID is stable identity of the inner encrypted object. Per-provider wrapping/locations are mutable and separate. Decryption material travels only inside E2EE events; BlobLocationRecord supports multiple locations, repair and re-homing without changing message identity.

## Q-009

EvidencePackage is optional local export: selected canonical events/hashes, relevant credential/control/group proof and an exporting-device observation statement. It supports selective disclosure and must not be described as universal non-repudiation.

## Q-010

1:1 calls derive call-scoped media secrets from the existing secure session with explicit exporter labels, CallID and participant DeviceIDs. Group calls use the MLS exporter bound to CallID and MLS epoch; derive directional/per-sender SFrame keys; membership/epoch changes create a new media epoch; never reuse a media key with restarted counter.

## Q-011

Automatic LAN discovery is only for already-related users/devices. Derive short-lived HMAC rendezvous tags from pairwise LAN secrets; never advertise stable AccountID/DeviceID. Native discovery may use mDNS/DNS-SD while QR/manual local endpoint remains universal fallback.

## Q-012

ProviderCapabilities separates function/limits from untrusted failure-domain claims. Client routing uses measured health with hysteresis and user policy; Recommended uses active+fallback and Hardened seeks independently evidenced diversity. Provider health never grants protocol authority.

## Q-013

Protocol/CryptoProfile registries are public, versioned and machine-readable. IDs are never reused; published CryptoProfiles are immutable and superseded by new profiles. Experimental/private UUID/namespaced extensions do not require SeedLife permission; canonical standard IDs require public ADR + conformance.

## Q-014

ContactDescriptor is deterministic-CBOR and carries AccountID, authorized device/control-head proof, discovery endpoints and validity interval. It is signed by a currently authorized device and bound to AccountControlState. Alias bindings sign exact normalized alias+namespace+AccountID; provider signatures are provenance only.

## Q-015

There is no global OM directory. DirectorySet is user/client selected and merged locally. Exact alias match is distinguished from fuzzy suggestion. A directory may run its own append-only transparency log/witnesses, but no global directory root is protocol authority.

## Q-016

Invite/IntroductionTicket are random scoped capabilities with issuer authentication, explicit expiry/use count and IntroGate scope. Single-use tickets are provider-scoped by default; a multi-provider invite can hold distinct capabilities. Revocation is pushed to relevant IntroGates; no claim of impossible instantaneous offline revocation. Privacy-Pass/ZK admission remains optional future extension.

## Q-017

RelaySet is signed, versioned and hash-chained. Migration is add-new -> dual-operate -> distribute -> peer acknowledgement -> drain-old -> revoke-old. Recommended uses primary delivery with timed hedging; Hardened may parallel-send with independently wrapped/padded envelopes.

## Q-018

LAN rendezvous secrets are HKDF-derived from relationship-specific secrets, never identity private keys. Discovery tags rotate in short epochs (15-minute reference seed with adjacent-epoch tolerance); exact timing is profile configuration. LAN carries normal OM E2EE ciphertext rather than a second message-crypto protocol.

## Q-019

NetworkTransport is a capability interface: request/response, bidirectional stream, cancellation, proxy route and metadata limits. HTTPS/WebSocket is universal baseline; QUIC/HTTP3, SOCKS/CONNECT, Tor/community transports are adapters. A transport is an untrusted byte carrier.

## Q-020

AccountControlState is hash-chained with generation+parent hash. Default controller threshold is floor(N/2)+1. Any controller may temporarily quarantine, but quarantine never reduces authorization denominator and cannot permanently revoke/promote/change thresholds. Conflicting valid descendants cause safety freeze, never LWW.

## Q-021

AccountControlHead gossip travels only through authenticated relationships. Peers retain recent heads and detect equivocation. Fork resolution must explicitly acknowledge conflicting branches and be authorized from the common-ancestor policy; newest timestamp never wins authority.

## Q-022

Cloud/reference-provider thresholds are benchmarked operational values, not protocol constants. Large blobs do not belong in small SQLite/relay state merely to fit a reference free tier.

## Q-023

Use TUF-style update trust, in-toto/SLSA provenance, reproducible-build evidence and independent mirrors. Even during founder-only custody, keep a physically separated 2-of-3 offline release root under the founder until trustworthy custodians exist; online delegated keys handle routine releases.

## Q-024

Privacy presets never change cryptographic strength. Recommended enables ordinary private conveniences; Hardened reduces metadata and uses stronger provider diversity/proxying; Maximum disables automatic directory/link-preview/presence/typing/receipts where practical and uses stronger padding/batching. Manual overrides yield Custom.

## Q-025

Push is opaque wakeup only: no sender, conversation, AccountID, plaintext or message type. Gateway maps rotating opaque subscription handles to APNs/FCM/WebPush and batches/collapses where practical.

## Q-026

v1 canonical disappearing timer is absolute after-send expiry. “After I read” is local retention unless a later explicit extension is adopted. Expiry destroys local attachment keys/removes locations and leaves a minimal tombstone so old sync/backup cannot resurrect content. Ephemeral content is excluded from ordinary history backup by default.

## Q-027

Core integer extension IDs are public-registry assigned; third-party/experimental extensions use collision-resistant namespaced/UUID identifiers without permission. Extensions carry explicit criticality. Unknown noncritical data is semantically ignored but preserved where required; unknown critical semantics fail safely.

## Q-029

Rust remains canonical om-core. Typed logical APIs sit above it; web uses WASM bindings, native uses generated Swift/Kotlin/FFI bindings with a narrow C ABI escape hatch. CryptoProvider exposes opaque key handles/operations, not universally exportable long-term private-key bytes.

## Q-030

A personal ClientHost/PushGateway owns its own VAPID identity unrelated to AccountID. The VAPID private key remains provider-secret state and is rotatable; browser subscriptions remain per-device opaque mappings.

## Q-031

PWA must not promise native-grade background call ringing. Web Push can notify and open/join an active call; native shells add platform call/background integration without changing call protocol semantics.

## Q-032

Merged into Q-009 EvidencePackage semantics; there is one evidence-export model rather than a second overlapping architecture question.

## Q-033

During founder S0, disappearance may end the canonical SeedLife release line but must not end the protocol: licenses, mirrors, specs, vectors and build instructions permit a community fork/new trust root. Later trusted maintainers distribute repository/release/domain custody and run succession drills.

## Q-034

Country-specific protocol gating is removed. Legal review is jurisdiction-neutral and role-based for any project-controlled distribution/service; no country is treated as OM’s protocol jurisdiction.

## Q-035

Store/reporting/moderation obligations belong to the distribution that elects to ship through that store. Core exposes block/report/export primitives without creating a universal moderation service or decryption authority.

## Q-036

Provider migration state machine is PREPARE -> MIRROR -> VERIFY -> ANNOUNCE -> OVERLAP -> DRAIN -> REVOKE. Roles migrate independently; encrypted objects copy/re-home; push rebinds opaque endpoints; rollback remains possible until revoke.

## Q-037

UI framework is deliberately not protocol architecture. Keep TypeScript/web standards/design tokens/Core contract framework-neutral and choose by future benchmark.

## Q-038

One shared om-ui remains a target rather than constitutional law. Prototype it and allow thin native surfaces where calls/camera/share/auth/accessibility/text input require native quality.

## Q-039

Use adaptive/container layout rather than device-name breakpoints. Compact/Medium/Expanded/Wide values are seeds validated later against localization, large text, foldables, split windows and resizing.

## Q-040

Themes are inert declarative data/resources only, never executable markup/code. Resource formats/counts/decoded sizes are bounded. Signatures prove provenance, not safety. Per-chat theme selection synchronizes only across the same user’s devices unless a separate shared-space style feature is explicitly adopted.

## Q-041

Bundle Inter Variable where script coverage/license fit; use system/script-aware fallback and optional Noto subsets. Font packaging is not protocol semantics.

## Q-042

Reduced-motion and “motion never carries essential meaning” are architectural accessibility rules; exact durations/easing/performance constants are deferred measurement tokens.

## Q-043

WebDeviceVault hierarchy is WebAuthn PRF -> non-extractable WebCrypto root -> unsupported for durable production identity if neither secure path exists. PRF derives wrapping material; it is not DeviceIdentity. Password may augment locking, not replace cryptographic storage.

## Q-044

Prefer an OM-owned Rust implementation of published PQXDH/Triple-Ratchet state machines over unsupported libsignal API coupling, using independently reviewed primitive libraries. This architecture is accepted only after a native+WASM feasibility/interoperability/persistence proof gate.

## Q-045

Field IDs are machine-registry allocated and never reused. Signed/KDF/hash contexts use exact ASCII domain strings in an OM/<object>/<purpose>/v1 style plus CryptoProfile binding, frozen before wire release.

## Q-046

Recommended may use a user-selected privacy LinkPreviewProvider/proxy; Hardened is manual/proxied; Maximum has no automatic preview. Never trust sender HTML/preview rendering as executable content.

## Q-047

StorageAdapter abstraction is frozen: native defaults to SQLite; web starts with IndexedDB and may adopt OPFS-backed SQLite for large histories after benchmarks. Backup/wire semantics never depend on local DB choice.

## Q-048

Built-in theme names/visual tokens are a future product choice, not an architecture blocker.

## Q-049

Each device publishes versioned HandshakeBundles and MLS KeyPackages. One-time consumables are never identically mirrored across independent providers; each provider gets disjoint pools. Reusable last-resort keys are explicitly marked and independently generated per provider; bundles carry generation/expiry/key IDs/authorization proof.

## Q-050

Maintain one 1:1 session per (local DeviceIdentity, remote DeviceIdentity, CryptoProfile), with explicit Absent/Initiating/Active/Stale-or-ResetPending/Retired and quarantine behavior. A logical EventID sits above device envelopes. Crossed initiation may temporarily coexist; SessionID disambiguates and deterministic ordering selects preferred future outbound session while retaining in-flight decryptability.

## Q-051

Each group has a CommitCoordinator DeviceIdentity that serializes MLS Commits for the accepted OM group-control state. Coordinator sequencing grants no authorization power. Admin policy authorizes coordinator replacement; equivocation/fork detection triggers safety freeze.

## Q-052

GroupControlIntent carries the authorized application change; GroupCommitBinding binds it to exact MLS Commit, prior/result epochs/tree state and resulting group state. Persist OM control state + MLS state atomically via crash-safe journal and idempotent transition IDs; neither half becomes visible alone.

## Q-053

Add optional relationship-secret route recovery. Established peers derive RouteRecoverySecret and rotating opaque lookup handles; encrypted/signed latest RelaySet may be published at one or more replaceable rendezvous stores. Only a peer holding the relationship secret can locate/decrypt it.

## Q-054

Message carries stable BlobID plus opaque repair capability/token. BlobLocationRecord maps authorized repair lookup to current locations. Recipients verify downloaded ciphertext against BlobID, so malicious repair data can cause availability failure but not substitute content.

## Q-055

Canonical time has four classes: HLC ordering only; monotonic local duration; wall-clock human expiry; provider retention clock. Time never establishes authority. Expiry/skew/bad-clock behavior is field-specific, and restore re-evaluates expiry before showing content.

## Q-056

ClientHost releases are signed/immutable and discovered through independent update metadata/mirrors. Prefer an external deployment controller that verifies before replacing the origin. Service-worker verification is defense-in-depth, not protection against malicious origin. Suspected-origin compromise uses new-origin/new-device enrollment then revocation.

## Q-057

Normative protocol contributions must disclose known essential patent claims and grant an irrevocable worldwide royalty-free implementation license/non-assert sufficient for conforming implementations, with ordinary defensive termination. No royalty-bearing/permission-gated mechanism becomes mandatory OM.

## Q-058

Trademark may distinguish official/canonical builds and prevent impersonation, but policy must allow truthful statements such as OM-compatible/implements OM/works with OM. Trademark is never an interoperability license.

## Q-059

Link-preview network sandbox permits only bounded HTTP(S), rejects private/loopback/link-local/reserved/metadata destinations including mapped variants, validates DNS and every redirect, resists rebinding, caps redirects/time/bytes/decompression/image dimensions, sends no ambient cookies/auth and runs behind egress controls.

## Q-060

LocalDiscoveryProvider is capability-gated. Native may use platform discovery; universal fallback is QR/manual pairing or known local endpoint. No browser local-network API is mandatory protocol surface.

## Q-061

Provider diversity uses evidence dimensions (operator, ASN/network, cloud/vendor, DNS zone, region/jurisdiction, implementation family) with evidence levels such as self-claimed/observed/third-party-attested. Routing may request diversity across dimensions but never claim mathematically proven corporate independence.


## 07:18 PHT — Extension/ecosystem hardening

A-001/A-002/A-003/A-004/A-009 are resolved at architecture-design level. OM now has bounded inert expressive-media profiles; portable data-only content packs with signed update provenance; a generic optional entitlement proof boundary with commerce external; typed ServicePrincipals for bots/agents/LLMs with selected-context default and explicit full-Room plaintext membership; and a capability-sandboxed plugin model built around distinct protocol/data/Wasm/web/service/provider extension classes. `ConversationEvent` authorship is generalized from human AccountID/DeviceIdentity fields to `PrincipalRef + author_endpoint_id`. V-010..V-012 retain implementation/adversarial proof as blocking verification work.
