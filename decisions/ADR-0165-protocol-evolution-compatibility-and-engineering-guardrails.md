# ADR-0165 — Protocol evolution, compatibility classes and engineering-agent guardrails

- **Status:** accepted normative evolution/change-control supplement
- **Checkpoint:** 2026-09-29 Run 7
- **Engineering:** not authorized
- **Supersedes:** no existing wire/profile/authority decision

## Context

The longevity, extensibility and crypto-agility documents already establish immutable registry meaning, explicit
profile/fork evolution and no silent downgrade. Run 7 found that an implementation agent could still make four
reasonable-but-divergent choices: treat numeric version/profile IDs as security ordering, mutate a published schema in
place, negotiate different profiles from the same capability set, or classify an incompatible change as a harmless
patch. The repository also lacked one machine-owned change-class contract tying those decisions to the required ADR,
registry, vector, migration and continuity artifacts.

## Decision

Freeze a single protocol-evolution policy in [`docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md)
and its machine form [`machine/protocol_evolution_policy.json`](../machine/protocol_evolution_policy.json).

The policy has five mandatory properties:

1. **Published meaning is immutable.** A published registry/profile/extension ID, `(extension_id, version)` tuple,
   frozen W0 object shape, domain-separation label, or canonical field meaning is never edited in place. A
   behavior-changing correction requires a new immutable profile/extension/object lineage as appropriate.
2. **Version numbers are not security scores.** Crypto/profile selection compares the applicable
   `security_generation` floor first. Profile IDs are only a deterministic tie-break among candidates with equal
   generation; extension version numbers never authorize, upgrade, or downgrade security.
3. **Negotiation is deterministic.** The candidate set is the authenticated mutual intersection after local policy,
   lifecycle, conversation-type support and the effective floor are applied. The highest applicable security
   generation wins; deprecated candidates are avoided for new traffic when an equal-or-better non-deprecated
   candidate exists; equal-generation ties use the smallest canonical profile ID. If the candidate set is empty,
   the implementation must migrate/reinitialize, remain read/export-only, or fail safely according to the underlying
   protocol—never invent a fallback profile.
4. **Extensions evolve without criticality ambiguity.** A registry binds owning object, criticality and schema.
   Criticality is immutable for an extension ID; future critical semantics use the reserved `om.critical.` prefix (ADR-0157). A new extension
   version may reuse an ID only for backward-compatible, non-authority-changing semantics with a safe fallback;
   incompatible/security-critical changes use a new ID/profile. A `(extension_id, version)` schema is immutable once
   published.
5. **Every normative change has an explicit class and evidence path.** Engineering agents must classify a change as
   editorial, behavior-preserving clarification, additive non-critical extension, compatible new profile,
   security tightening/deprecation, incompatible protocol evolution, or fork/lineage transition before editing
   normative material. The class determines the required ADR, ledgers, conformance vectors, migration guidance and
   continuity snapshot. An agent may not downgrade a change class because implementation is easier.

## Change classes

The canonical classes and required artifacts are machine-owned by [`machine/protocol_evolution_policy.json`](../machine/protocol_evolution_policy.json).
In particular, no class permits in-place mutation of a frozen W0 wire object or published profile semantics.

## Engineering boundary

This ADR does **not** authorize any engineering scope. It adds guardrails that apply once a scope is independently
authorized. A prototype may challenge the policy with evidence, but may not silently redefine the protocol. A real
contradiction requires a new ADR and updated machine truth before the implementation adopts different semantics.

## Result

Future OM evolution has a deterministic compatibility/negotiation rule, explicit change classes, and machine-checkable
agent guardrails. Historical protocol generations remain independently interpretable and mirrorable, while genuinely
incompatible evolution is forced into a new profile/lineage instead of silently rewriting old meaning.
