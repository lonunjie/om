# Security-equivalence evidence pass after ADR-0137

- **Executor:** GPT-5.6 Sol
- **Date:** 2026-09-26 16:52 PHT (UTC+08:00)
- **Base export commit:** `fa8370d665eb9e3e6659040f332fb3305b8b2299`
- **Scope:** point-6 cross-model equivalence/conformance pass from [`BRIEF-2026-09-26-security-equivalence-pass.md`](../../verification/reviews/BRIEF-2026-09-26-security-equivalence-pass.md)
- **Role:** verification evidence producer, **not** ADR-0117 independent reviewer of ADR-0137 (same model authored ADR-0137 hardening)
- **Engineering effect:** none; no scope unlocked and `engineering_authorized=false`

## Executive result

No new design contradiction was found in the ADR-0137 rules. The bounded/executable portions of all five work
packages pass. The pass materially strengthens the evidence in exactly the areas exposed by F-10..F-13:

- ACS freeze-resolution accepted sets are checked **bidirectionally**, not only by replaying TLA traces into Python;
- every public V-001 inbound device boundary structurally requires `RelationshipContext`, with established-session,
  reload and ambiguous-credential negatives now present in the Rust selftest source;
- authentication-layer metadata is mutation-tested per affected provider and the human observer matrix is checked
  against the machine descriptor;
- Privacy Pass issuance has separate regressions for `r=1`, repeated nontrivial `r=2`, direct signing and the actual
  CSPRNG path; and
- provider role interfaces are checked for negative capabilities and authority laundering.

Two environment/tooling reruns could not be freshly executed here: **TLC** (the pinned `tla2tools-1.7.4.jar` is not
present and the isolated runner cannot fetch it) and the **Rust V-001 selftest** (`cargo`/`rustc` are absent). Existing
committed TLC/Rust/wasm results were left intact rather than being falsely regenerated. The new Python structural and
bounded evidence is fresh. A future runner with those tools should rerun `python3 tools/run_tlc.py` and the V-001
prototype test driver before treating this report as the final tool-complete equivalence artifact.

## Work package 1 — ACS accepted-set equivalence — F-10 / A1-A7

**Verdict: bounded equivalence PASS; fresh TLC rerun pending tooling.**

[`verification/bounded/acs_tla_crosscheck.py`](../../verification/bounded/acs_tla_crosscheck.py) now has two independent layers:

1. committed TLC counterexample/golden traces are replayed through the executable authorization table; and
2. the selective `ResolveFreeze` constitutional relation is independently enumerated on both sides, with the
   relevant TLA source clauses checked for drift.

Fresh result ([`verification/results/acs-tla-crosscheck-latest.json`](../../verification/results/acs-tla-crosscheck-latest.json)):

- committed TLA traces: **11**;
- constitutional trace steps replayed: **13** total, **12** positive, **17** negative mutations, **1** intentionally
  skipped non-row-equivalent step;
- bounded freeze candidates compared: **66,816**;
- TLA accepted: equal to executable accepted set;
- executable-only acceptances: **0**;
- TLA-only acceptances: **0**;
- explicit negative failures: **0**.

The explicit negatives cover all requested cases: insufficient selective restoration, recovery vote in resolution,
outsider challenge signer, challenge signer on an ordinary lift, resolution combined with promotion/revocation/
recovery-policy/constitutional-policy/minimum-security changes, and fork-bridge confusion. The ordinary executable
authorization table also reran clean: **16,200 cases, 0 violations, 56 vectors**.

This closes the *logical* gap behind F-10: the executable model cannot accept a bounded selective-freeze transition
that the TLA `ResolveFreeze` rule rejects, or vice versa, in the covered constitutional dimensions. It does not
replace TLC's state-reachability/trap checking; the committed [`tlc-latest.json`](../../verification/results/tlc-latest.json) remains historical evidence until the
pinned JAR can be rerun against the current tree.

## Work package 2 — relationship-context non-bypass — F-11 / I1-I5

