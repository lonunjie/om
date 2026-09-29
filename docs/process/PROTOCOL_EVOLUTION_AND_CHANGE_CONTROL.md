# Protocol Evolution and Change Control

**Checkpoint:** 2026-09-29 Run 7 / ADR-0165  
**Status:** normative pre-engineering change-control contract  
**Engineering:** not authorized

This document is the implementation-facing companion to [`PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md`](../constitution/PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md),
[`PROTOCOL_EXTENSIBILITY.md`](../constitution/PROTOCOL_EXTENSIBILITY.md), and [`CRYPTOGRAPHY_AND_AGILITY.md`](../architecture/crypto/CRYPTOGRAPHY_AND_AGILITY.md). It does not create a new wire format.

## 1. Published artifacts are immutable

Within a published protocol generation/profile or continuity bundle, these meanings are immutable:

- registry IDs and their assigned meaning;
- CryptoProfile IDs and profile construction meaning;
- `(extension_id, version)` schema/criticality/owner binding;
- frozen W0 CDDL object shapes and field meanings;
- canonical field/domain-separation labels;
- historical event/control type meanings.

A documentation correction that is provably behavior-preserving may be recorded as a clarification/erratum. If the
correction can change an accepted/rejected byte, state transition, security property or interoperability outcome,
it is **not** a clarification: create a new immutable profile/extension/object lineage as appropriate.

Never solve a compatibility problem by editing the old definition until the new implementation passes.

## 2. Change classes

| Class | Meaning | May mutate published semantics? | Minimum artifacts |
|---|---|---:|---|
| `editorial` | Non-normative wording/layout only | No | changelog |
| `clarification` | Resolves genuine underspecification without changing any conforming outcome | No | ADR/erratum + bounded equivalence check |
| `additive_noncritical_extension` | New optional presentation/data semantics with safe old-client behavior | No | ADR + registry + schema + fallback + vectors + trace/threat updates |
| `compatible_new_profile` | New immutable profile with explicit migration and no weaker floor | No | ADR + profile registry entry + negotiation/migration vectors + trace/threat updates |
| `security_tightening` | Advisory/policy removes unsafe use or advances a floor | No | ADR/advisory + replacement/floor rule + negative downgrade vectors |
| `incompatible_protocol_evolution` | Changes authenticated bytes, authority, state semantics or mandatory interpretation | No | new profile/object/extension lineage + ADR + migration/reinit + independent conformance plan + continuity snapshot |
| `fork_or_lineage_transition` | Deliberate stewardship/protocol fork | No | new lineage identifier/root + continuity bundle + explicit historical-meaning statement + migration/escape guidance |

A change must be assigned the **strongest applicable class**. Convenience never justifies choosing a weaker class.

## 3. Crypto/profile negotiation

For a conversation type `T`, selection is deterministic:

1. Start with the authenticated/validated mutual intersection of profiles supported by both peers for `T`.
2. Remove profiles forbidden by local policy, `test_only`/experimental production rules, unsupported conversation type,
   or the applicable account/relationship/Room security floor.
3. If no candidate remains, do not invent a fallback. Follow the underlying protocol's migration/reinitialization,
   read/export-only, or safe-failure behavior.
4. Among candidates, choose the highest `security_generation[T]`.
5. For new traffic, if an equal-or-higher-generation non-deprecated candidate exists, remove deprecated candidates.
   A deprecated profile may remain usable for bounded continuity only when no non-deprecated candidate at the required
   generation is available and the underlying profile explicitly permits continuity.
6. If multiple candidates remain at the same generation, choose the smallest canonical numeric `profile_id`.
   This final step is a deterministic tie-break **only**; profile IDs have no security ordering.
7. A SecurityAdvisory may forbid a profile and cause a move to an equal-or-higher non-forbidden generation. It may
   never lower an established floor.

A peer may reject a profile because of its local policy. In that case the common candidate set is recomputed; if the
sets no longer intersect, migration/reinitialization or safe failure occurs. Neither peer guesses the other's preference.

