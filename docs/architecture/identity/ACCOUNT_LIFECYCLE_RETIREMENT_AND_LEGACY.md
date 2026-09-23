# Account Lifecycle End: Retirement, Deletion, Succession and Legacy

- **Status:** Canonical architecture
- **Checkpoint:** 2026-09-24 02:29 PHT
- **Decision:** ADR-0105
- **Gate:** V-002 (extended)
- **Schema:** `protocol/account-retirement.logical.schema.json`

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
       successor_account_id?      // only with continuity proof or explicit "moved" statement
       farewell_note?             // optional short text, E2EE to contacts
       effective_at               // informational clock class; not authority
       authorization_signatures[] // parent-state constitutional rule or RecoveryPolicy or LegacyPolicy
     }
```

Rules:

1. It is authorized exactly like any constitutional change: by the **parent** state's rule, a valid RecoveryPolicy, or (only for `reason_class = legacy`) a valid LegacyPolicy (§4). A single quarantined or ordinary messaging device cannot retire an account.
2. It is **terminal**. No later state descends from a retired state. A competing descendant is a fork and follows the existing safety-freeze/fork rules.
3. Clients that receive it through relationship channels or head gossip render the contact as **Closed** (or "Moved to …" with a successor). They stop sending new messages, stop encrypting to the account's devices, and remove its delivery capabilities from their own RelaySets.
4. The user's own Homes, on receiving it from a control device, purge queued envelopes and revoke the account's capabilities. Provider compliance is best-effort and cannot be proven; clients state that.
5. Retirement never deletes other people's copies of past messages. The UI says so ("People you talked to keep their copies").

## 3. Deletion

"Delete my account" = AccountRetirement(`user_closed`), then local wipe of every profile store on each device, then Home purge. It is one user action with a clear confirmation. It needs no provider or project involvement.

A user who only wants to leave one device uses device removal, not retirement.

## 4. LegacyPolicy (optional, off by default)

For death or permanent incapacity, a user may configure a **LegacyPolicy** naming legacy contacts (guardians) and a threshold. It is part of the RecoveryPolicy family, authorized and committed the same way (R-029: explicit user action only).

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

Abuse guard: a legacy retirement starts a **contest window** that live controllers can see. Any valid controller signature during the window cancels it. This deliberately does not follow the quarantine rule: legacy retirement is irreversible, so it must be cancellable by the living owner. Time never grants authority by itself here either, because the threshold signatures are still required.

## 5. Moved accounts

When a user creates a new account (for example after identity loss with no recovery), the new account cannot claim the old AccountID. Continuity follows `IDENTITY_CONTINUITY.md`: guardian continuity proof, re-verification or an introduction. If the old account is still controllable, `AccountRetirement(moved, successor)` gives contacts a signed pointer, and they show "Alice moved to a new OM account" (existing UX).

## 6. Verification

V-002 now includes:

- retirement authorization under the parent state;
- no descendant after a terminal state;
- retirement/promotion races;
- a legacy retirement cancelled by a live controller during the contest window;
- a LegacyPolicy attempting forbidden powers.
