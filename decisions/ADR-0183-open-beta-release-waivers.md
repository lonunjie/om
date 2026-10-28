# ADR-0183 — Open beta: which release gates are waived, which are not, and when each comes back

- **Status:** Accepted by operator delegation 2026-10-29 (operator: "decide on waivers for the not so necessary parts in development and I approve on those waivers")
- **Date:** 2026-10-29
- **Amends (release gating only, no wire or crypto change):** [`ADR-0130`](ADR-0130-distribution-provisioning-and-provider-trust-roots.md) rule 7 (records `waived_by_adr` for PR-001, PR-006, PR-014), [`ADR-0170`](ADR-0170-w0-provider-and-client-review.md) and [`ADR-0172`](ADR-0172-authorize-reference-client-ui-engineering.md) (V-018, V-021 and the real-device part of V-006 no longer block a beta).
- **Does not amend:** any protocol, crypto or key-custody rule; [`SECURITY_RELEASE_GATES.md`](../docs/process/SECURITY_RELEASE_GATES.md) for a **non-beta** first public secure release.
- **Evidence base:** [`2026-10-28-beta-gate-readiness`](../docs/reports/2026-10-28-beta-gate-readiness.md).

## Context

All fourteen PR gates are open. Waiting for every one (counsel in each jurisdiction, a 12-person two-language study, a forensic audit on every platform, two independent transparency monitors) would mean nothing ships and nothing is learned from real use. The operator's direction: label the product **beta** (the way a mature mail service carried the label for years), open it to anyone, make no security claims, and bring the heavy gates back when the product is mature and actively used.

The beta is a **label in the branding**, not a warning wall. It carries no security claim, so no claim-gated gate (PR-002, 003, 007, 008, 009, 010, 011, 013) binds it.

## Decision

### 1. Release shape

One planned release, `om-open-beta-1`: scopes `core_kernel`, `provider_reference`, `reference_client_ui`; kinds `project_pwa_origin`, `project_service`, `project_deployment_template`; claims: **none**. Audience: anyone (open beta). Recorded as `planned` in [`machine/repository_contract.json`](../machine/repository_contract.json); it becomes `authorized` only when section 4 is done.

### 2. Waived now (status `waived_by_adr`)

| Gate | What is waived | Why this is acceptable for a labelled beta | Returns when |
|---|---|---|---|
| PR-001 | Counsel review per jurisdiction | Counsel is not engaged; the beta stores only what E2EE requires and the Home operator role is documented in a privacy notice (section 3). Legal AI tooling may draft the role analysis but is **not** counsel and does not satisfy the gate. | Before the beta drops the label, **or** at ~1,000 monthly active accounts, **or** any paid feature, **or** a legal request, whichever is first |
| PR-006 | Push/VAPID platform matrix | No `background_delivery` claim is made; the product says messages arrive while OM is open | Before any background-delivery claim |
| PR-014 | Illegal-content intake/takedown process | Counsel-dependent; an abuse contact and a short draft procedure still ship (section 3) | Same triggers as PR-001, and before any directory, catalog or public blob host launches |

### 3. Waived in part (status stays `open` until the named software-only work lands, then flips to `waived_by_adr` in the same commit as that work)

| Gate | Waived | Still required before `om-open-beta-1` is `authorized` |
|---|---|---|
| PR-005 | Criterion 5 (independent source/release mirrors), criterion 7 (release-transparency log with independent monitors) | Custody statement in `deploy/` and the site; rebuild/compare procedure; in-toto/SLSA-style provenance statement per release signed by the delegated key; a single-operator signed digest log, **labelled as single-operator**; SBOM. No wording claims transparency, mirrors or independent monitoring |
| PR-012 | The signed advisory channel (needs a Ledger ceremony; interim: advisories as signed git tags in the public repo) and the independent-reviewer requirement | Rewritten [`SECURITY.md`](../SECURITY.md), `security.txt` with `Expires`, embargo policy, implementer contact list file, abuse/security contact. `security.txt` ships with the web client |

Also required, software only, not waivable: the in-app beta label (a quiet "Beta" tag on the wordmark, no blocking banner), a privacy notice stating what the Home and ClientHost can observe, an abuse contact with the draft procedure, and the corrected landing copy.

### 4. Authorization condition

`om-open-beta-1` is flipped `planned` -> `authorized` by the agent that completes the section 3 list, after `make check` passes with the gates recorded as above and the operator approves the visible landing page. Production push of the website stays operator-approved (AGENTS.md cadence).

### 5. Verification gates

- **V-018** (participant study): waived for the beta. Harness-only flows stay behind the dev flag. **Round 1 on the harness is scheduled, not blocking.** Returns before any "easy for non-technical people" claim or the first non-beta release.
- **V-021** (forensic audit per shipped platform): waived for the beta. The product makes no claim about residue on lost or seized devices. Returns before any such claim or non-beta release.
- **V-006** (pre-release real-client runs): the real-device run (iPhone Safari, Android Chrome, Mac) stays a half-day operator task, **not blocking**. Until it runs, the published support list says: macOS Chrome/Safari/Firefox supported (engine-tested); iOS and Android best-effort; Windows untested. Argon2id cost calibration stays open.

### 6. Wording rules (replace the earlier alarming banner)

- "Beta" is a label: a small tag by the wordmark and in the page title, as for any beta product.
- The detail lives on a **Security & privacy** page and in [`SECURITY.md`](../SECURITY.md): no human cryptographer has audited it; no security claims are made; data may be lost during the beta.
- The product never claims: audited, secure against X, resilient, censorship-proof, transparent, or private from the Home operator beyond what is documented. "End-to-end encrypted" is allowed only as a description of the design.

### 7. What "audited" means here

Several Anthropic-model review passes exist (including one Opus pass on the Core). They are single-vendor AI reviews of AI-written code (ADR-0167): useful, not independent. A second Opus pass is **deferred** until the beta candidate is complete; it must be scoped to code changed since `b6e32da` and the release signing path, and the operator approves it at that time.

## Consequences

- Open gates stay visible in [`machine/public_release_gates.jsonl`](../machine/public_release_gates.jsonl) (`waived_by_adr` carries the ADR id), so nothing is quietly forgotten.
- Every waiver has a return trigger; [`docs/reports/2026-10-29-open-beta-checklist.md`](../docs/reports/2026-10-29-open-beta-checklist.md) tracks them.
