# Account Lifecycle End: Retirement, Deletion, Succession and Legacy

- **Status:** Canonical architecture
- **Checkpoint:** 2026-09-24 02:29 PHT
- **Decision:** ADR-0105
- **Gate:** V-002 (extended)
- **Schema:** [`spec/protocol/account-retirement.logical.schema.json`](../../../spec/protocol/account-retirement.logical.schema.json)

## 1. Problem

Central messengers delete an account on their server, and contacts see "deleted account". OM has no such server. Before this checkpoint an OM account could only be abandoned. Contacts kept encrypting to it, relays kept spooling for it, and a lost-but-not-retired account could not be told apart from a merely offline one. Death or incapacity had no path at all.

## 2. AccountRetirement (constitutional transition)

`AccountRetirement` is a terminal AccountControlState transition:

```text
AccountControlState(generation n)
  -> AccountRetirement {
       account_id
       previous_state_hash
       reason_class: user_closed | moved | legacy
       successor_account_id?      // only with reason_class = moved (§5)
       farewell_note?             // optional short text, E2EE to contacts
       effective_at               // informational clock class; not authority
       authorization_signatures[] // parent-state majority, possibly with one recovery vote (rule 1)
     }
```

Rules:

1. It is authorized exactly like any constitutional change ([`ACCOUNT_CONTINUITY_AND_CONTROL.md`](ACCOUNT_CONTINUITY_AND_CONTROL.md) §4.1): by the **parent**
   state's majority, where a RecoveryPolicy counts as one vote. With `authorization_kind = recovery_policy` it is
   immediate only if at least one current, non-quarantined controller co-signs. Otherwise it takes effect only after
   the RecoveryPolicy activation delay (at least 24 h) with no RecoveryVeto from any live controller, exactly like a
   RecoveryRequest ([`RECOVERY_AND_BACKUP_POLICY.md`](RECOVERY_AND_BACKUP_POLICY.md) §10.3). A single quarantined or ordinary messaging device cannot retire
   an account. `authorization_kind = legacy_policy` is not valid in W0 (§4).
2. It is **terminal** once effective. No later state descends from a retired state. A competing descendant is a fork
   and follows the existing safety-freeze/fork rules. A retirement still inside its recovery delay is pending, not
   effective, and a RecoveryVeto cancels it.
3. Clients render the contact as **Closed** (or "Moved to …" with a successor) only after they receive and verify the
   signed `AccountRetirement` object itself, through a relationship channel or a fetch. An AccountControlHead cannot
   show a retirement: a gossiped head that points at an unknown newer state only triggers a fetch of that state.
   After verifying, clients stop sending new messages, stop encrypting to the account's devices, and remove its
   delivery capabilities from their own RelaySets.
4. The user's own Homes, on receiving it from a control device, purge queued envelopes and revoke the account's capabilities. Provider compliance is best-effort and cannot be proven; clients state that.
5. Retirement never deletes other people's copies of past messages. The UI says so ("People you talked to keep their copies").

## 3. Deletion

"Delete my account" = AccountRetirement(`user_closed`), then local wipe of every profile store on each device, then Home purge. It is one user action with a clear confirmation. It needs no provider or project involvement.

A user who only wants to leave one device uses device removal, not retirement.

## 4. LegacyPolicy (optional, off by default)

For death or permanent incapacity, a user may configure a **LegacyPolicy** naming legacy contacts (guardians) and a threshold. It is part of the RecoveryPolicy family and needs explicit user action (R-029).

**Deferred to W1 (ADR-0127).** No frozen W0 object can yet commit a LegacyPolicy, so W0 verifiers reject every
`AccountRetirement` with `authorization_kind = legacy_policy`. Before LegacyPolicy ships, design-queue item A-030
must define: where the commitment lives (a key-23 AccountControlState extension, so W0 objects carry it unchanged),
including guardian keys, threshold and a policy-fixed **minimum contest window**; that verifiers reject a legacy
retirement whose `contest_window_seconds` is below the committed minimum; and the pending-state wire behavior below.

A LegacyPolicy can authorize **only**:

- AccountRetirement with `reason_class = legacy`;
- an optional memorial note (E2EE to contacts);
- an optional successor pointer that the user pre-designated when configuring the policy.

A LegacyPolicy can **never**:

- decrypt or export history;
- send as the account;
- enroll devices or change control;
- designate a successor that the user did not pre-designate.

History inheritance, if a user wants it, is a separate explicit export or grant the user makes while alive. It is not a protocol power.

Abuse guards (normative now, enforced when legacy ships):

- **Owner inactivity first.** A legacy claim can be made only after the owner's devices have been inactive for the
  committed waiting period (the TLA model's `idle >= LegacyWait`).
- **`LegacyRetirementPending` is non-terminal.** A legacy retirement first enters this pending state and starts a
  **contest window** that live controllers can see. Any valid signature by a live controller during the window
  cancels it. It becomes terminal only at window end. During the window contacts keep the account open, keep
  delivering and may show "a legacy request is pending"; they render **Closed** only at window end.
- This deliberately does not follow the quarantine rule: legacy retirement is irreversible, so it must be
  cancellable by the living owner. Time never grants authority by itself here either, because the threshold
  signatures are still required.

## 5. Moved accounts

When a user creates a new account (for example after identity loss with no recovery), the new account cannot claim the old AccountID. If the old account is still controllable, `AccountRetirement(moved, successor)`, authorized under §2 rule 1, gives contacts a signed pointer, and they show "Alice moved to a new OM account". That object is the **only** cryptographic continuity proof clients render as "moved" (ADR-0127). A successor pointer without that authority is rejected. Guardians do not bind a successor: guardian recovery keeps the same AccountID ([`RECOVERY_AND_BACKUP_POLICY.md`](RECOVERY_AND_BACKUP_POLICY.md) §10.3). Otherwise continuity is human evidence: re-verification or an introduction ([`IDENTITY_CONTINUITY.md`](IDENTITY_CONTINUITY.md) Level 3).

## 6. Verification

V-002 now includes:

- retirement authorization under the parent state, including a recovery-vote retirement with delay and veto;
- a successor pointer without constitutional authority is rejected;
- no descendant after a terminal state;
- retirement/promotion races;
- a legacy retirement cancelled by a live controller during the contest window;
- a LegacyPolicy attempting forbidden powers.
