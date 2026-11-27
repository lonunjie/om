# ADR-0276 — Resolution for the engineering host conformance review

- **Status:** Delegated resolution (additive). Independent review remains required.
- **Date:** 2026-11-27
- **Scope:** The optional engineering host and its Core projections. The audited design and wire objects remain authoritative.

## Resolution

1. **Room offers and membership.** Require the current verified Room observation for bridge offers and admission. Keep reported approvers as review information. Require a Core-validated basis for protected changes. The engineering API refuses changes that lack that basis. Native reducer fixtures can exercise admission with explicitly supplied test facts. Retire removed bridge incarnations and superseded review targets for the lifetime of the Room reducer.
2. **Runtime time and reductions.** Use the scoped instance's monotonic time for Room observations and freshness. A repeated signed head keeps its original observation time. Forward verified endpoint reductions to the scoped membership reducers, including explicit revocations on an accepted branch. The reducers record pending endpoint removals.
3. **Profile and reader boundaries.** Require a current profile context for stateful scoped commands. Clear scoped state when its profile closes or opens with a new generation. Show Room control metadata and private Space child links only to verified members of the relevant audience. A verified Space-side reference may be shown as unilateral. Official affiliation and authority binding require separately verified child acceptance.
4. **Pending changes and invocation.** Reconcile a change only under its exact pending identity. Return a typed outcome when a pending-store or view read fails. Bind the invocation verdict to the service, command, destination, expiry and ordered disclosure items, with one explicit trigger. The scoped broker defaults to denial without verified grants. Invocation review has no delivery path in the engineering host.
5. **Review and history projections.** Include the exact external scope in bridge review and retain the separately labelled external claim and reported approvers after admission. Reject bridge network and scope values whose exact text cannot be displayed safely. Show the service trigger filter with the full-member access terms. Core labels retention when it has the original message basis. Projection exports omit individual items with unclassified retention and state that original message evidence is absent.
6. **Release boundary.** Check both the application and service-worker bundle graphs and exercise rejection of a transitive scope import in an automated build.

End-to-end process-restart recovery for Room mutations needs a durable reducer operation record and an engineering host entry that can open it. Both remain open for review.
