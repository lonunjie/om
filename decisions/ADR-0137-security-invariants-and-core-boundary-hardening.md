# ADR-0137 — Security invariants and core-boundary hardening

- **Status:** Accepted for verification; independent re-review required under ADR-0117
- **Date:** 2026-09-26
- **Author:** GPT-5.6 Sol design-hardening pass
- **Remediates:** independent re-review findings F-10..F-13 without self-closing them
- **Amends:** ADR-0120, ADR-0127, ADR-0132, ADR-0135, ADR-0136
- **Adds:** canonical security-invariant map, ACS transition policy, W0 metadata observer matrix and a focused verification-equivalence handoff

## Context

The 2026-09-26 independent re-review found four remaining gaps after ADR-0136:

- F-10: the TLA and executable AccountControlState encodings admitted different safety-freeze resolution children;
- F-11: the V-001 prototype still exposed ordinary receive/decrypt entry points that could omit the relationship route pin required by K12;
- F-12: W0 metadata accounting omitted stable DeviceCredential correlation visible in `device_auth` control-plane operations; and
- F-13: the issuance-unlinkability test detected the unblinded `r=1` mutation but not reuse of a nontrivial blinding factor.

All four are instances of the same architectural failure mode: an important security property existed in prose, but an implementation-facing boundary still permitted a weaker interpretation. OM's constitution prefers structural privacy and least authority over promises that callers will remember optional checks.

## Decision

### 1. AccountControlState transition authority has one normative policy artifact

[`spec/protocol/account-control-transition-policy.json`](../spec/protocol/account-control-transition-policy.json) is the normative machine-readable authorization policy for AccountControlState deltas. [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) remains the normative human explanation, but its authorization table is a rendering/explanation of the policy rather than a second independently invented rule set.

The bounded authorization model loads the policy artifact. Formal models may remain independent encodings so they can catch errors, but they must not silently redefine the accepted transition set. Bidirectional equivalence testing is intentionally left to the focused follow-up brief; this ADR does not claim it has been executed.

### 2. Safety-freeze resolution is selective, threshold-restoring and challenge-only

A quarantine-induced `SAFETY_FREEZE` is resolved by a special transition with these exact rules:

1. the parent is below its voting threshold because one or more parent controllers are quarantined;
2. the child changes only the quarantine set (plus ordinary generation/authorization fields); it cannot promote, revoke, change recovery policy, change security policy or perform another constitutional delta;
3. the restored set is a **non-empty subset** of the parent's quarantined controllers, and every restored controller must contribute its own freeze-challenge signature;
4. authorization requires a device-only majority `floor(N/2)+1` of the **parent** controller set;
5. an active controller signs normally; a quarantined parent controller may sign only with its narrow freeze-challenge capability;
6. a recovery vote contributes zero and is rejected if present; and
7. after the selected restorations, the child must contain at least the parent majority of active, non-quarantined controllers. A child that remains below threshold is not a freeze resolution.

This deliberately does **not** restore every quarantined controller. A controller believed compromised can remain quarantined while enough safe controllers are restored to regain constitutional operation. The challenge capability is not general authority: it is accepted only for this single transition class and cannot authorize ordinary AccountControlState changes.

This mechanism does not promise liveness against a withholding threshold member. For `N=2`, the parent majority is two; a malicious controller can still deny availability by refusing to sign. OM does not weaken the threshold or add recovery authority merely to hide that physical limitation.

Fork resolution remains separate: a fork uses the existing §8 `fork_ack` bridge authorized by a device-only majority of the common ancestor. A quarantine-freeze resolution is not a fork bridge and vice versa.

### 3. Relationship binding is mandatory input, not an optional mode

K12 is strengthened from a semantic check into an API-shape invariant:

- every inbound 1:1 envelope is processed under a `RelationshipContext` containing the expected peer AccountID and trusted peer AccountControlState view;
- no public/new-session receive or decrypt entry point may accept an inbound envelope without that context;
- the sender `IK_A` must resolve to exactly one active, non-quarantined DeviceCredential committed by that expected peer's trusted ACS head **before** any private prekey operation, one-time-key consumption, ratchet/session creation or plaintext release;
- the same route/account pin is checked for already-established sessions, so moving an existing sender session onto another relationship route is not an alternate acceptance path; and
- a stale head is refreshed under K7 before processing, never after plaintext release.

A transport locator, mailbox, relay identity or directory result cannot manufacture the relationship context. It comes from already accepted relationship state. First contact remains HPKE `MessageRequest`; an unbound PQXDH initiation never creates a relationship.

### 4. W0 metadata accounting includes authentication-layer observations

