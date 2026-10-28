# Beta gate readiness: what a labelled public DEV/BETA of OM still owes

- **Date:** 2026-10-28 (repo state `6f2fb02`, read-only review; nothing was built or run)
- **Question:** which release gates bind a labelled public beta of the OM web client at `https://om.lonunjie.org`, what is true in the repo today, and what only people can supply.
- **Sources:** [`machine/public_release_gates.jsonl`](../../machine/public_release_gates.jsonl) (PR-001..PR-014), V-006 and V-018 in [`machine/architecture_verification_gates.jsonl`](../../machine/architecture_verification_gates.jsonl), [`SECURITY_RELEASE_GATES.md`](../process/SECURITY_RELEASE_GATES.md), ADR-0130, ADR-0170, ADR-0172, ADR-0180, ADR-0182.
- **Rule used:** a gate or criterion is called met only when I verified it in the repo. No gate is fully met today; all fourteen PR gates and both V gates are `open`.

## Bottom line

1. **No independent human cryptographic review has happened, and one is required before any public security claim.** The reviews so far are single-vendor AI sessions (ADR-0167 model; the Core audit of 2026-10-28 is one). A beta must carry no security claim, and its banner must say so.
2. **V-018 (participant study) and V-006 pre-release (real devices) explicitly block "any public or beta reference-client release"** (ADR-0170, ADR-0172). V-006 is a half-day of operator device work (kit: [`2026-10-28-v006-device-checks.md`](2026-10-28-v006-device-checks.md)). V-018 needs at least 12 people in two languages (kit: [`2026-10-28-v018-participant-study-kit.md`](2026-10-28-v018-participant-study-kit.md)), and 9 of its 16 study items cannot run on any current build (2 more only in part).
3. **Of the PR gates, only PR-001, PR-005, PR-006 and PR-012 bind the beta as built, plus PR-014 for the hosted Home if it counts as a provider-readable surface.** PR-002, 003, 004, 007, 008, 009, 010, 011 and 013 bind only if the beta makes the claim they guard; the beta simply must not make those claims.
4. **The machine check is not yet armed.** [`machine/repository_contract.json`](../../machine/repository_contract.json) has `public_releases: []`. The validator refuses a release declared `authorized`/`published` while an applicable PR gate is neither `passed` nor `waived_by_adr` ([`tools/validate_architecture.py`](../../tools/validate_architecture.py), ADR-0130 rule 7). So the honest path to shipping with open gates is an ADR per waived gate, then declaring the release. V-018/V-006 are not covered by that check; they block by ADR prose, so waiving them also needs an ADR that amends ADR-0170/0172.

## Assumed shape of the beta

| Thing shipped | `distribution_kind` | Why |
|---|---|---|
| Signed web bundle served at `om.lonunjie.org` | `project_pwa_origin` | R-260 makes a project-controlled PWA origin a declared project service under PR-001/005/012 |
| Deploy kit and the "Deploy to Cloudflare" copy (`deploy/`) | `project_deployment_template` | R-261 applies to every template |
| Reference Home at `om-home-lonunjie.majal.workers.dev` | `project_service` | SeedLife runs it |

Scopes shipped: `core_kernel`, `provider_reference`, `reference_client_ui`. PR-006 lists `reference_client_ui` in its `applies_to.scopes`, and the validator counts a shared scope as a hit, so **PR-006 binds this beta even if it claims no background delivery** (the way out is a waiver, see PR-006).

## Summary table

