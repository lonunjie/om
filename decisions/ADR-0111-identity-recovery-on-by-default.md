# ADR-0111 — Identity recovery is on by default

- **Status:** Accepted
- **Date:** 2026-09-24
- **Decided by:** operator ("why don't we start with recovery on? that's why we are designing it here")
- **Supersedes:** ADR-0026 (recovery optional and off by default). **Amends:** ADR-0106, Q-003 default, D-042, D-128.

## Context

ADR-0106 kept recovery off by default and added an "informed-off" continuity prompt. Most people have one phone.
Under that default, a lost phone lost the identity, and a phone-plus-laptop user who lost one device could never
again change account control. A prompt that most users dismiss does not fix a default that fails the common case.
The architecture exists precisely so that recovery can be on without a recovery operator.

## Decision

1. Every identity is created with a working RecoveryPolicy ([`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) §10). The custodian is
   chosen automatically: a synced passkey with WebAuthn PRF, else a recovery key. Guardians, spare devices and
   security keys are added options. Turning recovery off is an explicit, confirmed choice that keeps disposable
   identities first-class.
2. **No silent takeover.** A recovery request on its own completes only after a delay (Recommended 72 h,
   Hardened/Maximum 7 d, never below 24 h), and any live controller can veto it. It completes immediately only
   when at least one current controller co-signs. Completion revokes every earlier controller, rotates the recovery
   epoch and shows contacts a continuity notice.
3. **One vote.** The RecoveryRoot counts as exactly one vote toward the `floor(N/2)+1` majority. That removes the
   two-device constitutional lockout, because survivor plus recovery makes a majority.
4. **Separate secrets, one custodian.** Identity, provider-metadata, relationship and history keys stay separate.
   One default custodian may wrap several of them. Only the identity key confers authority, and only through rule 2.
5. Platform credential managers see a random RecoveryHandle, never the AccountID. Maximum Privacy uses a recovery key
   only.

## Alternatives considered

- *Informed-off (ADR-0106):* rejected. It keeps the failure as the default.
- *Mandatory recovery key at signup:* rejected. Completion rates for "write this down" steps are poor, and it fails
  the Wi-Fi test.
- *Operator-held escrow or enclave (SVR-style):* rejected. It creates the recovery operator OM exists to avoid.
- *Recovery with no delay:* rejected. A phished platform account would then be an instant takeover.

## Consequences

The 1- and 2-device rows of the ContinuityAssessment now show no loss or lockout while a custodian survives. New
residual risk T-041: an attacker who holds the custodian can win against a user who has no device left to veto
within the delay. V-002 must model activation, veto and the one-vote rule. V-006 must show PRF on synced passkeys
and `signalCurrentUserDetails` across the browser matrix. V-018 adds the "recovery on" and "turn off" flows.