[`METADATA_OBSERVER_MATRIX.md`](../docs/architecture/providers/METADATA_OBSERVER_MATRIX.md) is the canonical human observer/collusion matrix. The machine descriptor remains [`w0-metadata-exposure-descriptors.json`](../spec/metadata/w0-metadata-exposure-descriptors.json).

W0 `device_auth` is explicitly **not anonymous authentication**. For an operation authenticated by an active DeviceCredential, the receiving provider can at minimum correlate the stable signer DeviceID/credential across those authenticated operations. Therefore:

- discovery publication has a stable-device correlator when publication/authenticated maintenance occurs;
- RecoveryRecord storage has a stable-device correlator on `device_auth` writes;
- blob-location repair has a stable-device correlator when the `device_auth` branch is used;
- KeyMaterialProvider and mailbox roles retain their stronger existing disclosures; and
- push providers retain the push-token correlator.

Descriptors use `sometimes` where only a subset of a role's W0 operations uses `device_auth`, and `always` only where every relevant observation or an independent always-present field exposes the device.

The privacy model distinguishes cryptographic/protocol unlinkability from network, timing, cardinality and operator-collusion observations. Roles operated by one entity are one observer regardless of internal service names.

### 5. W0 referral issuance unlinkability has an explicit observer/property boundary

For the W0 RFC 9578 type `0x0002` referral profile, the normative cryptographic property is:

> Given issuance transcripts and later redeemed token bytes, the issuer — and an IntroGate or other party that obtains/colludes with those issuance transcripts — must not be able, from the Blind-RSA protocol values themselves, to determine which issuance transaction produced a particular redeemed token, except with negligible advantage.

This property requires every issuance to sample a fresh, independently generated, uniformly distributed non-zero blinding scalar from a CSPRNG (resampling values not invertible modulo `n`). Reusing a blinding scalar across issuances violates the W0 property even when the scalar is not `1`.

The property does **not** claim to hide:

- which contact participated in an issuance from the issuer conducting that issuance;
- redemption network address, timing, token-use count or ticket metadata from the IntroGate;
- timing/cardinality correlation by an issuer/IntroGate coalition;
- the optional ReferralAttestation after the recipient decrypts the sealed request; or
- information a user voluntarily reveals outside the Blind-RSA protocol.

The deterministic issuance evidence therefore includes both a positive control with distinct known invertible factors and a negative control that reuses the same nontrivial factor. The ordinary implementation path continues to use a CSPRNG rather than deterministic factors.

### 6. Provider and subsystem interfaces declare negative capabilities

OM now treats “must not be able to do X” as part of an interface contract. A role's prohibited powers must be absent from its normal credentials, key material and interface surface, not merely forbidden by operator policy.

At minimum:

- a Relay/Home mailbox cannot decrypt message payloads, sign AccountControlState transitions, become an identity root, or mint recipient referral tokens merely because it carries them;
- a DirectoryProvider cannot authorize account controllers, permanently bind AccountID globally, or turn an alias into identity authority;
- a RecoveryStore cannot independently authorize AccountControlState transitions, recover message history plaintext, or become account authority;
- an IntroGate cannot decrypt MessageRequest plaintext, mint the recipient's issuer credentials, or derive a stable requester identifier from the referral token protocol;
- a BlobStore cannot obtain content keys or derive canonical identity/ConversationID from blob locators by protocol design;
- a PushProvider gets only opaque wake capability and no message/account authority; and
- a network `CryptoProvider` does not exist in W0: `CryptoProvider` is a local implementation boundary and cannot become a remote mandatory crypto oracle.

If an implementation combines roles, it gains the union of their observations but no new protocol authority. Combining roles never authorizes a transition that neither role could authorize alone.

## Consequences

- F-10 has a single chosen resolution shape: selective restoration is valid only when it restores the voting threshold. The old TLA “clear every quarantine” shape is no longer normative.
- F-11's optional route argument is forbidden by architecture. Prototype-facing receive/decrypt paths are changed to require the expected relationship AccountID.
- F-12's missing authentication-layer correlation is declared and modeled in the W0 descriptors.
- F-13's security property and nontrivial factor-reuse mutation are explicit.
- [`SECURITY_INVARIANTS.md`](../docs/architecture/safety/SECURITY_INVARIANTS.md) maps the key invariants to their normative source and evidence surface.
- This ADR does not self-pass V-001/V-002/V-015/V-016 or close F-10..F-13. The author changed the relevant artifacts and is not independent for their re-review under ADR-0117.
- The cross-model/equivalence work described in [`BRIEF-2026-09-26-security-equivalence-pass.md`](../verification/reviews/BRIEF-2026-09-26-security-equivalence-pass.md) is intentionally **not executed in this design pass**.