| Gate | Binds the beta? | Status today | Agent can finish | Needs people |
|---|---|---|---|---|
| PR-001 jurisdiction review | Yes (all three kinds) | Not started | Role/data inventory, draft notice, counsel brief | Counsel per jurisdiction |
| PR-002 RF patent text | Only if normative contributions are invited | Not started | Draft | Counsel |
| PR-003 trademark policy | Only if "official" status is claimed | Not started | Draft | Counsel |
| PR-004 store compliance | No (no store listing) | n/a | n/a | n/a |
| PR-005 supply chain | Yes (PWA origin, template) | Partly | Docs, provenance, rebuild procedure, release-digest log | Independent reviewer, mirror/monitor parties, Ledger ceremony for new roles |
| PR-006 push matrix | Yes by scope | Not started | Little | Devices, a push-capable Home |
| PR-007 link previews | No if unclaimed | No link-preview code found | n/a | n/a |
| PR-008 LAN matrix | No if unclaimed | n/a | n/a | n/a |
| PR-009 provider diversity | No if unclaimed | n/a | n/a | n/a |
| PR-010 evidence export | No if unclaimed | n/a | n/a | n/a |
| PR-011 key transparency | No if unclaimed | n/a | n/a | n/a |
| PR-012 disclosure program | Yes (all three kinds) | Partly (SECURITY.md exists, stale) | Most of it | Independent reviewer; root ceremony for the advisory key |
| PR-013 provider monoculture | No if no resilience claim | Second Home not built | n/a for beta | n/a |
| PR-014 illegal-content process | Likely (hosted Home) | Not started | Draft process | Counsel |
| V-006 pre-release | Yes (ADR-0172) | Probe passed on 5 browsers; real-client and Windows runs absent | Kit, matrix doc | Operator devices; reviewer for the nested wrap |
| V-018 pre-release | Yes (ADR-0170) | Not started; no results exist | Kit, builds, translations | 12+ participants, facilitator |

## PR gates

### PR-001 `jurisdiction_role_review`

- **Requirement:** before any project-controlled public distribution or hosted service, role-specific legal/privacy/compliance review per jurisdiction where it is offered; no country is a protocol-global gate.
- **Applies:** kinds `project_distribution`, `project_app_store_distribution`, `project_pwa_origin`, `project_service`, `project_deployment_template`. Binds all three beta artifacts.
- **Criteria:** (1) role-specific review for each jurisdiction where the concrete distribution or service is offered; (2) no country becomes a protocol-global gate.
- **Status:** not started. I found design-level boundary text ([`LEGAL_AND_OPERATIONAL_BOUNDARIES.md`](../constitution/LEGAL_AND_OPERATIONAL_BOUNDARIES.md), [`METADATA_THREAT_MODEL.md`](../threat-model/METADATA_THREAT_MODEL.md)), no role-specific review record and no privacy notice for the beta.
- **Agent:** list the roles SeedLife plays in the beta (ClientHost operator, Home operator), what each can observe (from [`2026-10-28-relay-threat-model.md`](2026-10-28-relay-threat-model.md) and [`METADATA_OBSERVER_MATRIX.md`](../architecture/providers/METADATA_OBSERVER_MATRIX.md)), a plain-language privacy notice draft, and a counsel brief.
- **Humans:** counsel in each jurisdiction the beta is offered in. An agent cannot supply this.
- **Minimal beta approach:** operator names the jurisdictions (own jurisdiction plus any deliberately targeted), limits the beta to invite-only testers there, and either gets counsel review or waives by ADR.

### PR-002 `rf_patent_text`

- **Applies:** claim `external_normative_contributions` only. Binds only if the beta invites normative protocol contributions.
- **Criteria:** counsel-reviewed royalty-free essential-patent disclosure/license/non-assert language; mixed-license packaging guidance.
- **Status:** not started ([`LICENSING_AND_CONTRIBUTION_POLICY.md`](../constitution/LICENSING_AND_CONTRIBUTION_POLICY.md) section 6 promises it; [`CONTRIBUTING.md`](../../CONTRIBUTING.md) says implementation contributions are not yet solicited).
- **Minimal approach:** do not invite normative protocol contributions during the beta. Say so in CONTRIBUTING. Counsel text stays future work.

### PR-003 `trademark_compatibility`

