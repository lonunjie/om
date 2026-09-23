# ADR-0106 — ContinuityAssessment and informed no-recovery

- **Status:** Accepted; default amended by [ADR-0111](ADR-0111-identity-recovery-on-by-default.md) (recovery on by default). The ContinuityAssessment stays; the continuity check now applies to pending or explicitly-off recovery.
- **Date:** 2026-09-24
- **Source:** [`HARDENING_REVIEW_20260924.md`](../docs/reviews/HARDENING_REVIEW_20260924.md)

## Decision

Recovery remains off by default (Q-003). Core computes a local, non-authoritative ContinuityAssessment: lost devices until identity loss, lost devices until constitutional lockout, compromised devices until takeover, and recovery status. Configurations where one lost device loses the identity or locks the constitution (the common one- and two-device cases) show a persistent, non-blocking continuity check after the first conversation until the user configures recovery, adds a controller, or explicitly acknowledges the risk. Changes that worsen the assessment to one device are explained before they are finalized (R-165). Evidence: [`verification/bounded/continuity_assessment.py`](../verification/bounded/continuity_assessment.py).
