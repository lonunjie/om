# v016-room-moderation: Room timeout/remove/ban race and evidence model (TLA+)

Feasibility prototype for V-016 (Room half). Not product code. Not for production.

Written by a Claude agent. The design package was authored by a ChatGPT design agent and Claude agents; ADR-0117
(proposed) decides whether an AI reviewer counts as independent. The model is written from [`DECENTRALIZED_ABUSE_MODERATION.md`](../../../docs/architecture/safety/DECENTRALIZED_ABUSE_MODERATION.md)
§§6–11 and §24, [`EVIDENCE_AND_EXTERNAL_REPORTING.md`](../../../docs/architecture/safety/EVIDENCE_AND_EXTERNAL_REPORTING.md) and [`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../../../docs/architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md) §4. It is not
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
| [`RoomModeration.cfg`](../../tla/RoomModeration.cfg): ban/unban, demotion while acting, admit vs ban | pass, 1,909 states |
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

## Historical gaps and accepted resolutions

This prototype originally exposed five design gaps. They are no longer open design work:

1. **Moderator vs. governance** — resolved by ADR-0122: moderation against a governance principal is a constitutional transition requiring governance authority/quorum.
2. **Past-epoch window / receiver divergence** — resolved by ADR-0121: authorization is against the sending epoch; `past_epoch_window` defaults to 2 and is capped at 8; late-arrival divergence is an explicit bounded residual risk.
3. **Timeout expiry** — resolved by ADR-0122: timeout ends only through an accepted, epoch-bound `untimeout` transition.
4. **Evidence ACL across demotion** — resolved by ADR-0123: evidence is encrypted to the moderator set at report time; demotion removes access only to future evidence.
5. **Appeal hold bound** — resolved by ADR-0123: appeal retention is capped by `hold_max`, at most 180 days after original retention expiry.

The current machine ledger records all **9/9 repo-local V-016 pre-engineering criteria as met**. V-016 remains pre-engineering-open only because ADR-0117's different-vendor independent-review record is still missing. The reference-provider adversarial load criterion is pre-release work and is intentionally not required for the present `core_kernel` review.
