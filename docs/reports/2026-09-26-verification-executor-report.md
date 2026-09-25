# Verification executor report — core_kernel pass (2026-09-25/26)

- **Executor:** Claude Opus 5.5, `verification_executor` under ADR-0134 and [`CLAUDE_OPUS_5_5_HANDOFF.md`](../../verification/CLAUDE_OPUS_5_5_HANDOFF.md)
- **Base:** drop `om-design-20260925-1154-PHT-verification-handoff.zip` (imported as `checkpoint/20260925-1154`); work is in the commits after it
- **Result:** every repo-local criterion for `core_kernel` has passing evidence. **`core_kernel` is not yet eligible**
  for an unlock ADR. The only remaining blocker is one batched independent review, described in
  [`BRIEF-2026-09-26-core-kernel-independent-review.md`](../../verification/reviews/BRIEF-2026-09-26-core-kernel-independent-review.md).
  Engineering stays unauthorized.

## 1. Gate table (core_kernel scope)

| Gate | Pre-engineering | Criteria | Feasibility | Remaining |
|---|---|---|---|---|
| V-001 production crypto path | **passed** | 9/9 | — | pre-release only (external crypto review; R-072 release proof) |
| V-002 AccountControl model | open | 8/9 | — | independent review |
| V-003 1:1 session model | open | 4/5 | — | independent review |
| V-005 canonical encoding | **passed** | 6/6 | — | — |
| V-015 metadata model | open | 11/11 | — | independent review |
| V-016 abuse/moderation model | open | 9/9 | — | independent review |
| V-017 continuity drill | open | 3/3 | — | independent review |
| V-004 groups | open | — | **evidenced** 2/2 | (later wave) |
| V-008 Room/Space authority | open | — | **evidenced** | (later wave) |
| V-009 materialization | open | — | **evidenced** | (later wave) |
| V-011 service principals | open | — | **evidenced** | (later wave) |
| V-013 bridges | open | — | **evidenced** | (later wave) |
| V-014 archive | open | — | **evidenced** | (later wave) |
| V-019 PQ group ReInit | open | — | **evidenced** | (later wave) |

The generated view is [`GATE_PASS_CRITERIA.md`](../../verification/GATE_PASS_CRITERIA.md). The ledger is authoritative.

## 2. Evidence added per criterion

- **V-001** ([`v001-crypto/`](../../verification/prototypes/v001-crypto/README.md), [ADR-0135](../../decisions/ADR-0135-om-crypto-v1-construction-pinning.md)):
  - The prototype is now exactly OM-Crypto-v1: SPQR, spqr and libsignal are gone, and there is no copyleft or git
    dependency.
  - Composite Ed25519+ML-DSA-65 binding over the frozen ACS → DeviceCredential → AsyncKeyBundle chain.
  - Exact 4096-cap replay ledger, committed before plaintext is released (crash/reload, no eviction).
  - GCM-SIV OM-DR derivation.
  - draft-ietf-hpke-pq-05 HPKE: official KATs pass 33/33 in Rust (native/WASI/Node) and in an independent Python
    implementation.
  - 49 selftest checks, byte-identical on three runtimes. Python/OpenSSL reproduces 202/202 vector values, including
    12 negative binding vectors.
- **V-002:**
  - [`RecoveryObservation.tla`](../../verification/tla/RecoveryObservation.tla) (R-278) passes; the timestamp-trust mutation is caught; a golden late-verifier trace
    and the T-090 residual trace are recorded.
  - [`acs_authorization_table.py`](../../verification/bounded/acs_authorization_table.py) parses §4.1 and mirrors all 14 rows: 16,200 exhaustive cases and 48 vectors,
    covering R-201 and R-205.
- **V-003:**
  - [`restore_redelivery.py`](../../verification/prototypes/v003-sessions/restore_redelivery.py): ADR-0131 restore as a fresh DeviceID, with no delivery exemption; 13,378 states
    explored exhaustively.
  - [`floor.py`](../../verification/prototypes/v003-sessions/floor.py): 14 R-206 vectors from the real profile registry.
