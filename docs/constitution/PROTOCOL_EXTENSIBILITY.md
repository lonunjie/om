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

Registered key-23 extension ids (critical and non-critical object extensions) live in the `extensions` list of
[`spec/registries/om-numeric-fields.json`](../../spec/registries/om-numeric-fields.json) and are mirrored by [`om-cbor-profile.json`](../../spec/registries/om-cbor-profile.json) for decoder-visible consistency. A registered ID binds its owning top-level object, exact criticality and payload schema; it is never open to vendor-defined reinterpretation. A wire criticality different from the registry's is malformed and rejects the containing object. For a **critical** registration, a wrong owning object or invalid payload also rejects the object. For a **non-critical** registration, a wrong owning object or invalid payload makes the extension unusable: it is ignored and its bytes are preserved, so the validity of the containing object never depends on which registry version a client ships (ADR-0155).

**Future critical-name rule (ADR-0157).** Every new critical extension registered after the W0 registry freeze uses an ID beginning with `!`. An OM-CBOR-v1 decoder rejects an unknown `!` ID even when its wire `critical` bit is false, so a hostile sender cannot create old-client/new-client validity disagreement merely by clearing that bit. IDs beginning with `!` can never be registered non-critical. `om.acs.fork_ack` and `om.acs.legacy_policy` are the only grandfathered critical IDs outside that class because both are already present in the W0 registry snapshot. Current registered entries include those two plus `om.voice-note.v1`, `om.location-card.v1` and `om.rich-list.v1` (ADR-0126, ADR-0141, ADR-0149, ADR-0150, ADR-0151, ADR-0157).

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

[`PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md`](PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md) resolves A-015.

Already-assigned registry identifiers are immutable and never reused for incompatible meaning. Published protocol generations/profile bundles snapshot the registry material needed to interpret them so runtime validity never depends on consulting a live OM Project registry server.

If stewardship diverges, successor registries/profiles use an explicit new lineage/identifier namespace. A vendor-private extension may remain private, but market adoption does not grant it canonical status. Graduation into a future baseline requires public independently implementable semantics, canonical encoding/criticality, conformance behavior and acceptable implementation rights.

## 7. Version evolution is not silent reinterpretation (ADR-0162)

A published `(extension_id, version)` registration is immutable. Criticality is immutable for an extension ID, and IDs
beginning with `!` are permanently critical. A new version under an existing non-critical ID is permitted only when old
clients can ignore it safely and the containing object remains valid under its base fallback. Changes to authority,
authentication, confidentiality, membership, downgrade behavior, owning object or fallback validity require a new extension
ID and/or CryptoProfile. Extension version numbers never act as security-generation or preference values.

The frozen W0 CDDL and registries cannot be expanded by editing a logical JSON schema. Future wire additions use the
reserved extension/profile mechanisms or a new protocol lineage as specified by ADR-0162.
