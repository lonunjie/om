# OM Architecture Hardening Review — 2026-09-24

- **Status:** Canonical review record for checkpoint 2026-09-24 02:29 PHT
- **Input:** checkpoint 2026-09-23 09:20 PHT (frozen under `history/20260923-0920/`)
- **Reviewer:** independent agent pass (Claude), operator-directed: find gaps, blind spots and weak spots before any code exists, then apply the fixes
- **Engineering:** still **not authorized**

## 1. Verdict

The 09:20 architecture is unusually strong on authority separation, crypto agility, metadata honesty and anti-capture. Its weak spots were not in the protocol core. They were in five places:

1. **Process logic.** The engineering lock could never open (§3, F-01).
2. **Onboarding reality.** The v1 premise asked every user to own a Cloudflare account. That fails OM's own "can't configure Wi-Fi" test and turns one cloud vendor into a practical choke point, which contradicts 0M (F-06).
3. **Endpoint adversaries.** Physically present and coercive attackers, and OS/browser leakage channels, were missing from the threat model (F-08, F-09).
4. **Honesty gaps.** The privacy profiles claimed PQ-hybrid protection for everyone, but v1 groups are classical (F-11). The recovery-off default also made silent identity loss the likely outcome for users with one or two devices (F-10).
5. **Operational lifecycles.** OM had no design for coordinated vulnerability response, for closing or retiring an account, or for handling illegal content on public surfaces (F-12..F-14).

Each finding below lists the fix applied in this checkpoint. None of the fixes promotes a verification gate to `passed`, and none authorizes engineering.

## 2. What held up under review (no change needed)

- Separating AccountControlState from the message plane, the quarantine-denominator rule, and "no LWW for authority".
- Opaque scoped relay capabilities, per-provider randomized envelopes and the surface-specific metadata claims.
- CryptoProfiles as immutable, reviewed bundles; "no emergency master key".
- Typed principals (Account/Service/Bridge), selected-context default for bots, and bridges that terminate E2EE explicitly.
- "Evidence is not gate passage" and the machine contract/validator discipline.

## 3. Findings register

| ID | Severity | Finding | Fix in this checkpoint |
|---|---|---|---|
| F-01 | Critical (process) | **Circular engineering lock.** V-013, V-014, V-015, V-017 (and parts of V-009/V-012) require real adapters, two independent implementations, real packet traces or an independent rebuild. Those require engineering, and engineering required every gate to pass first. The lock could never open honestly. The likely outcome was an eventual quiet waiver. | ADR-0108/0109: every gate is split into `pre_engineering` and `pre_release` criteria. Engineering scopes unlock on pre-engineering criteria only. Public release still needs full gate passage. See `RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`. |
| F-02 | High | **All-or-nothing unlock plus a scope contradiction.** One boolean held back all engineering, so plugin-sandbox or bridge proofs blocked the 1:1 messaging kernel. Separately, `LEAN_ARCHITECTURE_RULES.md` deferred public channels, bots and voice/video, while `MESSENGER_BASELINE_V1.md` listed Publications and calls as V1. | Delivery waves W0..W4 and named `engineering_scopes` in the contract, all `authorized=false`. V1 is now read as a semantic contract, delivered in waves. LEAN is reconciled. |
| F-03 | High | **Gates had no pass criteria and no independence rule.** The same agent that wrote the design also wrote the bounded models that "check" it. | Every V-gate now carries `pre_engineering_criteria`, `pre_release_criteria`, `independence` and `unlock_scopes`, and the validator enforces them. Gates cannot pass on same-author evidence alone. |
| F-04 | Medium-high | **No traceability.** 160 requirements had no machine link to the gate, test or review that proves them. | `machine/requirement_traceability.json` maps every requirement to a gate, release gate, validator check, conformance obligation or named review. The validator fails on any untraced requirement. |
| F-05 | Medium | **The threat model was an append-log.** It had no IDs, no adversary→mitigation→gate mapping, and missing adversary classes. | `machine/threat_register.jsonl` (T-001..) has references the validator checks. `threat-model/README.md` now points to it. |
| F-06 | Critical (product + 0M) | **Every user needed their own Cloudflare account.** That means account signup, email verification and a deploy flow before the first message. It fails R-010 and the Wi-Fi test. If most homes sit on one vendor, a vendor-wide block of the OM template takes down most of the network. That is exactly the "indispensable universal permission boundary" that R-145 forbids. | ADR-0102 and `HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md` add Home tiers. A **Sponsored starter mailbox** from the inviter's home lets a new user chat with zero signup. Community, household and organization homes, own-cloud and self-host follow. Monoculture is a declared 0M risk. A second independently implemented reference provider is required before public-production claims (V-020, PR-013). |
| F-07 | High | **For PWA users, compromising the hosting account compromises the endpoint.** Whoever controls the Cloudflare account that serves the ClientHost origin can ship malicious JavaScript. The hosting account password was therefore the real root of E2EE trust for PWA users, and this was undeclared. | R-164 and the endpoint doc require least-privilege deploy credentials and phishing-resistant auth on hosting accounts where available. Independent release-hash observation and honest UI are added. Native remains the higher-assurance option. |
| F-08 | High | **No coercive or proximate adversary.** Device seizure, compelled unlock, border search, intimate-partner abuse, stalkerware via silently linked devices, and shared family devices were all absent. | ADR-0103 and `ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md` cover these. Device linking is always announced on the account's other devices (R-166). Quick lock and per-profile lock are added. OM explicitly does not promise hidden-volume deniability (R-167). |
| F-09 | High | **Platform leakage channels were undeclared.** OS/cloud device backups, app-switcher snapshots, screenshots, IME/keyboard learning, browser cloud spellcheck, browser extensions with site access, and OS notification history each leak plaintext around E2EE. | LocalDataClass is mapped to platform controls (R-168). V-021 checks a platform matrix. |
| F-10 | High | **Recovery was off by default and the majority rule applied.** One device plus no recovery means a lost phone loses the identity. Two controllers plus no recovery means one lost device locks the constitution permanently. Those are the most common real configurations. | ADR-0106: Core computes a **ContinuityAssessment** (how many lost devices cause lockout, how many compromised devices cause takeover). "Recovery off" becomes informed-off: onboarding stays non-blocking, and a persistent continuity check appears after first use. Executable evidence: `verification/bounded/continuity_assessment.py`. |
| F-11 | High | **PQ overclaim.** The profiles promised "strongest approved E2EE/PQ-hybrid" to all users, but v1 MLS groups use the classical X25519 suite. Group content is therefore exposed to harvest-now-decrypt-later, and there was no objective trigger for migration. | ADR-0107 makes PQ status per conversation type (R-173). Objective adoption triggers are defined, and the v1 group wire must carry a tested ReInit path to a hybrid-PQ suite (R-174, V-019). |
| F-12 | High | **No decentralized vulnerability response.** OM has independent implementations, no kill switch and no central authority, but no design for coordinated disclosure, embargoes, advisories or floor raises. | ADR-0104 and `VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md` add signed SecurityAdvisories (inputs, not authority) and peer-local security floors. PR-012 requires a disclosure program before first public release. |
| F-13 | Medium | **No account end-of-life design.** OM could not close an account, point to a successor, purge the relay, or handle a deceased user without an operator. | ADR-0105 and `ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md` add AccountRetirement as a constitutional transition and an optional LegacyPolicy with minimal power. V-002 is extended to cover them. |
| F-14 | Medium | **Illegal-content posture on public surfaces was implicit.** Private E2EE had a clear "no client-side scanning" stance. Public Publications, directories, catalogs and public blob hosts did not. | ABUSE doc §"public-surface illegal content" plus PR-014, with role-scoped processes for project-controlled public surfaces. No scanning of private E2EE in reference clients (R-181). |
| F-15 | Low-medium | **Weak drift controls.** Validator guards mostly check for phrase presence, and the checkpoint manifest is an unsigned integrity list. | New validator checks are structural: gate schema, scopes, traceability and threat references. Signing checkpoints (e.g. `ssh-keygen -Y sign` or minisign over `MANIFEST.json`) is recommended to the operator. |
| F-16 | High (mission) | **The usability test was never verified.** The mission rests on "normal users never need crypto vocabulary", but no gate checked comprehension of the security-critical flows. | V-018 requires comprehension testing with non-technical participants on non-production prototypes (`design/prototype` qualifies). |
| F-17 | Low (workflow) | **Design drops live as zips outside git.** Drops are authored in a separate agent environment and land as zips, so diffs and blame are lost and concurrent agents can fork the design. | Recommendation only (the operator workspace rule keeps drops out of git until freeze): track a non-frozen `design/` tree in the private `om` repo so each checkpoint is a reviewable diff. |

