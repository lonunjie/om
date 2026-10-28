# Reporting security issues

OM is in architecture verification, and a labelled **beta** web client exists (not independently audited; no security claims; may lose data). The most valuable reports right now are
**design flaws**: a way to break an invariant, gain authority you should not have, learn metadata the design claims
to hide, or defeat a stated threat mitigation.

- **Design flaws that are not yet exploitable anywhere** can be reported publicly as an issue. Open design review is
  how OM gets the independent scrutiny its verification gates require. Reference the document, section and, if you
  can, the requirement (`R-…`) or threat (`T-…`) id.
- **Anything affecting the beta, a deployed prototype, a real user or a third party** should be reported privately to
  `hi@lonunjie.org` with the subject `OM security`. Please include steps to reproduce. We will acknowledge within
  7 days.

How OM handles vulnerabilities once software exists is specified in
[`VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md`](docs/architecture/providers/VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md):
coordinated disclosure, signed advisories, and no remote kill switch.
