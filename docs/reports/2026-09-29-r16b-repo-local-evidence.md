# R16B — remaining repo-local executable/spec/vector evidence

- **Checkpoint:** 2026-09-29 08:50 PHT
- **Input:** R16A gate-triage architecture checkpoint
- **Scope:** V-004, V-008, V-011, V-013, V-014, V-020 and V-024 only
- **Engineering authorization:** unchanged; **not authorized**

## Result

R16B closes only criteria whose semantics were already fixed by accepted architecture. It does not use verification work to invent unresolved protocol behavior, self-approve independent-review criteria, or substitute simulations for browser/device/human evidence.

| Gate | R16A repo-local progress | R16B progress | What R16B added | Still open locally |
|---|---:|---:|---|---|
| V-004 group control | 0/7 | **3/7** | 531 bounded cases covering reducing-transition liveness, account-control send/add rules and immutable-incarnation stale reductions | formal crash/partition/equivocation; target-runtime MLS ordering; disconnected same-epoch partition convergence; ADR-0156 wire freeze |
| V-008 Space/Room authority | 0/6 | **5/6** | credits the existing independent-tool TLC model after ADR-0122 fixed the modeled semantics; adds 24,637 bounded incarnation/ceiling/delegation/invite cases | ADR-0156 Space/Topic/Publication wire freeze |
| V-011 service authority | 0/7 | **6/7** | credits existing TLC host-migration/alias evidence and capability-broker injection corpus; adds 68 bounded membership/representation/removal/incarnation cases | ADR-0156 ServicePrincipal/RepresentationGrant wire freeze |
| V-013 bridges | 2/4 | **2/4** | no unsafe credit added | independent reviewed adapter contract; ADR-0156 bridge wire freeze |
| V-014 archive | 2/3 | **2/3** | no unsafe credit added | frozen `om-archive-v1` specification/vector profile after the candidate encryption profile receives the required review |
| V-020 provider portability | 7/9 | **9/9** | blocked-vendor + blocked-suffix survival measurements; 724 sponsor-revocation/non-sponsor-route cases | repo-local criteria complete; required independent review remains |
| V-024 LAN delivery | 0/4 | **3/4** | frozen profile constants; 9 positive + 8 negative vectors with independent Python/Node agreement; passive-observer and offline-first-contact mutation models | real-browser WebRTC DataChannel feasibility or explicit unsupported capability result |

Across the whole architecture ledger, **128/173 pre-engineering criterion rows now carry repo-local evidence**. This number is evidence progress only; gate passage still requires each gate's listed independence and any remaining environment/human criteria.

## R16B evidence

### V-004

[`verification/bounded/v004_group_control_r16b.py`](../../verification/bounded/v004_group_control_r16b.py) exercises 531 cases. The pass is intentionally restricted to already-settled rules:

- reducing transitions remain live under coordinator censorship/failure and deterministic succession;
- rebased non-reducing intent is revalidated at the new parent;
- learned device revocation blocks sending until accepted removal, CONTROL_CONFLICT/SAFETY_FREEZE blocks adds, and device-add is deduplicated;
- a delayed reduction targets an immutable incarnation and cannot remove a later re-add.

It does **not** claim the unresolved formal partition/equivocation or MLS-runtime criteria.

### V-008

The existing [`ScopedPermissions.tla`](../../verification/tla/ScopedPermissions.tla) evidence already checks Room/Space grants/restrictions, quorum, bilateral links, ceilings, cascade/revocation and transitive-authority traps with independent TLC. ADR-0122 subsequently made the cascade and subdelegation semantics normative. R16B therefore credits that criterion and adds [`v008_authority_r16b.py`](../../verification/bounded/v008_authority_r16b.py), which passes 24,637 bounded cases for immutable grant/decision incarnations, role ceilings, delegated-intent/stale-head rules, reducing liveness, invite lapse and CryptoProfile immutability.

Wire encoding remains deliberately separate.

### V-011

The existing [`ServiceAuthority.tla`](../../verification/tla/ServiceAuthority.tla) and capability-broker corpus already cover host migration/revocation, alias replacement and prompt/tool capability containment. R16B adds 68 bounded cases for the two service positions, RepresentationGrant authority/quarantine/notice rules, endpoint removal liveness/rebase and immutable installation incarnations. Only the later-wave wire freeze remains repo-local.

### V-020

[`v020_provider_resilience_r16b.py`](../../verification/bounded/v020_provider_resilience_r16b.py) completes both missing local criteria:

- blocked-suffix: established-relationship success fraction **0.5503**, first-contact success fraction **0.7416**;
- blocked-vendor: established-relationship success fraction **0.4244**, first-contact success fraction **0.6499**;
- sponsor revocation / non-sponsor route: **724** bounded cases pass.

These are bounded model outputs, not production availability promises.

### V-024

R16B turns ADR-0158's frozen LAN design into executable wire/model evidence:

- [`spec/protocol/lan-delivery-profile.json`](../../spec/protocol/lan-delivery-profile.json) records the frozen constants and required domain labels;
- independent Python and Node schema-specific codecs agree on **9 positive and 8 negative** canonical vectors;
- passive-observer modeling enforces K=8 fixed cardinality, absence of clear sender/DR header and rotation of connection/route/delivery identifiers across epochs/link contexts;
- offline first contact requires IntroGate + MessageRequest + reciprocal relationship binding before async-key or device eligibility authority exists.

Real-browser WebRTC/DataChannel feasibility remains unclaimed.

## Deliberate non-closures

### V-013

The bridge semantics are mature, but the requested wire freeze would allocate later-wave object/field/domain identifiers and define canonical CDDL for BridgePrincipal/BridgeControlState/foreign provenance. Those bytes are an interoperable protocol design decision. R16B does not invent them under a verification label. The separate independent-review criterion also remains open.

### V-014

The archive hostile corpus and reference reader/writer remain evidenced. The archive's encryption profile is still explicitly a candidate requiring review. Freezing it here would improperly turn an unreviewed cryptographic choice into a normative format.

## Meaning for the next run

R16B substantially reduces uncredited repo-local work. The highest-value remaining local work is now concentrated in later-wave wire freezes and a small number of true runtime/formal criteria. A next bounded run should choose **one coherent wire domain** (rather than freezing all W1–W4 objects at once), produce CDDL + registry allocations + two independent codecs/vectors, and leave human/device/reviewer evidence separate.
