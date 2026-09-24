# ADR-0122 — Authority reductions, grant cascade and governance protection

- **Status:** Accepted
- **Date:** 2026-09-24
- **Decided by:** operator (delegated 2026-09-24: "draft the design gap rulings ... check it and accept it if good")
- **Source:** V-008 ([`v008-permissions`](../verification/prototypes/v008-permissions/README.md)), V-011 gaps 1–4
  ([`v011-services`](../verification/prototypes/v011-services/README.md)), V-016 gaps 1 and 3
  ([`v016-room-moderation`](../verification/prototypes/v016-room-moderation/README.md)). **Amends:**
  [`SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md`](../docs/architecture/groups/SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md), [`SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`](../docs/architecture/groups/SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md),
  [`SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md`](../docs/architecture/ecosystem/SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md), [`DECENTRALIZED_ABUSE_MODERATION.md`](../docs/architecture/safety/DECENTRALIZED_ABUSE_MODERATION.md).

## Context

Four models found the same class of problem: the fork freeze and the delegation rules protect against gaining
authority, but they can block *losing* it (a revocation dropped by a freeze), or let authority survive the loss of its
basis (a grant outliving its grantor's revocation).

## Decision

1. **Reductions are monotone.** In Space, Room and ServiceControlState control chains, an authority-*reducing*
   transition is never a fork. Such transitions include revoking a grant, role or endpoint, dropping a binding,
   unlinking, and a mirror sync that removes Space roles. A reducing transition must be authorized at its parent
   state. It then rebases onto any later head, commutes with other reductions, and stays applicable while control is
   frozen. Only transitions that add or change authority freeze on a fork.
   **AccountControlState is excluded.** Its fork rules stand, because a stolen device must not be able to revoke the
   owner's devices through this rule.
2. **Grant cascade.** Every grant records its authorization basis: the grantor's role or grant. A grant is valid only
   while its basis is valid, evaluated at the sending epoch of the event that uses it. Keeping a grant alive after its
   basis is gone ("re-rooting") needs a governance transition.
3. **Sub-delegation.** A `SpaceAuthorityBinding` names the Space governance whose authorship it accepts. A Room mirror
   rejects Space-role updates authored under that Space's delegation to another Space, unless the binding explicitly
   opts in.
4. **Unlink ends the binding.** A Space-side unlink terminates the authority binding (a reduction, rule 1). Re-linking
   needs a new binding approved by the Room's quorum, so it never silently restores authority.
5. **Service controllers.** The service itself, its endpoints and its hosts are ineligible controller principals. This
   extends no-self-bootstrap from approval to appointment.
6. **Anti-rollback.** Recipients keep the highest validated head of every hash-chained control state, services
   included, and reject older states.
7. **Host migration.** `migrate_endpoint` is one atomic transition that authorizes the new endpoint and revokes the
   old one. The split form (authorize, then revoke) also stays safe under rule 1.
8. **Stale recipients.** A recipient that has not synced may still accept a revoked endpoint or a stale mirror role.
   This is documented bounded staleness (residual risk). Clients sync control state before acting on a privileged
   request whenever they can.
9. **Moderation against governance.** Removing, banning, timing out or restricting a governance principal is a
   constitutional transition. It needs governance authority or quorum, not an ordinary moderation capability.
10. **Timeout expiry is committed.** A timeout ends only through an accepted, epoch-bound `untimeout` transition.
    Moderators' clients commit it automatically once the expiry time passes. The subject cannot lift its own timeout,
    and until the transition is committed the timeout stays in force.

## Consequences

The V-008 gaps, V-011 gaps 1–4 and V-016 gaps 1 and 3 are closed as design work. These gates now need only their
non-author reviews (ADR-0117).