- **Applies:** claim `official_compatibility`. Binds only if the beta asserts official compatibility or trademark status at scale.
- **Status:** not started; no standalone policy file (policy principles exist in LICENSING section 7).
- **Agent:** draft a policy that blocks impersonation and permits truthful "OM-compatible". **Humans:** counsel.
- **Minimal approach:** no "official/certified" claims; name the beta "OM (beta) by SeedLife".

### PR-004 `distribution_store_compliance`

- **Applies:** `project_app_store_distribution` only. No store listing exists. Not applicable to the beta.

### PR-005 `software_supply_chain`

- **Applies:** kinds `project_distribution`, `project_app_store_distribution`, `project_pwa_origin`, `project_deployment_template`. Binds the PWA origin and the deploy kit. Independence required: `independent_reviewer`, `independent_tool`.
- **Per criterion (verified in repo):**

| # | Criterion | Today |
|---|---|---|
| 1 | TUF-style root/targets/snapshot/timestamp, expiry, rollback/freeze protection | **Partly.** Pinned roots, root-signed delegation with expiry (v1 expires 2027-01-26), monotonic bundle and delegation version floors, rollback only by a signed marker, a PQ floor that only rises: `apps/web/src/update/{release-key,delegation,floor,pq-root}.ts`, [`deploy/KEY-CEREMONY.md`](../../deploy/KEY-CEREMONY.md) ("minimal TUF-style form"). Not separate targets/snapshot/timestamp roles; freeze protection is the delegation expiry only. Whether this counts is the independent reviewer's call. |
| 2 | S0 custody: separated 2-of-3 shares, one custodian, stated as equal to 1-of-1 against coercion | **Partly.** Stated in [`PROJECT_STEWARDSHIP_AND_CONTINUITY.md`](../constitution/PROJECT_STEWARDSHIP_AND_CONTINUITY.md) and [`SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md`](../architecture/providers/SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md); not in `deploy/` docs or any user-facing page. Pins: 2 Ledger roots plus 1 software root, and the ML-DSA-65 co-signature is required (ADR-0182). Physical separation is an operator fact I cannot check. |
| 3 | Routine signing and SecurityAdvisory role delegated from root, with rotation | **Partly.** Online key delegated, with a rotation and revocation runbook ([`KEY-CEREMONY.md`](../../deploy/KEY-CEREMONY.md) sections 7-9). The delegation carries no advisory role; see PR-012. |
| 4 | SLSA/in-toto provenance for every public artifact, plus a rebuild/compare procedure | **Partly.** [`apps/web/build/check-reproducible.mjs`](../../apps/web/build/check-reproducible.mjs) (`pnpm repro`) builds twice and requires identical manifests; I did not run it. No provenance attestation is generated anywhere in `apps/web/build` or `.github/workflows` (only `verify.yml`, on checkpoint tags). No third-party rebuild/compare document. |
| 5 | Independent source and release mirrors | **Not met.** One remote, `github.com/lonunjie/om`, currently private. |
| 6 | No path activates code that is not a verified, digest-pinned release (R-261) | **Partly.** [`deploy/site-stage.mjs`](../../deploy/site-stage.mjs) runs `verify.mjs --require-signed` and refuses `main`; the service worker rejects bytes that differ from the manifest; the kit READMEs make `verify.mjs` the first step. `--dev` builds exist and are marked dev. No published digest to pin against. |
| 7 | Release-transparency log publication for every release (R-274) | **Not met.** Nothing publishes release digests. `deploy/ledger` is a hardware-wallet signing helper, not a log. Consumer-side "two independent observations" has no implementation. |

- **Agent can do:** a rebuild/compare procedure and a CI release workflow with provenance (an in-toto statement signed by the delegated key, no new primitive); a release-digest log as a signed append-only file in a public repo (honestly labelled single-operator, not independent); custody wording in `deploy/` and the landing; SBOM generation.
- **Needs people:** an independent reviewer and tool pass (the gate says so); at least two independent monitor parties for R-274 (none exist during S0); second-forge accounts for mirrors (operator account creation); a new delegation carrying an advisory role needs two root signatures (Ledgers).
- **Minimal beta approach:** ship the signed, reproducible bundle with the verification chain that exists, publish the custody statement, add provenance and a digest log if time allows, and **record criteria 5 and 7 as open under a PR-005 waiver ADR**. The beta text must not claim transparency, mirrors or independent monitoring.