## 4. Extension evolution

A registry entry binds at least:

`extension_id + version + owning_object + criticality + canonical_payload_schema + fallback/semantic rules`.

Rules:

- A published `(extension_id, version)` tuple is immutable.
- Criticality is immutable across versions of an extension ID.
- An ID under the reserved `om.critical.` prefix (ADR-0157) is permanently critical and cannot be registered non-critical.
- A future critical semantic that cannot safely inherit the old ID uses a new `om.critical.` ID.
- A new version under an existing non-critical ID is allowed only when an older client can ignore the unknown version
  and the containing authenticated object remains valid with its W0/base fallback.
- If the new version changes authority, authentication, confidentiality, membership, downgrade behavior, required
  interpretation, owning object or fallback validity, it requires a new extension ID and/or CryptoProfile rather than
  an in-place version bump.
- Extension `version` is never a security-generation or preference value.
- Logical JSON schema changes cannot create new wire fields in a frozen W0 object.

## 5. Profile migration

A compatible profile migration must preserve the stable account/conversation identity and authenticate the transition.
Normal migration is:

`discover -> mutually support -> validate floor -> establish new profile state -> move new traffic -> bounded drain -> retire old state`

The old profile may not become the active sender profile again after its floor has advanced, except where the existing
SecurityAdvisory/deprecation rules explicitly permit bounded continuity. If identity continuity cannot be proven, the
migration is a re-establishment, not a silent upgrade.

Group profile changes use the existing authenticated ReInit/MLS + GroupControlState path; 1:1 changes use the existing
relationship/session profile transition. No new generic “upgrade token” is introduced by this policy.

## 6. Fork/lineage semantics

A protocol fork preserves the historical interpretation of old objects. New semantics receive a new lineage/profile or
extension identifier and a continuity statement. A fork does not retroactively invalidate old bytes or make a dominant
client's interpretation canonical merely because it has more users.

Implementation forks that preserve protocol semantics need no protocol ID change. Stewardship/project forks may retain
existing protocol IDs while creating a new release/distribution trust root. Incompatible protocol forks require explicit
new protocol lineage/profile identifiers.

## 7. Engineering change-control algorithm

Before an engineering agent changes a normative artifact:

1. identify the affected wire/state/security surface;
2. classify the change using §2;
3. locate the current ADR/profile/extension lineage;
4. update requirements, threats, invariant traceability and conformance obligations;
5. update the relevant schema/registry/vector/continuity snapshot if the class requires it;
6. run the protocol-evolution validator and normal repository checks;
7. if the change is behavior-changing, stop and obtain the ADR/new-lineage decision before implementing it;
8. preserve the previous published semantics for all old IDs/profiles;
9. record the migration/fallback story before shipping the implementation;
10. never mark an external/formal/interoperability gate passed merely because the local harness passes.

## 8. Forbidden shortcuts

An implementation agent MUST NOT:

- choose “highest version” as a security rule;
- choose “highest profile ID” as a security rule;
- mutate an existing CryptoProfile to point at new primitives;
- change an extension's criticality in place;
- add a mandatory wire field to a frozen W0 object through a logical JSON schema;
- treat `preferred`, `deprecated`, or `experimental` status as authority by itself;
- silently fall back below an established security floor;
- use a project website, live registry, DNS result or dominant client as runtime protocol authority;
- call a new wire behavior “backward compatible” merely because old clients can parse the outer bytes;
- claim a local bounded harness proves formal, independent or real-network compatibility;
- erase old protocol history needed to interpret existing canonical objects.

## 9. Machine contract

The machine-readable source for this policy is [`machine/protocol_evolution_policy.json`](../../machine/protocol_evolution_policy.json).
The bounded sanity harness is [`verification/bounded/protocol_evolution_adr0165.py`](../../verification/bounded/protocol_evolution_adr0165.py).
The harness checks policy mechanics only; V-002/V-003/V-004/V-005/V-017 and other gates remain the authority for
formal, independent, runtime, interoperability and continuity evidence.
