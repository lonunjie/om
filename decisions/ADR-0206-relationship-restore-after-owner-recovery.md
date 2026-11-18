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
The remaining mapped Core, client and UI sources were not read in full. Their design gate still applies to implementation.

- **Status:** Accepted by delegation, 2026-11-18, for local sequencing and presentation only.
- **Scope:** D157 in [the implementation ledger](../docs/reports/2026-10-28-relay-status.md). Existing wire formats,
  admission, authorization, retry and security rules apply. Automatic reconnect requires separate protocol decisions.

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
2. **Show progress from Core's actual work.** The reference client may say “Getting your chats back” while Core
   has real reconnect work in progress. A delivered notice, an accepted owner successor and a relay acknowledgement
   each leave the chat pending. A chat is ready when the normal session path has established a fresh usable session
   on the existing relationship. When reconciliation cannot proceed, offer the ordinary re-introduction path
   from ADR-0190 §3.4. Keep the backup-file action separate. ADR-0184 imported conversations stay read-only,
   even if a new live conversation exists with the same person.
3. **Resume lazily.** Use ADR-0120 P1 for session repair. The ADR-0190 notice is still allowed on its existing
   sealed route and schedule. Its receipt does not enable key fetch, messaging or group admission during the
   contact's wait. Before a fresh PQXDH session, the existing relationship verifier must establish the current
   eligible credentials, preserve any surviving relationship and prekey floors, and validate the private route
   and delivery key. Start with fresh keys and publish through the existing durable outbox. Carry no peer-local
   timer credit, ratchet, replay ledger or message history into session setup.
4. **Use current Room admission for a fresh device.** ADR-0173 rule 4 limits the roster-neutral external-commit and
   re-add path to devices already eligible in the canonical Room roster. A recovered account has a fresh
   DeviceID. A current Room member must verify its account evidence and current Room policy, remove revoked
   leaves as required by the audited account-control rules, and add the new device through an authorized,
   journaled Room transition. The stable account alone cannot authorize a fresh MLS leaf. Group-only relationships
   are outside the 1:1 snapshot.

## Open protocol questions

- Which independently authenticated evidence binds a recorded contact and its established security floor to the
  recovered owner after total loss? Which private carrier supplies the current peer state, credentials, key
  service and return route before either side releases messaging secrets? RecoveryRecord AEAD, a Home's key
  bundle response and an AccountID string are insufficient. Older snapshots can omit even the advisory peer
  head and fetch capability.
- Which current, authenticated Room member supplies the Room locator and initiates the authorized fresh-device
  Add, including when no 1:1 relationship can reach the Room? ADR-0199 §7 defines no Room entry, and ADR-0173's
  roster-neutral recovery Commit cannot provide admission.

Until these paths are specified, the existing contact notice, ordinary re-introduction and authorized Room
invitation remain available. History still comes from the separately chosen backup file. No new registry entry,
wire field, Home endpoint or authority rule is defined here.

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