### PR-006 `push_platform_matrix`

- **Applies:** scope `reference_client_ui`, claim `background_delivery`. Binds by scope.
- **Criteria:** opaque push/VAPID lifecycle validated on the supported platform matrix; background behaviour documented per platform.
- **Status:** not started. A push capability adapter exists (`packages/om-web-platform/src/platform.ts`), but I found no VAPID or push endpoint in `infra/cloudflare-home`, so the lifecycle cannot be exercised end to end.
- **Agent:** nothing meaningful until a Home push endpoint exists. **Humans:** real devices (the V-006 kit records push capability only).
- **Minimal approach:** claim no background delivery; the beta says messages arrive while OM is open; waive PR-006 by ADR scoped to "no `background_delivery` claim".

### PR-007, PR-008, PR-009, PR-010, PR-011 (claim-gated)

| Gate | Guarded claim | Status | Beta action |
|---|---|---|---|
| PR-007 link previews (`project_service`) | `link_previews` | No link-preview code in `apps`, `packages`, `infra`, `crates`; the renderer turns unsafe links off | Do not offer previews |
| PR-008 LAN matrix | `lan_local_mode` | Client reports "nearby devices are not found automatically" | Do not advertise a LAN mode |
| PR-009 diversity evidence | `provider_diversity` | n/a | Do not claim provider diversity |
| PR-010 evidence export | `evidence_export_interop` | n/a | Do not claim interoperable export |
| PR-011 key transparency | `key_transparency` | n/a | Do not advertise transparency |

### PR-012 `vulnerability_disclosure_program`

- **Applies:** same five kinds as PR-001. Binds all three beta artifacts. Independence: `independent_reviewer`.

| Criterion | Today |
|---|---|
| Published security contact (security.txt/SECURITY.md) | **Partly.** [`SECURITY.md`](../../SECURITY.md) exists (contact `hi@lonunjie.org`, 7-day acknowledgement) but says "there is no shipped software yet": stale for a beta. No `security.txt` in `apps/web/public` or anywhere in the repo. |
| Signed advisory channel, key delegated from the release root, expiry on every advisory (R-263/R-264) | **Not met.** Schema exists ([`spec/protocol/security-advisory.logical.schema.json`](../../spec/protocol/security-advisory.logical.schema.json)); no feed, no advisory role in [`apps/web/release-delegation.json`](../../apps/web/release-delegation.json). |
| Embargo/coordination policy | **Partly.** Reference policy (90 days, signed advisory at disclosure) in [`VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md`](../architecture/providers/VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md) section 4; not stated in SECURITY.md. |
| Voluntary implementer contact list | **Not met.** Described, no list file. |

- **Agent can do:** rewrite SECURITY.md for the beta (scope, contact, response target, embargo, safe-harbour wording for counsel to review), draft `security.txt` (RFC 9116, with `Expires`), create the implementer list file, draft the advisory feed format. Placing `security.txt` under `apps/web/public` belongs to the in-flight web agent.
- **Needs people:** the advisory signing role requires a new root-signed delegation (two Ledgers); an independent reviewer for the gate.
- **Minimal approach:** do everything except the signed channel before launch; for the channel, either run the delegation ceremony once (also needed for renewal, [`KEY-CEREMONY.md`](../../deploy/KEY-CEREMONY.md) section 8) or list it in the waiver ADR with an interim of git-signed advisories.

### PR-013 `provider_monoculture_limit`

- **Applies:** claims `public_production_resilience`, `censorship_resilience`. Binds only on those claims (R-269).
- **Status:** not met: the only Home is the Cloudflare one (`infra/` holds only `cloudflare-home`); relay status D12 states PR-013 needs the second Home. The handoff already says the second Home is not needed for beta.
- **Beta action:** no resilience wording anywhere. Operator to re-read the landing line "no central service that has to stay online" against R-269 before launch (my reading is that it describes the design, but it sits close to resilience wording).

