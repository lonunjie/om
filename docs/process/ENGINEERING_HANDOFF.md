# OM Engineering Handoff — DESIGN-FROZEN, INACTIVE UNTIL NAMED-SCOPE AUTHORIZATION

> **Final design checkpoint: 2026-09-26 / ADR-0139.** Architecture design is closed. Product engineering is **not** authorized by this document. In plain terms: product engineering is not authorized at this checkpoint. [`machine/repository_contract.json`](../../machine/repository_contract.json) is authoritative for scope unlock.

## 1. What “design-frozen” means

The protocol/security/product architecture is sufficiently specified to stop making new architecture choices by default. It does **not** mean the implementation verification gates have passed. Future implementation work should consume the design, measure it and challenge it with evidence; it should not silently substitute new authority/privacy/security semantics.

A contradiction discovered during implementation is valuable evidence. Preserve safe behavior, write an ADR and update the machine ledgers/conformance story before changing the contract.

## 2. Source-of-truth precedence

When two artifacts appear inconsistent:

1. constitution, [`AGENTS.md`](../../AGENTS.md), [`machine/repository_contract.json`](../../machine/repository_contract.json) and current machine truth;
2. accepted current ADRs + canonical architecture documents;
3. for a frozen W0 object, **CDDL + frozen registries + domain-separation/signature/hash rules define the accepted wire bytes**;
4. logical JSON schemas/models are documentation/tooling views and cannot add fields/critical-extension/encoding semantics to frozen W0 wire objects;
5. conformance vectors and bounded evidence;
6. implementation detail.

Do not resolve a true conflict by choosing the convenient interpretation. File an ADR.

## 3. Non-negotiable architecture boundaries

Engineering MUST preserve at least these properties:

- no mandatory OM/SeedLife-operated runtime service for established independent communication;
- AccountID is stable identity, never a routing address and never derived from replaceable provider/key coordinates;
- AccountControlState is the constitutional authority object; recovery/provider/directory/service/bridge/plugin/update roles do not become alternate account authority;
- accepted relationships use opaque rotatable routing/admission capabilities rather than stable canonical IDs;
- every inbound 1:1 decrypt/receive has mandatory relationship context and binds sender identity before secret/session/plaintext processing;
- AccountControl transitions follow the normative machine policy; stale reductions target immutable incarnations; control state never uses chat HLC/LWW/ordinary CRDT authority merge;
- backups/imported archives restore data/evidence, not stale authority or routing state;
- provider administration uses narrow opaque provider-resource capabilities; those capabilities never substitute for end-user authorization;
- RecoveryRecord storage is opaque/capability addressed; recovery/legacy powers remain narrower than normal controller authority;
- strongest approved CryptoProfile behavior is automatic; profile/privacy/notification settings never silently select weaker message crypto;
- **SecurityAdvisory expiry, supersession or override never lowers the relationship's stored security floor**;
- metadata claims are observer/surface specific; same-operator roles collude by construction; cryptographic unlinkability is not advertised as transport anonymity;
- extensions/services/bridges are explicit principals/recipients with scoped capabilities and visible provenance; plaintext read and network egress are distinct powers;
- user themes/expressive assets are inert data, not executable extensions;
- v1 MLS groups are classically protected/transferably authenticated as specified; do not claim PQ confidentiality or group deniability that the profile does not provide;
- published wire generations remain independently interpretable/mirrorable; incompatible semantic evolution uses explicit version/profile/fork mechanisms rather than rewriting old meaning;
- the Creator-Disappearance Gate remains satisfiable.

The compact index is [`SECURITY_INVARIANTS.md`](../architecture/safety/SECURITY_INVARIANTS.md); its machine trace is [`machine/security_invariant_traceability.json`](../../machine/security_invariant_traceability.json).

## 4. Wire and crypto implementation rules

- Never infer signed/hashed bytes from a permissive logical schema when a W0 CDDL/registry is frozen.
- Domain-separation labels are protocol data, not refactoring strings.
- Unknown/critical-extension behavior must follow the frozen registry rules.
- Do not invent new cryptographic constructions to make an implementation easier. Use only the approved/profiled constructions and reviewed dependency path.
- `OMA-ENC-1-candidate` is verification-only. It is **not** permission to ship archive encryption. V-014 must freeze the reviewed production archive profile first.
- Guardian recovery uses only the reviewed method/profile admitted by the current RecoveryPolicy. No bespoke threshold cryptography.