- **V-005:**
  - 677 vectors; the Rust and Node codecs have 0 mismatches and the two CDDL tools 0 disagreements.
  - Profile-1 HPKE sizes are pinned in the registry and enforced by both codecs. They include a MessageRequest
    sealed with the real suite.
  - RequestKey, ForkAck and R-237 attachment vectors.
  - 16 R-280 reciprocal-binding sequences, on which the independent Python and Node checkers agree.
- **V-015:** [`metadata_w0_rules.py`](../../verification/bounded/metadata_w0_rules.py) has 8 checks, each with a caught mutation, and reads real V-001/V-005 bytes. The
  descriptors gain `ratchet_header_shape`, `shared_blob_linkage` and `lan_observer`.
- **V-016:**
  - [`abuse_w0_rules.py`](../../verification/bounded/abuse_w0_rules.py): RFC 9578 type-2 tokens with real RSA-PSS, IntroGate admission order, stale incarnations,
    unsolicited adds, committed lifts, call gating, and advisory / safety-ref v2 injectivity.
  - Existing IntroGate and RoomModeration models are credited.
- **Feasibility:** [`cross_wave_feasibility.py`](../../verification/bounded/cross_wave_feasibility.py):
  - 13 later-wave features are carried by W0 envelopes and accepted by both codecs.
  - 4 group-only objects are accepted under a purely additive W1 registry overlay and rejected by the W0 codecs.
  - The V-004 partition sketch covers 32 cases.

## 3. Findings: no frozen-rule contradiction; four clarifications for the integrator

1. **Backup import must merge (V-003).** A fresh DeviceID can already be receiving when the backup import runs. An
   overwrite-style import loses an accepted message. The executable trace is the `RestoreImportMerges=False` mutation.
   Proposed rule: union the dedupe history, and take the max of floors and control heads.
2. **Registered extensions were not schema-checked (V-005).** Both codecs accepted malformed `om.acs.fork_ack`
   values. They now validate a registered extension's value and criticality. This was a codec bug, not a design one.
3. **1:1 "new event kind" route (EVENT_MODEL.md §RichTextV1).** W0 validators close event kinds at 1..14, so a later
   1:1 kind is rejected rather than shown as a placeholder. That route needs recipient capability gating, the same as
   reserved body encodings.
4. **§4.1 lift row.** "not counting quarantines the signer initiated" cannot be derived from a parent/child diff (the
   ACS does not record who initiated a quarantine), and it is redundant with rule 1. Reviewer question 1.

Documented residuals (not blockers):

- T-041: an offline owner loses a race to a custodian holder.
- T-090: a request withheld from every controller lets a partitioned peer accept after its own interval; fork
  detection on contact is the mitigation.
- History gap: messages sent only to the lost device between its backup and the restore.
- OM-Crypto-v1 ratchet headers are visible to mailbox Homes.

## 4. Consolidated independent-reviewer queue

The seven items in [`BRIEF-2026-09-26-core-kernel-independent-review.md`](../../verification/reviews/BRIEF-2026-09-26-core-kernel-independent-review.md):
V-002, V-003, V-015, V-016 and V-017 model review; ADR-0135; and the Claude-authored design since 2026-09-24 (this
also absorbs the two unfulfilled earlier briefs).

## 5. External or human only (not simulated)

- V-018 participant study.
- Every pre-release `external_review` (for example, V-001's cryptographic review).
- Real-device performance (V-022).
- Independent rebuilds.

## 6. core_kernel eligibility

Machine prerequisites are met except `pre_engineering_status=passed` on V-002, V-003, V-015, V-016 and V-017. Their
criteria are all met, but ADR-0117 requires a non-Anthropic reviewer. Once one accepts the queue (or the integrator
applies its findings), those five gates can be set to `passed`. `core_kernel` then becomes **eligible** for an unlock
ADR that the operator or integrator decides on; the executor does not self-authorize.

## Next executor pass (not started)

The later-scope gates whose environments are available: V-006, V-007, V-010, V-012 and V-020–V-023 (browser,
runtime and platform work).