### PR-014 `public_surface_illegal_content_process`

- **Applies:** `project_service`. Binds if the hosted Home is a "public or provider-readable surface". The Home runs a mailbox, key pool, descriptor store and IntroGate (relay status); the descriptor store is the directory-like part. Whether it counts is a counsel/operator reading, so treat it as binding.
- **Criteria:** role-scoped intake, takedown, preservation and referral process; no client-side scanning of private E2EE (R-181).
- **Status:** not started. The scanning half holds on inspection (the only scanner in the client is the camera QR scanner, `apps/web/src/qr/Scanner.tsx`; I found no content-scanning code).
- **Agent:** draft the process and an abuse contact. **Humans:** counsel per jurisdiction.
- **Minimal approach:** publish an abuse contact and a short procedure drafted for counsel; or include in the waiver ADR.

## Verification gates

### V-006 `web_device_vault_feasibility` (pre-release criteria)

- **Requirement:** prove the WebAuthn-PRF/non-extractable-WebCrypto DeviceVault hierarchy and migration on the first PWA's browser matrix. Independence (pre-release): `independent_tool`.
- **Pre-engineering is `passed`** from real-device probe runs on 2026-10-01 (macOS Chrome 154/Safari 27.0.1/Firefox 157, iPhone Safari 26.6.1, Android 16 Chrome 154), recorded in [`verification/prototypes/v006-web-vault/README.md`](../../verification/prototypes/v006-web-vault/README.md). That evidence is from the **probe page, not the shipped client**.
- **Pre-release criteria and status:**

| Criterion | Today |
|---|---|
| Published support matrix with regression tests on each supported browser | **Partly.** Findings are in the probe README and [`PLATFORM_CAPABILITY_MATRIX.md`](../architecture/client/PLATFORM_CAPABILITY_MATRIX.md) (documented, not measured); Playwright e2e runs the client on Chromium/Firefox/WebKit engines (relay status: 19 of 22 pass on 2026-10-28). No published matrix for the shipped client; Windows Chrome/Edge untested; desktop engines are not real iOS/Android. |
| Passphrase layer (ADR-0180): Argon2id unlock time on a low-end Android and an older iPhone, calibrated parameters recorded, nested wrap read by a cross-vendor or human reviewer | **Not met.** Code uses the default m=64 MiB, t=3, p=1 (`packages/om-web-platform/src/vault/crypto.ts`); no device timings; the only review is single-vendor. |
| Camera QR scan, PRF passkey path and service worker verified on macOS Chrome/Safari/Firefox, iOS Safari, Android Chrome | **Not met on the real client.** |

- **Agent:** the checklist ([`2026-10-28-v006-device-checks.md`](2026-10-28-v006-device-checks.md)), then fold returned results into a published support matrix and ADR-0180 parameter record.
- **Humans:** the operator's iPhone, Android phone and (if the beta advertises Windows) a Windows PC; a human or cross-vendor reviewer for the nested wrap.
- **Minimal approach:** run the checklist on iPhone Safari, Android Chrome and the Mac; state in the beta that Windows is unsupported/untested unless run.

### V-018 `security_ux_comprehension` (pre-release criteria)

