# core_kernel consolidated review — Claude Opus 5.5, 2026-09-29

- **Reviewer:** Claude Opus 5.5 (`claude-opus-5-5`), under the ADR-0167 operator review model. This is **not**
  vendor-independent review; gates passed here record `pre_engineering_review_basis=operator_single_vendor_ADR-0167`.
- **Brief:** [`BRIEF-2026-09-27-final-core-kernel-independent-review.md`](BRIEF-2026-09-27-final-core-kernel-independent-review.md)
- **Tree:** om `main` after ADR-0166 (`0904865`) plus this review's fixes (ADR-0168).
- **Scope:** V-002, V-003, V-005 (independent implementation), V-015, V-016, V-017, and a regression check of F-1..F-13.

## Commands re-executed (tool-complete worktree, macOS arm64)

| Command | Result |
|---|---|
| `make verify` (before fixes) | pass except registry immutability vs the non-adopted Run tag (resolved by ADR-0166 [`machine/checkpoint_lines.json`](../../machine/checkpoint_lines.json)) |
| `verification/prototypes/v003-sessions/run.sh` | pass: TLC + Hypothesis; K12 `BindInitiator` mutation caught (`NoMisattributedSession`); 17 floor vectors. The committed result was stale (14 vectors, no K12 row) and is refreshed |
| `verification/prototypes/v001-crypto/run.sh` | pass: native, wasm32-wasip1, wasm32-unknown-unknown, HPKE-PQ-05 KATs, ML-KEM ACVP, persistence chains, independent Python check; deterministic result unchanged |
| provider-resource admin trace | 8 routine + 3 RecoveryRequest fan-out traces, 0 violations, 7/7 security mutations caught |
| `verification/prototypes/v005-encoding/run.sh` | **failed** on the current tree (F-15), then **passed** after the fixes: Rust 758/758, Node 758/758, both CDDL tools, bench block retained |
| [`acs_authorization_table.py`](../bounded/acs_authorization_table.py), [`acs_tla_crosscheck.py`](../bounded/acs_tla_crosscheck.py) | pass after ADR-0168: 16,200 cases / 63 vectors; 98,496 freeze cases, 847 = 847 accepted |
| TLC AccountControl configs (20) | all as expected after ADR-0168; general config 1,952,289 distinct states |
| [`extensions_independent_rust_adr0168.py`](../bounded/extensions_independent_rust_adr0168.py) | pass: clean-room Rust agrees on 64/64 verdicts and canonical CBOR bytes |

## Findings

| ID | Severity | Summary | Resolution |
|---|---|---|---|
| F-14 | **Blocker** | One stolen device could permanently lock a two-controller account: quarantine-induced freeze resolution was device-only and restore-only, and recovery was barred. The owner could also trigger it by suspending the stolen phone. | ADR-0168: freeze resolution has normal-mode majority authority (co-signed recovery vote, revocation of non-signers); TLA, table, cross-check, vectors, spec, policy, UX note updated; new pass config and golden trap |
| F-15 | Major (evidence) | V-005 harness could not run on the current tree after R16C (W3 types); R16A's 23 `om.critical.` vectors were never regenerated and its Rust change never compiled | ADR-0168: V-005 tools scoped to W0; corpus regenerated (758); two-codec and CDDL agreement re-established |
| F-16 | Major (wire) | R16C widened frozen W0 validity: `sig_role` 7 became acceptable on every W0 object | ADR-0168: `enum_value_waves`, W0 `sig-role = 1..6`, `w3-sig-role` for Publication envelopes, W0 decoders drop later-wave values |
| F-17 | Minor (spec) | Canonical `geo:` token boundaries existed only in a harness regex, not in normative text | MESSENGER_BASELINE_V1 now defines the token grammar and boundaries; the clean-room implementation agrees |

The F-14 analysis also checked the opposite direction. A thief without the recovery method never reaches a majority
(new `thief_freeze` pass config, table vector `frozen_thief_alone_cannot_revoke_owner`). An attacker with a device and
the recovery method already had normal-mode majority, so the amendment adds no attacker power.

## Per-gate disposition

| Gate | Disposition | Basis |
|---|---|---|
| V-002 | **accept** after ADR-0168 | Authorization table, TLA and cross-check agree on the amended freeze rule; the F-10 selective-restoration shape is kept (restored controllers must sign); all ADR-0137/0147/0154/0161 criteria re-verified; TLC runs clean |
| V-003 | **accept** | K12 initiator binding enforced before any secret-key work (route-aware path, V-001 F-9 re-review); sealed-delivery RESERVED/replay and bootstrap models pass; ADR-0161 floor semantics evidenced |
| V-005 | **accept** (independent implementation) | Rust (Claude) and Node (OpenAI) strict codecs agree on 758/758 W0 vectors; clean-room Rust agrees on all 64 extension vectors and bytes, so criteria 8–11 no longer rest on one author's pair; F-15 and F-16 fixed |
| V-015 | **accept** | F-12 resolved: routine provider administration uses resource-scoped capabilities (ADR-0138), so `stable_device_correlator` is `no_by_design` for discovery/recovery/blob roles and `always` for key-material and push; ADR-0150/0158 LAN privacy models pass |
| V-016 | **accept** | F-13 resolved: nontrivial blinding-factor reuse, `r=1`, direct signing, PSS-zero and forced-CSPRNG-reuse mutations are each caught; room-moderation TLC passes; provider negative capabilities hold |
| V-017 | **accept** | Continuity manifest (84 docs) and offline bundle checks pass; registry immutability passes against the canonical trunk checkpoint |

No regression of F-1..F-13 was found: each has an executable negative or mutation in the re-run evidence above.

## Result

Every `core_kernel` pre-engineering criterion is met, V-017 is satisfied, cross-wave feasibility is evidenced and no
design item is open. `core_kernel` is eligible for an explicit scoped unlock ADR.
