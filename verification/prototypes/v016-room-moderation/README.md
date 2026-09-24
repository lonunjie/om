# v016-room-moderation: Room timeout/remove/ban race and evidence model (TLA+)

Feasibility prototype for V-016 (Room half). Not product code. Not for production.

Written by a Claude agent. The design package was authored by a ChatGPT design agent and Claude agents; ADR-0117
(proposed) decides whether an AI reviewer counts as independent. The model is written from `DECENTRALIZED_ABUSE_MODERATION.md`
§§6–11 and §24, `EVIDENCE_AND_EXTERNAL_REPORTING.md` and `GROUP_COMMIT_AND_STATE_CONVERGENCE.md` §4. It is not
derived from `verification/bounded/`.

## Model ([`RoomModeration.tla`](../../tla/RoomModeration.tla))

- **Intents.** A client, possibly hostile, can sign a moderation intent at any time: timeout, untimeout, remove, ban,
  unban, admit or demote. The Room's commit order applies each intent against the state immediately before it, so a
  demoted moderator's in-flight action is rejected.
- **Scope.** Authority belongs to Room moderators and governance. Space moderators have it only in Rooms whose binding
  delegates it. Nobody may act on themselves.
- **Epochs and write access.** Remove, ban, timeout and admit advance the MLS epoch. `writersAt[e]` and
  `membersAt[e]` snapshot who may post and who holds the epoch secret. A hostile sender can send at any epoch whose
  secret it ever held. Two receivers lag behind the canonical epoch. They accept a message only if the sender could
  post in *that message's* epoch and the epoch is within a past-epoch `Window`.
- **Evidence.** A report may select an accepted message or fabricate one, and message content is never marked
  verifiable. The package is encrypted to the moderators in force at report time. Retention is bounded, and an appeal
  hold extends it by at most `HoldMax`.

## Results (TLC 1.7.4, `run.sh`, about 30 s)

| Config | Result |
|---|---|
| `RoomModeration.cfg`: ban/unban, demotion while acting, admit vs ban | pass, 1,909 states |
| `_scope`: other room, unbound Space, moderator vs governance, self-lift | pass, 811 states |
| `_write`: epoch-bound writes, hostile senders, lagging receivers | pass, 1,582,486 states |
| `_evidence`: evidence ACL across a demotion; retention and appeal | pass, 7,035 states |
| 7 × `mut_*` | each caught: auth_at_creation, admit_at_creation, global_mod, mod_vs_gov, no_rekey, stale_evidence_acl, no_expiry |
| `residual_divergence` | expected violation of `ReceiversAgree` (see gap 2) |
| 5 × `trap_*` | golden vectors: demoted moderator rejected, unban after ban, banned admit rejected, late old-epoch write, appeal hold |

Invariants that hold: `ScopedAuthority`, `WriteBoundedByEpoch` (no removed, timed-out or banned member has a message
accepted at or after the excluding epoch), `NoKeyAfterRemoval`, `StaleWriteWindow`, `BanExcludes`,
`GovernanceProtected`, `EvidenceScoped` and `RetentionBound`. The vectors are in
`verification/vectors/tla-RoomModeration_*.json`.

## Design gaps found (proposed design updates)

1. **Moderator vs. governance.** §6 and §10 do not say whether an ordinary `remove_member`/`ban_member` holder may
   remove, ban or time out a *governance* principal. `mut_mod_vs_gov` shows that one malicious moderator can then
   strip the governance set and orphan the Room. *Proposal (modelled):* moderation actions against a governance
   principal are constitutional transitions that need governance authority or quorum.
2. **The past-epoch window is unspecified and makes acceptance receiver-dependent.** MLS receivers keep old epoch
   secrets for late messages. A removed or timed-out member (a hostile client) can keep sending in the last epoch it
   was allowed in. Receivers accept those messages while that epoch is inside their window
   (`trap_late_write`), and two receivers at different epochs disagree about the same message
   (`residual_divergence`). *Proposal:* specify a small past-epoch window in the group profile. Authorize a message
   against the state of its own epoch, never the receiver's current state. Document the bounded divergence as a
   residual risk, or have the CommitCoordinator sequence application messages if exact convergence is required. This
   touches V-009.
3. **Timeout expiry.** §6 says expiry is "convenience/policy time, not constitutional authority". The model makes
   expiry an explicit `untimeout` control transition that is bound to an epoch. *Proposal:* say normatively that a
   timeout ends only through an accepted transition, whether manual or automatic but committed, so every receiver
   evaluates the same state.
4. **Evidence ACL across demotion.** The docs do not say to whom private report evidence is encrypted. *Proposal
   (modelled):* encrypt each EvidencePackage to the moderator set in force when it is reported. A demotion takes away
   access to *future* evidence, but it cannot claw back evidence already disclosed.
5. **Appeal hold bound.** Retention during an appeal needs an explicit upper bound (`HoldMax`), so that an endless
   appeal cannot hold evidence indefinitely.

Registry note: the new files need `python3 tools/om_registry.py sync` (this agent may not edit `machine/`).