- **Requirement:** non-technical users complete and correctly understand the security-critical flows. Independence: `participant_study`. `criteria_met` is empty; no study results exist (`verification/results` has no v018 file).
- **Criteria (all seven open):** moderated sessions with at least 12 non-technical participants (3+ aged 60+, 3+ low digital literacy, 2+ languages); the listed flows; at least 80% task success and comprehension per flow with failures redesigned and retested; three added service flows; theme-compiler test (ADR-0129 rule 17); sponsored-invite origin choice (R-260); inviter-visibility and co-located-home comprehension (R-265/R-267). Pre-release also adds: repeat on the real client, plus assistive-technology users.
- **Client coverage (details in the kit):** of 16 study items, 1 runs on the real client (onboarding from an invite), 4 run only on the dev harness (recovery on, recovery veto notice, device-link notice, suspend device), 2 are partly built (contact continuity change: the notice component exists but no build can trigger it; inviter and co-located-home disclosure: only the observer line exists), and 9 are not built (onboarding T1 host choice, turning recovery off, continuity check, bridge boundary, full-Room service disclosure, representing-service counterparty notice, Connected services, trigger-filter service disclosure, app-origin chooser). The theme-compiler glyph test is not built either. The client has English copy only (`apps/web/src/copy/en.ts`), so the two-language requirement needs a translated copy module.
- **Agent:** study kit; building the missing flows behind the harness flag (ADR-0170 requires this); translation drafts for review by native speakers.
- **Humans:** recruitment, an independent facilitator (the protocol treats it as gate evidence), raters, native-speaker translators, assistive-technology users, participant payment.
- **Minimal approach:** there is none that satisfies the gate; it is people-hours. Realistic options are in the decisions below.

## Related blockers outside this report's list

- **V-021 pre-release forensic audit** on each shipped platform (ADR-0172 lists it beside V-018 as blocking any public or beta release); independence `external_review`.
- **Independent cryptographic review** for the first public secure release ([`SECURITY_RELEASE_GATES.md`](../process/SECURITY_RELEASE_GATES.md), Cryptography section). OpenAI-authored ADR-0157..0165 need a non-OpenAI reviewer and ADR-0166 a non-Anthropic one ([`AGENTS.md`](../../AGENTS.md)).
- **Beta label inside the app.** The only "Pre-release" wording is on the landing (`apps/web/src/copy/en.ts`, `landingCopy`), shown when the device has no identity, and it still says the version "does not send real messages yet", which the deployed Home and relay e2e contradict. A persistent in-app banner with the operator's wording ("not independently audited; no security claims; may lose data") does not exist. This is software-only work in `apps/web`.
- **Not checked by me:** the live site and Pages configuration, the deployed Home's WAF/rate rules, whether the Ledger and vault custody is physically separated, and whether `pnpm repro` currently passes.

## Decisions only the operator can make

1. **Ship with documented open gates (waiver) or block.** Waivers are an ADR per gate (`waived_by_adr`), authored under your authority; no agent can grant one. My recommendation: do not waive gates whose work is software-only (PR-012 except the signed channel, the PR-005 documentation and provenance parts, the in-app banner); waive with an explicit list only PR-001 (if counsel is not engaged), PR-005 criteria 5 and 7, PR-006, PR-014, V-018 and V-021, and call the release an invite-only tester beta rather than public.
2. **V-018: run a study round first, or waive.** Because 9 of 16 items are unbuilt, a full pass is months away. Choose: (a) waive for a labelled tester beta and schedule round 1 on the harness within a set time, or (b) hold the beta until the flows are built and round 1 passes. Also decide how many fresh participants a retest needs (the criteria do not say; the kit proposes 12).
3. **Jurisdictions and counsel (PR-001, PR-014).** Which jurisdictions the beta is offered in, and whether to engage counsel before launch.
4. **Who is the audience.** Invite-only testers (small, shrinks PR-001 and PR-014 exposure) versus an open public beta.
5. **Windows in or out** of the supported matrix (V-006).
6. **Independent crypto review:** whether and when to commission one, and the budget. Until then no security claim is made.
7. **Mirrors and transparency (PR-005):** open second-forge accounts; accept a single-operator digest log as interim; decide on any new external dependency (a public transparency service), since new dependencies are yours to approve.
8. **Advisory signing role:** schedule a Ledger ceremony for a delegation v2 that adds it (can be combined with a renewal).
9. **Declare the release** in `public_releases` as `planned` now, and `authorized` only after the waiver ADRs exist.
10. **Banner and landing wording,** including the stale "does not send real messages yet" line.
