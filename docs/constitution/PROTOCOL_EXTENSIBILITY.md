# Protocol Extensibility and Capability Evolution

## 1. Principle

OM must evolve without making every new feature a flag day and without allowing unknown security requirements to be silently ignored.

## 2. Extension semantics

Every extension/field that can affect interpretation has an explicit criticality class:

- **optional / non-critical:** an older implementation may ignore it while preserving authenticated bytes where forwarding/storage semantics require preservation;
- **critical / mandatory-to-understand:** an implementation that does not understand it MUST refuse the affected operation rather than continue with weaker/different semantics.

Security-relevant authentication, membership, crypto, recovery, or authority semantics default to critical.

## 3. Negotiation

Capability negotiation is authenticated and bound into the relevant session/group/control transcript. A network attacker must not be able to strip capabilities and force an older profile.

Clients maintain minimum-observed security/profile floors where applicable.

## 4. Registries

Maintain versioned public registries for:

- CryptoProfiles;
- canonical event/control types;
- transport/provider capabilities;
- critical extension IDs;
- compression/content formats where security-relevant.

Reserve experimental/private ranges. Registry governance must be public and cannot make one implementation mandatory.

## 5. Canonical encoding

Before wire freeze, define deterministic/canonical serialization rules for every signed/hashed object and publish golden vectors. Unknown-field preservation behavior must itself be testable.

## 6. Deprecation

Lifecycle:

```text
experimental -> supported -> preferred -> deprecated -> forbidden
```

Deprecation can be date/policy/profile driven, but forbidden cryptographic profiles cannot be silently resurrected by old backups or relays.

## 2026-09-23 registry/extension resolution

Canonical IDs are public-registry assigned and never reused; experimental/private extensions use collision-resistant namespaced/UUID IDs without permission. Critical unknown semantics fail safely; noncritical unknown data is ignored semantically and preserved where required.

The 07:18 extension pass adds an anti-fragmentation rule: **shared canonical meaning cannot exist only inside opaque plugin code.** Any extension that changes interoperable conversation/control state must publish its protocol schema, criticality, canonical encoding and capability semantics independently of one executable plugin. A plugin may implement or present such an extension, but absence of that plugin must not make baseline OM undecodable or hand a vendor special protocol rights. Inert content packs are data, remote services are typed principals, and local executable plugins are capability-sandboxed implementation modules; those categories do not substitute for the protocol-extension registry.

## 2026-09-23 08:21 longevity/registry resolution

`PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md` resolves A-015.

Already-assigned registry identifiers are immutable and never reused for incompatible meaning. Published protocol generations/profile bundles snapshot the registry material needed to interpret them so runtime validity never depends on consulting a live OM Project registry server.

If stewardship diverges, successor registries/profiles use an explicit new lineage/identifier namespace. A vendor-private extension may remain private, but market adoption does not grant it canonical status. Graduation into a future baseline requires public independently implementable semantics, canonical encoding/criticality, conformance behavior and acceptable implementation rights.
