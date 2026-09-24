# v008-permissions: Room/Space authority model (TLA+)

Feasibility prototype for V-008. Not product code. Not for production.

Written by a Claude agent. The design package was authored by a ChatGPT design agent and Claude agents; ADR-0117
(proposed) decides whether an AI reviewer counts as independent. The model is written from the public prose in
`SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md` and `SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md` (sections
cited in the module header). It is not a translation of `verification/bounded/authority_models.py`: it uses a different
formalism (TLA+ checked by TLC) and a different author.

## What is modelled

[`ScopedPermissions.tla`](../../tla/ScopedPermissions.tla):

- **Grants.** Each grant has `caps`, a grantable scope `deleg` and a `parent`: the governance root, the grant that
  justified it, or Space-derived authority. A role manager may issue only `caps ∪ deleg ⊆` the ceiling of its basis.
- **Restrictions.** Restrictions subtract from effective caps. Nobody may lift their own restriction.
- **Governance.** A governance set plus a threshold. Approvals are bound to the base version, and a transition executes
  only on its own base. A second sealed successor of an already-superseded head freezes privileged transitions until
  the pre-fork governance resolves the fork.
- **Links and binding.** Links are bilateral (Space side plus Room side). A Room has at most one authority-parent
  binding, created by Room quorum, and only over an official link. When the Space withdraws its side, the binding
  ends.
- **Mirror of Space roles.** The room-local mirror of Space roles changes only when the Room accepts a sync
  transition, which is asynchronous. This models the revocation race window from "No live transitive permission
  magic".
- **Nested Spaces.** S1 is S2's own authority parent, so it can author S2's roles. This is the `A -> B -> Room` case.

### Properties

| Property | Kind | Meaning |
|---|---|---|
| `CoalitionClosed` | action | No ordinary action creates a capability or grantable scope that nobody held (the delegation ceiling). |
| `NoSelfEscalation` | action | No principal's own action enlarges its own effective or grantable authority. |
| `HighRiskRooted` | invariant | `add_bridge` is held only through a governance-rooted chain or a quorum-approved binding ceiling. |
| `QuorumBoundToState` | invariant | The approvals that counted came from the governance in force. |
| `NoTransitiveAuthority` | invariant | Only a Space the Room bound by its own quorum, acting through its *own* governance, ever changed Room authority. |
| `MirrorWithinCeiling`, `NoLaunderedAuthority` | invariant | Space-derived authority stays within the binding. No grant outlives the chain that justified it. |
| `RevocationDominates` | liveness | A revoked or unbound Space role is eventually removed from the Room (weak fairness on the Room's sync). |

## Results (TLC 1.7.4, `run.sh`, about 70 s)

| Config | Expect | Result |
|---|---|---|
| `ScopedPermissions.cfg` (roles, restrictions, 2-of-2 governance) | pass | 42,788 distinct states |
| `ScopedPermissions_spaces.cfg` (links, binding, mirror, revocation race, liveness) | pass | 456,291 distinct states |
| `ScopedPermissions_gov.cfg` (2-of-3 governance, stale approvals, forks) | pass | 11,154 distinct states |
| 6 × `mut_*` (one rule disabled each) | violation | each caught: no_ceiling, self_lift, stale_approvals, accept_subdelegated, no_cascade, unilateral_binding |
| 5 × `trap_*` (golden witnesses) | violation | fork freeze, high-risk by quorum, Space-delegated moderation, cascade revocation, governance transfer |

Every counterexample and witness trace becomes a regression vector in `verification/vectors/tla-ScopedPermissions_*.json`.
These are written by `tools/run_tlc.py`. That tool labels any non-`Trap` vector `residual-risk`. For the `mut_*`
configs the label means "fault-injected counterexample: the step a conforming implementation must reject".

Bounds: 3 principals, 2 Spaces, 4 capabilities, 5–6 environment steps and at most 3 grants. Governance execution and
Room sync are system steps that do not count toward the bound. Per-principal syncs are merged into one Room transition,
which reduces the state space.

## Design gaps found (proposed design updates)

1. **Grant revocation cascade is unspecified.** The docs never say whether grants issued by a principal survive
   when that principal's authority is revoked. `mut_no_cascade` shows the attack: a Space moderator whose Space role is
   revoked can, during the sync window, issue room-local grants that outlive the revocation ("authority laundering").
   *Proposal:* every grant records its authorization basis, and it stays valid only while that basis is valid
   (cascade). A deliberate "re-root" requires a governance transition.
2. **Sub-delegated Space authority is ambiguous.** `SPACES_ROOMS…` "Nested Spaces" forbids authority *merely because
   of nesting*. It is silent when S2 has explicitly delegated its role management to S1. `mut_accept_subdelegated`
   shows that S1 then controls the Room without the Room's review. *Proposal:* a `SpaceAuthorityBinding` names the
   authoring governance it accepts. Room mirrors reject Space-role updates authored under the Space's own delegation
   unless the binding explicitly opts in.
3. **Space-side unlink vs. binding.** The docs say "either side may revoke the link" but not whether a Space-side
   unlink ends the authority binding. If it does not, re-linking silently restores authority. *Proposal (modelled):* a
   Space-side unlink terminates the binding, and the Room must re-bind by quorum.
4. **Freeze vs. revocation.** The docs freeze "privileged transitions" on a fork. *Proposal (modelled):*
   authority-*reducing* transitions (mirror sync removing Space roles, binding drop) stay applicable while frozen, so
   a fork cannot be used to block revocation.
5. **Proposed tooling tweak (for the maintainers of `tools/`):** let a `models*.json` entry carry a `vector_kind`
   field, so fault-injection counterexamples are not labelled `residual-risk`.

Registry note: the new files need `python3 tools/om_registry.py sync` (I may not edit `machine/`).