**Verdict: structural/API PASS; fresh Rust dynamic rerun pending tooling.**

New executable source-surface evidence:
[`verification/bounded/relationship_context_surface.py`](../../verification/bounded/relationship_context_surface.py) and
[`verification/results/relationship-context-surface-latest.json`](../../verification/results/relationship-context-surface-latest.json).

The check enumerates the public `Device` ingress boundaries that process inbound 1:1 envelopes:

- `Device::receive`
- `Device::decrypt`

Both require a non-optional `&RelationshipContext`. There is no public `decrypt_on_route`/unpinned compatibility path.
The analyzer also requires `binding::verify_initiator(...)` to appear before session lookup and before
`accept_initial`, requires an exact-one active credential scoped to `relationship.peer_account_id`, and requires
persisted peer state to rebuild the derived credential index through `reindexed()`.

Three negative mutations are caught: making relationship context optional, moving binding after session lookup, and
removing the reload negative test.

The Rust selftest source now additionally contains:

- established-session wrong accepted relationship => `Binding` failure with device state unchanged;
- same established envelope on the correct relationship => accepted;
- serialized/reloaded established state on the wrong relationship => failure before vault commit and no state change;
- serialized/reloaded state on the correct relationship => accepted and committed; and
- two active same-account credentials committing the same `IK_A` => ambiguous/fail closed.

Existing unknown, route-mismatch, revoked, quarantined, stale and substituted negatives remain. Because Rust is not
installed in this environment, those newly added dynamic cases are **present but not freshly compiled/executed** in
this pass. The structural check itself passes and directly verifies the non-optional API and pre-state-change ordering
that F-11 was about.

## Work package 3 — authentication-layer metadata — F-12 / M1-M4

**Verdict: PASS.**

Fresh results:

- [`metadata_w0_rules.py`](../../verification/bounded/metadata_w0_rules.py): every criterion and mutation passes;
- [`metadata_trace_model.py`](../../verification/bounded/metadata_trace_model.py): **216 cases / 12 roles / 0 violations**;
- per-role auth-layer mutations for discovery publisher, RecoveryRecord store and BlobStore are all caught.

The stable-device frequencies are now asserted exactly:

- mailbox-class Homes: `always` because every 1:1 envelope exposes sender `EncodeEC(IK)`;
- KeyMaterialProvider: `always` because AsyncKeyBundle exposes subject DeviceID;
- push provider: `always` for the registered push token/device handle;
- discovery publisher: `sometimes` due authenticated publication/maintenance;
- RecoveryRecord store: `sometimes` due authenticated record writes;
- BlobStore: `sometimes` due the `device_auth` repair branch.

`observer_matrix_alignment` parses the human [`METADATA_OBSERVER_MATRIX.md`](../architecture/providers/METADATA_OBSERVER_MATRIX.md) and compares those frequencies with the
machine descriptor. A deliberate descriptor downgrade is detected. Same-operator Home role colocation remains modeled
as one observer for privacy analysis.

This satisfies the F-12 information need: W0 no longer makes a body-only metadata claim that can omit a stable signer
visible at the authentication layer.

## Work package 4 — referral issuance unlinkability/freshness — F-13 / P1-P4

**Verdict: PASS.**

RFC 9474 and RFC 9578 known-answer tests remain green. The issuance suite now separates four properties that were
previously conflated:

- distinct known invertible factors (`2,3,5,7`) => pass;
- repeated nontrivial `r=2` => issuer candidate matrix finds the repeated marker at the true transcript/token pairs;
- repeated `r=1` => detected as the unblinded regression; and
- direct signing => detected independently from blind-factor reuse.

The actual production-like `Client.request(..., r=None)` path is exercised for 16 issuances. The test checks the
`secrets.randbelow(n - 1) + 1` path, nonzero range, invertibility and absence of collisions in that bounded sample.
A mutation that forces `r=2` is caught. This is not presented as a statistical proof of uniformity; uniform sampling
is the `secrets` CSPRNG contract plus the normative ADR-0137 requirement.

