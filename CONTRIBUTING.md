# Contributing to OM

**Current checkpoint:** architecture hardening / pre-engineering. Product implementation contributions are not yet being solicited as the next phase.

Read first:

1. [`PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`](docs/constitution/PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md)
2. [`ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md`](docs/process/ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md)
3. [`AGENTS.md`](AGENTS.md)
4. the relevant canonical architecture documents and machine records

## Contribution licensing

OM uses inbound-equals-outbound licensing. A contribution is submitted under the license already assigned to the file/component being changed.

Contributors retain their copyright. The normal path does not require assignment of copyright to SeedLife/lonunjie or a broad CLA granting unilateral proprietary relicensing rights.

Contributions use **Developer Certificate of Origin 1.1** sign-off. The authoritative DCO text is published at:

`https://developercertificate.org/`

Commits should include a `Signed-off-by:` line certifying the DCO.

## Normative protocol contributions

Before OM accepts broad external normative protocol contributions, the project will finalize lawyer-reviewed royalty-free patent disclosure/license language. Until then, potentially patent-encumbered normative mechanisms are architecture-blocking and must be flagged rather than merged casually.

## Architecture change discipline

A normative change requires the relevant canonical document, ADR, machine decision/requirement/open-question records, threat-model impact, interoperability/migration analysis and a clean `python3 tools/validate_architecture.py` run.
