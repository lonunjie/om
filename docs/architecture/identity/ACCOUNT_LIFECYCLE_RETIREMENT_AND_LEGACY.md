# Account Lifecycle End: Retirement, Deletion, Succession and Legacy

- **Status:** Canonical architecture
- **Checkpoint:** 2026-09-26 design closure set 1
- **Decision:** ADR-0105, ADR-0138
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

LegacyPolicy is **architecturally closed but not enabled in W0** (ADR-0138). A future profile that enables it commits
the policy in the critical AccountControlState key-23 extension `om.acs.legacy_policy`. The commitment fixes the
guardian verification credentials and threshold, an `owner_inactivity_wait_seconds`, a
`minimum_contest_window_seconds`, and optionally one pre-designated successor AccountID. W0 verifiers continue to
reject `authorization_kind = legacy_policy`; enabling the extension requires an explicit compatible profile/wire
upgrade rather than silently changing W0 semantics.

Legacy guardian credentials are a separate authority class. They are not controllers, RecoveryPolicy votes,
messaging devices or provider credentials. They can authorize only the narrow terminal path described here.

### 4.1 Verifier-local inactivity; no trusted death clock

OM cannot prove death or inactivity from a provider timestamp. A threshold-signed legacy claim therefore starts a
**verifier-local dormancy interval**. Each verifier records its own first valid observation of the claim and waits
`owner_inactivity_wait_seconds` using monotonic elapsed time. Home timestamps, guardian wall clocks and a claimed
"last seen" value cannot shorten the interval.

Any valid newer owner-authorized AccountControlState transition or a legacy veto during this interval cancels the
claim. This is deliberately conservative: time never becomes authority; guardian threshold signatures remain
necessary, and a malicious provider cannot age a claim into validity.

### 4.2 LegacyRetirementPending

Only after the local inactivity interval may the same claim enter **`LegacyRetirementPending`**. That state is
non-terminal for at least the policy's committed `minimum_contest_window_seconds`. Contacts continue treating the
account as live during the contest period. They may display that a legacy request is pending, but they do not render
**Closed** until the terminal retirement is accepted.

Any controller authorized in the parent state may veto during either waiting interval. A quarantined parent
controller may use only its narrow quarantine/freeze challenge credential to veto this irreversible action; that
signature grants no ordinary AccountControlState authority. A veto cancels the pending legacy action rather than
creating a competing constitutional branch.

### 4.3 Narrow guardian authority

After both waiting intervals, a verifier may accept `AccountRetirement(reason_class = legacy)` only if the guardian
threshold still verifies against the committed policy. Legacy authority may:

- retire the account;
- include an optional memorial note; and
- use the successor AccountID only if that exact successor was committed in LegacyPolicy.

It can **never**:

- decrypt/export history or obtain backup keys;
- send messages as the account;
- enroll, promote, revoke or quarantine devices;
- change RecoveryPolicy, LegacyPolicy or any constitutional threshold;
- designate a successor that the user did not pre-designate; or
- turn guardian credentials into ordinary account/controller authority.

History inheritance, if wanted, is a separate explicit export/grant made while the user is alive. It is not a
LegacyPolicy power.

### 4.4 Failure preference

The design intentionally prefers delayed/non-completion over false retirement. A malicious guardian quorum can start
the two waiting intervals, but cannot make provider time authoritative, skip the owner's veto opportunity or gain any
other account power.

## 5. Moved accounts

When a user creates a new account (for example after identity loss with no recovery), the new account cannot claim the old AccountID. If the old account is still controllable, `AccountRetirement(moved, successor)`, authorized under §2 rule 1, gives contacts a signed pointer, and they show "Alice moved to a new OM account". That object is the **only** cryptographic continuity proof clients render as "moved" (ADR-0127). A successor pointer without that authority is rejected. Guardians do not bind a successor: guardian recovery keeps the same AccountID ([`RECOVERY_AND_BACKUP_POLICY.md`](RECOVERY_AND_BACKUP_POLICY.md) §10.3). Otherwise continuity is human evidence: re-verification or an introduction ([`IDENTITY_CONTINUITY.md`](IDENTITY_CONTINUITY.md) Level 3).

## 6. Verification

V-002 now includes:

- retirement authorization under the parent state, including a recovery-vote retirement with delay and veto;
- a successor pointer without constitutional authority is rejected;
- no descendant after a terminal state;
- retirement/promotion races;
- a legacy claim whose provider/guardian timestamp tries to shorten the verifier-local inactivity interval;
- a legacy retirement cancelled by an eligible controller during dormancy or contest, including the narrow quarantined-controller veto;
- a contest window below the committed minimum;
- a successor different from the pre-designated successor; and
- a LegacyPolicy attempting forbidden powers.