The claim remains intentionally narrow under P4: IP address, timing, cardinality, batching and ReferralAttestation
side information are **not** claimed to be hidden by Blind RSA.

## Work package 5 — negative-capability conformance — C1-C6

**Verdict: PASS.**

New evidence:
[`verification/bounded/provider_negative_capabilities.py`](../../verification/bounded/provider_negative_capabilities.py) and
[`verification/results/provider-negative-capabilities-latest.json`](../../verification/results/provider-negative-capabilities-latest.json).

All canonical-role checks and their mutations pass:

- Relay/Home, Directory, Recovery, IntroGate, Blob/Storage and Push interface blocks do not carry end-user ACS signing
  keys, E2EE content keys or referral issuer private keys;
- the explicit negative-capability contract rows are present;
- a RecoveryStore `recovery_auth`/transport credential alone does not become the constitutional RecoveryPolicy vote;
  a mutation that launders it into that vote is caught;
- ContactDescriptor remains a distinct object type and signature domain from DeviceCredential and AccountControlState;
- IntroGate holds only public verification material and a zero-signature forgery is rejected; supplying the issuer
  private key as a mutation is caught; and
- colocating mailbox, directory and recovery-store roles yields only the union of their service powers; a synthetic
  `sign_account_control_state` power is caught.

These are interface/credential conformance properties, not a claim that an operator of open-source software cannot
write unrelated malicious code.

## Mapping to open findings and invariants

| Finding | Evidence produced here | Invariants | Closure state from this executor |
|---|---|---|---|
| F-10 | bidirectional bounded ACS accepted-set equality + explicit freeze/fork negatives | A1-A7 | evidence-ready; independent review still required |
| F-11 | mandatory ingress API surface + pre-session binding ordering + established/reload/ambiguity Rust negatives | I2-I5 | evidence-ready; Rust dynamic rerun and independent review still required |
| F-12 | exact auth-layer metadata frequencies, per-role mutations, matrix/descriptor drift test | M1-M4 | evidence-ready; independent review still required |
| F-13 | RFC KATs + r=2 reuse matrix + r=1 + direct-sign + CSPRNG path | P1-P4 | evidence-ready; independent review still required |
| cross-cutting | canonical provider negative-capability conformance | C1-C6 | evidence-ready; independent review still required |

This executor does **not** fill the `Resolution:` lines in the prior independent re-review and does not mark the gates
passed: ADR-0137 and these evidence changes were produced by the same model family identity, so ADR-0117's independent
review requirement is still meaningful.

## Commands executed successfully

```text
python3 verification/bounded/acs_authorization_table.py
python3 verification/bounded/acs_tla_crosscheck.py
python3 verification/bounded/relationship_context_surface.py
python3 verification/bounded/metadata_w0_rules.py
python3 verification/bounded/metadata_trace_model.py
python3 verification/bounded/abuse_w0_rules.py
python3 verification/bounded/privacy_pass_issuance.py
python3 verification/bounded/provider_negative_capabilities.py
```

## Repository integration checks

After registry integration and generated-view refresh:

- architecture validator: **pass** — 748 current files, `design_open=4`, `verification_open=23`, `pre_engineering_open=22`, `engineering_authorized=false`, 0/11 scopes authorized;
- documentation/link check: **pass**;
- file-registry drift check: **pass**;
- continuity normative-manifest check in a disposable Git probe: **78 docs / 0 problems**; and
- offline continuity bundle: **369 files**, integrity verification **pass**.

No gate status or independence field was promoted by this evidence pass.

## Tool-dependent completion commands

On a runner with the pinned dependencies available:

```text
python3 tools/run_tlc.py
verification/prototypes/v001-crypto/run.sh
```

The independent reviewer should consume the regenerated results from those commands plus this report. No engineering
scope should be unlocked solely from this executor report.
