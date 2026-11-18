# ADR-0206 — Resume relationship recovery after owner recovery

## Design sources read

Read in full: [AGENTS.md](../AGENTS.md), [AGENT_ARCHITECTURE_RULES.md](../docs/process/AGENT_ARCHITECTURE_RULES.md),
[RECOVERY_AND_BACKUP_POLICY.md](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md),
[ACCOUNT_CONTINUITY_AND_CONTROL.md](../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md),
[IMPLEMENTATION_READINESS_AND_TRANSACTION_CONTRACT.md](../docs/process/IMPLEMENTATION_READINESS_AND_TRANSACTION_CONTRACT.md),
ADR-0120, ADR-0173, ADR-0177, ADR-0184, ADR-0187, ADR-0199 through ADR-0203, and ADR-0205.
Read in part: ADR-0190 (verification, sender and receiver paths, local timing, and implementation rulings),
the freshness rules in [ASYNC_KEY_MATERIAL.md](../docs/architecture/crypto/ASYNC_KEY_MATERIAL.md), and the
roster and account-control rules in
[GROUP_COMMIT_AND_STATE_CONVERGENCE.md](../docs/architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md).
The delegated-rulings follow-up also read ADR-0173 rule 4, ADR-0176, ADR-0184,
[GROUP_COMMIT_AND_STATE_CONVERGENCE.md](../docs/architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md),
and the group state and binding layouts in [`group.cddl`](../spec/cddl/group.cddl). The remaining mapped Core, client
and UI sources were not read in full. Their design gate still applies to implementation.

- **Status:** Accepted by delegation, 2026-11-18, including the Q-REL-1 and Q-REL-2 rulings below.
- **Scope:** D157 in [the implementation ledger](../docs/reports/2026-10-28-relay-status.md). Existing wire formats,
  admission, authorization, retry and security rules apply. This ADR amends no audited text.

## Context

The owner can finish recovery while a contact is still verifying the same request locally. The encrypted
RecoveryRecord carries 1:1 relationship hints. The owner-held checkpoint anchors the owner's account chain.
A contact's accepted notice establishes the new device's eligibility at that contact. Establishing a fresh session
also needs the recovered owner's authenticated view of the contact and private delivery state for both ends.
Group membership and old message history each have their own recovery rules.

## Decision

1. **Keep relationship work tied to the completed owner recovery.** Stage bounded candidate work, if available,
   in the same local transaction that adopts the verified successor and clears the attempt. Bind it to the profile,
   adopted successor digest, accepted record generation and the snapshot's conversation and peer identifiers.
   Deduplicate that work across restart and repeated completion. Any surviving newer floor or conflicting
   observation supersedes the affected candidate. A split snapshot is usable only after opening its independent
   layer. Candidate data stays in the encrypted profile and never enters the live relationship table.
2. **Reconnect by explicit re-introduction (Q-REL-1).** The recovered owner chooses “Reconnect” for one
   contact or for all listed contacts. The snapshot supplies the advisory list. The ordinary contact introduction
   or invitation flow in ADR-0190 §3.4 supplies the authenticated contact anchor and private return path. The
   contact verifies the recovery notice against its own account-chain evidence. A notice or relay acknowledgement
   leaves the chat pending. A chat is ready when the normal handshake establishes a fresh usable session.
3. **Re-link the thread by AccountID.** Use the authenticated AccountID for local presentation once the new
   session is usable. Old messages appear through the separately chosen ADR-0184 backup-file import and stay
   read-only. The existing verifier preserves any surviving relationship and prekey floors, verifies current
   credentials and routes, and establishes fresh keys through the durable outbox. Timer credit, ratchets, replay
   ledgers and history never transfer into the new session.
4. **Ask a current Room member to re-add the device (Q-REL-2).** The owner chooses “Ask to rejoin” over an
   existing Room route or contact route. The Room shows a pending state until a member of the canonical chain
   performs the roster-neutral re-add under ADR-0173 rule 4. The ruling calls for the recovered device to appear
   in the canonical roster through its authenticated account chain. Room policy, revoked-leaf removal and the
   journaled Commit rules still apply. The recovered device cannot admit itself.

## Room implementation boundary

ADR-0173 rule 4 re-adds devices of a losing branch that are already in the canonical device roster. Owner
recovery creates a fresh DeviceID that never belonged to that branch. The audited group convergence rules
require validating the account chain and Room policy for a new leaf. ADR-0176 defines a `device.add` intent
carrying the validated account-head hash. The recovery binding has no intent hash and leaves the state hash
unchanged. The ruling does not specify the authorized transition that makes the fresh DeviceID roster-eligible
before rule 4 can run.

ADR-0199 §7 supplies neither a Room locator nor routes for group-only members. With no surviving Room route,
the owner needs authenticated Room information from a current member before sending the request. Which
existing flow supplies that information and which authorized transition enrolls the fresh DeviceID are
implementation blockers for Room rejoin. Ordinary Room invitations remain available for account admission
under current policy.

## Where did authority go?

The owner's verified account successor, each contact's own relationship verifier and the Room's canonical
policy retain their existing authority. Snapshot data is advisory. The reference client renders Core outcomes.

## Verification

The reconnect integration test must recover an owner from a real record and show the already-connected peer
both verifying recovery and delivering a message to the restored live conversation. Run it across restart and
notice replay, with stale and equivocated snapshots, and with an offline peer. Keep the existing read-only history
test. The Room test must join a fresh device through the authorized transition and prove that a revoked leaf,
removed member, or off-roster external Commit gets no future epoch secrets. A browser test must drive the rebuilt
WASM modules through the real host and show the difference between progress and a chat ready to use.
