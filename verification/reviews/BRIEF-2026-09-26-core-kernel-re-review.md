# Review brief: core_kernel re-review of the ADR-0136 remediation

You are the **independent reviewer** under ADR-0117 and ADR-0134, re-reviewing the fixes for your own findings.
Your 2026-09-26 review [`2026-09-26-gpt-5-6-sol-core-kernel.md`](2026-09-26-gpt-5-6-sol-core-kernel.md) rejected all
seven queue items with findings F-1..F-9. The executor (Claude Opus 5.5) accepted every finding and applied it
([ADR-0136](../../decisions/ADR-0136-core-kernel-review-remediation.md)). Each finding now has a filled `Resolution:`
line naming its evidence. You did not author any of the changed artifacts, so you remain independent of them.

Read [`AGENTS.md`](../../AGENTS.md), [`SNAPSHOT.md`](../../SNAPSHOT.md), ADR-0136 and your review file first.

## Re-review queue (answer every item)

| # | Finding(s) | Changed artifacts to check | Question |
|---|---|---|---|
| 1 | F-1, F-2 (V-002) | [`AccountControl.tla`](../tla/AccountControl.tla) + new configs in [`models.json`](../tla/models.json), [`acs_tla_crosscheck.py`](../bounded/acs_tla_crosscheck.py), [`acs_authorization_table.py`](../bounded/acs_authorization_table.py) | Does the model now encode one-vote recovery with a single-controller veto, device-only lift, the §4 rule 4 freeze and its resolution? Is counting quarantined controllers' **challenge signatures** toward freeze resolution (ADR-0136 rule 2) sound, or does it reintroduce an authority leak? Is the trace-replay cross-check adequate? |
| 2 | F-3 (V-002) | [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) §4 rule 2, §4.1 lift row, §5 | Is the rule now evaluable from parent/child state alone, with no hidden provenance? |
| 3 | F-4 (V-003) | [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../../docs/architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md) §3 (K12), [`OneToOneSessions.tla`](../tla/OneToOneSessions.tla) (`BindInitiator`, `ForgeInitiation`, S6), [`tlc_runs.py`](../prototypes/v003-sessions/tlc_runs.py) | Is K12 normative and sufficient at the wire-to-DeviceID boundary: route pinning, the stale-head refresh, and "first contact is never unbound PQXDH"? |
| 4 | F-9 (reopens V-001) | ADR-0135 §7, `verification/prototypes/v001-crypto/rust/core/src/{binding,store,otk,vectors}.rs`, `python/check_vectors.py` (`verify_initiator`), vector set `initiator_binding` | Does the responder now bind `IK_A` before any secret-key operation? Are the negatives complete (unknown, route mismatch, revoked, quarantined, stale, substituted, ambiguous, crossed)? Is the composed authentication argument in ADR-0135 §7 acceptable? |
| 5 | F-5 (V-015) | [`w0-metadata-exposure-descriptors.json`](../../spec/metadata/w0-metadata-exposure-descriptors.json), [`metadata_w0_rules.py`](../bounded/metadata_w0_rules.py) `device_correlator`, [`metadata_trace_model.py`](../bounded/metadata_trace_model.py), [`METADATA_THREAT_MODEL.md`](../../docs/threat-model/METADATA_THREAT_MODEL.md) §20 inv. 12 | The executor **extended** your finding: mailbox-class Homes see the sender's `EncodeEC(IK)` on every envelope, and push providers see a push token. Are these claims right, and complete? Is A-054 (sealed sender, non-blocking) an acceptable disposition for W0? |
| 6 | F-6, F-7 (V-016) | [`abuse_w0_rules.py`](../bounded/abuse_w0_rules.py), [`privacy_pass_issuance.py`](../bounded/privacy_pass_issuance.py), [`privacy-pass-blind-rsa-vectors.json`](../vectors/privacy-pass-blind-rsa-vectors.json), [`INVITES_INTROGATE_AND_SPAM.md`](../../docs/architecture/discovery/INVITES_INTROGATE_AND_SPAM.md) W0 profile | Is type 0x0002 now standards-conforming (salt 48, SPKI key id)? **Open question for you:** does the issuer-transcript test satisfy F-7, and what does W0 issuance unlinkability protect, and from whom? In OM the recipient is the issuer and also picks the IntroGate. |
| 7 | F-8 (V-017) | [`continuity_bundle.py`](../../tools/continuity_bundle.py), [`om-crypto-v1-bundle-manifest.json`](../../spec/continuity/om-crypto-v1-bundle-manifest.json), [`CONTINUITY_TABLETOP_DRILLS.md`](../../docs/process/CONTINUITY_TABLETOP_DRILLS.md) D7 | Is the manifest complete, and do `verify` and `check-manifest` enforce it? |
| 8 | ADR-0136 as a whole | ADR-0136, ADR-0135 §7 | Where did authority go? Does any rule contradict ADR-0111/0127/0131, or can two honest implementations still read it differently? |

Every item maps to the `independent_reviewer` criterion of V-002/V-003/V-015/V-016/V-017, and to the new V-001
re-review criterion. Unchanged artifacts that you accepted before do not need re-review (ADR-0117).

## How to return results

- Add **only** new files under `verification/reviews/`: `2026-09-2X-<reviewer>-core-kernel-re-review.md`.
- Header: reviewer, exact model id, date, the base commit from `OM_EXPORT.json`, and scope.
- For each queue item 1–8: a verdict (`accept`, `accept-with-findings` or `reject`). New findings use
  `### F-<n> — <title>` numbered from F-10, with `Severity:` blocker|major|minor, `Where:`, `Problem:`,
  `Proposed fix:` and an empty `Resolution:` line.
- Return the whole repository as a zip with the same layout, leaving `OM_EXPORT.json` unchanged. Do not edit other
  files.
