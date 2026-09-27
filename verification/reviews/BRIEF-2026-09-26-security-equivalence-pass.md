# Brief — security-equivalence verification pass after ADR-0137

## Execution status — 2026-09-26 16:52 PHT

This brief has now been executed as an evidence pass by GPT-5.6 Sol; see
[`2026-09-26-security-equivalence-evidence-report.md`](../../docs/reports/2026-09-26-security-equivalence-evidence-report.md).
The bounded/structural Python work packages pass. Fresh TLC and Rust dynamic reruns were not possible in that runner
because the pinned TLC JAR and Rust toolchain were unavailable; the report records the exact residual commands.
The brief remains the acceptance checklist for the eventual independent/tool-complete rerun.

## Purpose

This is the **next-agent** verification brief for ADR-0137. The ADR-0137 author intentionally did not perform this pass. The goal is not another broad architecture review; it is to prove that the independent executable/formal representations accept the same security rules and that no optional API path bypasses them.

Read first: [`AGENTS.md`](../../AGENTS.md), [`SNAPSHOT.md`](../../SNAPSHOT.md), ADR-0137, [`SECURITY_INVARIANTS.md`](../../docs/architecture/safety/SECURITY_INVARIANTS.md), the 2026-09-26 GPT-5.6 Sol core-kernel re-review, and the four open findings F-10..F-13.

Do not begin product engineering. Verification prototypes and formal/bounded evidence are allowed under ADR-0134.

## Work package 1 — ACS accepted-set equivalence (F-10)

Treat [`spec/protocol/account-control-transition-policy.json`](../../spec/protocol/account-control-transition-policy.json) as the normative policy input and [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) as its human explanation.

Required work:

1. update/confirm the TLA `ResolveFreeze` action implements **selective threshold-restoring restoration**, not “clear every quarantine”;
2. enumerate bounded frozen parents and candidate children on both sides;
3. prove/check both directions:
   - every TLA constitutional transition is accepted by the executable authorization model; and
   - every executable-model accepted transition in the bounded domain corresponds to a permitted TLA transition/action class;
4. include explicit negative cases:
   - partial restoration that leaves the child below the parent majority;
   - recovery vote added to a freeze resolution;
   - challenge signer not in the parent controller set;
   - challenge signature used for an ordinary/non-freeze transition;
   - freeze-resolution child that also promotes/revokes/changes recovery/security policy;
5. keep fork bridge distinct from quarantine-freeze resolution; and
6. rerun TLC traps/invariants and regenerate committed vectors/results.

Acceptance condition: bounded accepted sets are equal for the covered ACS transition classes, not merely TLA→Python compatible.

## Work package 2 — relationship-context non-bypass (F-11)

Required work:

1. enumerate every public/prototype inbound 1:1 entry point that can process a new or existing session envelope;
2. demonstrate there is no callable path that omits the expected peer AccountID/RelationshipContext;
3. test a valid initiation delivered under the wrong accepted relationship and require failure before secret-key work or state change;
4. repeat the wrong-route test for an already-established session, not only first session creation;
5. retain negatives for unknown, revoked, quarantined, stale and ambiguous DeviceCredentials; and
6. prove persisted/reloaded state does not reintroduce an unpinned helper.

Acceptance condition: the secure context is structurally required by the API surface; a boolean/`Option`/default cannot disable it.

## Work package 3 — authentication-layer metadata coverage (F-12)

Required work:

1. extend the metadata flow model to include at least descriptor publication, RecoveryRecord put and blob-location repair;
2. model `device_auth` itself, not only request bodies;
3. assert `stable_device_correlator` matches operation frequency for each receiving role;
4. include same-operator Home colocation as one observer;
5. add mutations that incorrectly return affected roles to `no_by_design`; and
6. compare the human observer matrix with the machine descriptor for role/field drift.

Acceptance condition: an auth-layer stable signer cannot be omitted while the model still passes.

## Work package 4 — referral issuance unlinkability/freshness (F-13)

Required work:

1. retain the RFC 9474 and RFC 9578 known-answer tests;
2. deterministic positive control: several issuances with distinct known invertible blinding factors;
3. deterministic negative control: several issuances reusing one nontrivial factor such as `r=2`;
4. compute the issuer-side candidate factor matrix from issuance transcript and redeemed authenticator pairs and demonstrate the reuse creates a repeated/linkable marker;
5. retain a direct-sign/unblinded negative as a separate detector control; and
6. keep network/timing/cardinality and ReferralAttestation leakage outside the cryptographic claim rather than “testing” them as Blind-RSA properties.

Acceptance condition: both `r=1` and reused nontrivial-factor regressions fail the unlinkability/freshness evidence, while fresh distinct factors and the CSPRNG implementation path pass.

## Work package 5 — negative-capability conformance shape

Do not try to prove an operator cannot modify open-source software. Instead test the canonical interfaces/credentials:

- provider interfaces do not supply account-control signing keys or E2EE content keys to Relay/Home/Directory/Recovery/Push/Blob roles;
- combining roles joins observations but does not satisfy a cryptographic authorization rule that the individual credentials could not satisfy;
- recovery credentials alone cannot sign an ordinary ACS transition;
- directory/alias claims cannot be accepted as DeviceCredential/ACS authority; and
- IntroGate public verification material cannot mint issuer tokens.

Record these as conformance/negative cases, not as a promise that a malicious host cannot run arbitrary extra software.

## Deliverables

- changed evidence/model files only as needed;
- regenerated committed results/vectors for the work above;
- one independent-review-ready report mapping each result to F-10..F-13 and `A*/I*/M*/P*/C*` invariants;
- no engineering-scope unlock unless the existing gate process and ADR-0117 independence requirements are actually satisfied.

Do **not** bounce individual failures back to the design author one at a time. Complete the whole equivalence pass, collect any genuine design contradictions, then return one consolidated result.