## 4. Fixes applied (index)

| Artifact | Purpose |
|---|---|
| `RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`, ADR-0108 | F-01, F-02 |
| `verification/GATE_PASS_CRITERIA.md`, ADR-0109, gate ledger fields | F-01, F-03 |
| `machine/requirement_traceability.json` | F-04 |
| `machine/threat_register.jsonl` | F-05 |
| `HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`, ADR-0102, `protocol/sponsored-mailbox-grant.logical.schema.json` | F-06, F-07 |
| `ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`, ADR-0103 | F-07, F-08, F-09 |
| `ACCOUNT_CONTINUITY_AND_CONTROL.md` §11, ADR-0106, `verification/bounded/continuity_assessment.py` | F-10 |
| `CRYPTOGRAPHY_AND_AGILITY.md` §PQ groups, `SECURITY_PRIVACY_PROFILES.md` correction, ADR-0107 | F-11 |
| `VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md`, ADR-0104, `protocol/security-advisory.logical.schema.json` | F-12 |
| `ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md`, ADR-0105, `protocol/account-retirement.logical.schema.json` | F-13 |
| `ABUSE_SAFETY_AND_REPORTING.md` addendum, PR-014 | F-14 |
| `tools/validate_architecture.py` 09-24 guards | F-03, F-04, F-05, F-15 |
| V-018..V-021, PR-012..PR-014, R-161..R-181, D-124..D-131, A-016..A-022 | machine truth |

## 5. Items for the operator to confirm

These were resolved in the direction that best fits OM's constitution. They are product or governance choices the operator may still want to overrule with an ADR:

1. **Sponsored starter mailbox as the default first-run path** (ADR-0102). The alternatives are "own cloud first" (today's premise) or "community home first".
2. **Informed-off recovery** instead of recovery-on (ADR-0106). This keeps Q-003's "off by default" but adds a persistent, non-blocking continuity prompt.
3. **Wave boundaries** (ADR-0108), especially keeping calls in W2 and Publications in W3.
4. **The second reference provider target** (V-020). Suggested: a single static binary with SQLite on a VPS or home server, because it removes every cloud-vendor dependency.

## 6. Remaining open work (unchanged honesty)

- `design_open = 0` after A-016..A-022 were resolved in this pass.
- `verification_open = 21` (V-001..V-021). Every gate still needs its pre-engineering criteria met before its scope can be authorized by ADR.
- `engineering_authorized = false`; every `engineering_scopes.*.authorized = false`.