## 5. What engineers may choose without reopening architecture

Inside an authorized scope, normal engineering choices include:

- internal data structures, storage engines and caching strategies that preserve canonical semantics;
- languages/libraries/deployment packaging consistent with licensing, supply-chain and approved-crypto constraints;
- retry/batching/queue algorithms that preserve idempotency, limits, ordering and fail-closed authority behavior;
- UI component implementation/details that preserve mandated user-visible security semantics and accessibility;
- provider deployment/operations details within the role/capability/privacy contracts;
- performance optimizations that do not change wire/security/authority meaning;
- test/fuzz harness structure;
- non-wire internal schemas and migrations.

An implementation choice becomes an architecture change if it changes who can authorize, what an observer learns, which bytes authenticate, when a downgrade is accepted, how recovery/legacy becomes effective, which principal authored an action, or how independently operated implementations interoperate.

## 6. Scope map and principal evidence gates

The machine ledger is authoritative; this summary is orientation only.

| Scope | Wave | Principal gate families |
|---|---:|---|
| `core_kernel` | W0 | V-001/002/003/005/008/009/011/013/014/015/016/019 plus always-required V-017 |
| `reference_client_ui` | W0 | V-006/018/021/022/023 + dependency on core |
| `provider_reference` | W0 | V-015/020/022 + provider conformance + dependency on core |
| `groups` | W1 | V-004/009/019 + core dependency |
| `portability` | W1 | V-014 + core dependency |
| `expressive_media` | later wave | V-012 |
| `calls` | later wave | V-007 |
| `communities_publications` | later wave | V-008/016 |
| `services_bots` | later wave | V-011 |
| `plugins` | later wave | V-010 |
| `bridges` | later wave | V-013 |

Do not copy this table into authorization logic; use [`machine/repository_contract.json`](../../machine/repository_contract.json) and current gate records.

## 7. Recommended first authorized engineering sequence

Once the relevant scope is actually authorized, preserve this dependency direction:

1. repository/toolchain/reproducibility foundation;
2. canonical IDs/data model/deterministic encoding/parser limits;
3. AccountControlState implementation against the frozen transition contract and evidence vectors;
4. DeviceVault/profile isolation and local encrypted-state foundation;
5. reference Home/discovery vertical slice using provider-resource capabilities;
6. relationship/event vertical slice with mandatory relationship context;
7. hostile content/blob/local lifecycle boundaries;
8. approved production crypto integration through its dedicated gate;
9. push/PWA operational behavior;
10. multi-device and recovery;
11. later scopes only after their dependencies/gates authorize them.

This sequence is not permission to start now.

## 8. Product/UX implementation rules

- Render local state quickly, but never fake irreversible security success.
- Optimism is allowed only where semantic rollback is safe.
- Provider/crypto jargon stays out of default UX unless the user is making a real security choice.
- Warnings must match the actual guarantee: endpoint/platform leakage and metadata exposure are not cured by E2EE marketing.
- Profile/account isolation is built into local namespaces even if the first UI exposes one profile.
- Telegram-like rich text uses OM's safe semantic representation, never arbitrary HTML/Markdown execution on the wire.

## 9. Update/distribution and anti-capture rule

A ClientHost, app store, package mirror, provider, directory, transparency log, safety-advisory publisher or catalog may be important operational infrastructure, but it does not acquire constitutional protocol authority merely by being popular or bundled. Verified releases and transparency/monitoring are supply-chain defenses; they are not a kill switch.

## 10. Architecture-change rule

When reality contradicts the design:

1. reproduce/measure the contradiction;
2. fail closed where a security decision is involved;
3. write an ADR explaining the evidence and alternatives;
4. update requirements/threats/security invariants/migrations/conformance/gates as applicable;
5. obtain any required independent review;
6. then implement the accepted change.

Never ship a quiet implementation-specific exception and document it afterward.

## 11. Current lock

At this checkpoint:

- architecture design queue: closed;
- verification gates: still ledger-owned/open unless explicitly passed/waived there;
- engineering scopes: unauthorized;
- global `engineering_authorized`: false.

The next phase is evidence and named-scope authorization, not additional speculative architecture hardening.
