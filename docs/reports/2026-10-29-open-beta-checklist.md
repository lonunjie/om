# Open beta checklist and agent routing (read this first)

- **Date:** 2026-10-29. **Owner of the decisions:** operator. **Status file for the next agent:** this one; tick items here, commit with the work.
- **Goal:** ship `om-open-beta-1` (an open beta anyone can join; "Beta" is a label like early Gmail, not a warning wall) at https://om.lonunjie.org.
- **Decisions made 2026-10-29 (do not reopen):** open beta, not invite-only; waivers per [ADR-0183](../../decisions/ADR-0183-open-beta-release-waivers.md); no legal blocking now; the Deploy button source lives in `lonunjie/om.lonunjie.org` (approved); second Opus audit deferred until everything below is done, operator approves then; Bluesky/Facebook posts and the website `main` push stay operator-approved.
- **How to resume:** read this file, then ADR-0183. Do not re-read the large readiness report; section numbers below point to what you need. Run `make check` (om) before every commit.

## Token rules (the last run burned too many)

1. **Sequential by default.** One agent per task, one task per session. Parallel only for tasks that touch different files, at most 2 at once. Each parallel agent re-pays the whole context.
2. **Brief by file, not transcript.** A subagent gets: goal, exact files, the pass/fail command, "return `done:` or `blocked:`". No history.
3. **Never read** [`docs/reports/2026-10-28-beta-gate-readiness.md`](2026-10-28-beta-gate-readiness.md) whole (187 lines of context you do not need now); `grep` it. Never re-run the full e2e suite to check a copy change; run the targeted test.
4. **Commit per task** so the next session starts clean; tick the box here in the same commit.
5. Opus only on the operator's word (global rule). The main session should be Sonnet 5.5, medium effort.

## Model and effort per task type

| Task type | Model | Effort | Why |
|---|---|---|---|
| Copy changes, `security.txt`, SECURITY.md rewrite from ADR-0183, list files, registry sync, SBOM script, CI workflow from a spec, `make check` repair of mechanical failures | Haiku 5.5 | low | Bounded, pass/fail by command, context stays under 100k. Do not hand it design choices |
| Landing page build, web client UI, provenance/digest-log tooling, anything touching `apps/web` logic or `deploy/` | Sonnet 5.5 | medium | Judgement within an established design; tests exist |
| Landing page art direction and copy review (taste, AI-slop wording pass) | Sonnet 5.5 | medium, once, main session | Needs the operator's voice; do not delegate |
| Crypto, key custody, signing path, anything the second audit would cover | Sonnet 5.5 | high, main session only; Opus audit later with approval | Blast radius |
| Verifying a stronger model's work | Never a cheaper model | n/a | Global rule |

Escalate one way only: Haiku fails twice -> Sonnet; Sonnet blocked -> ask the operator about Opus.

## Checklist

### A. Done 2026-10-29
- [x] Worktrees cleaned (stale `loving-yonath` worktree and merged branch removed).
- [x] Waiver decisions recorded: ADR-0183; PR-001, PR-006, PR-014 `waived_by_adr`; V-018, V-021, V-006 carry `pre_release_waiver`; release `om-open-beta-1` is `planned`.
- [x] Operator answers recorded (see Q&A at the end).

### B. Software-only work that unblocks `authorized` (ADR-0183 section 3). Order = suggested order.
- [x] B1 (Haiku, low) Rewrite [`SECURITY.md`](../../SECURITY.md): scope, contact `hi@lonunjie.org`, 7-day acknowledgement, 90-day embargo, interim advisories as signed git tags, beta wording per ADR-0183 section 6. Pass: `make check`. Done 2026-10-29: SECURITY.md rewritten (beta state, scope, 7-day ack, 90-day embargo, interim signed tags, no bounty, draft safe harbour, links).
- [x] B2 (Haiku, low) `security.txt` (RFC 9116, `Expires` <= 1 year) in `apps/web/public/.well-known/`, plus implementer contact list file. Pass: file served by the build; `pnpm test` targeted. Done 2026-10-29: security.txt added (Expires 2027-09-29T00:00:00Z); IMPLEMENTER_CONTACTS.md added; _headers is global, so text/plain is the host default.
- [x] B3 (Haiku, low) Custody statement (S0: 2-of-3 shares, one custodian, equal to 1-of-1 against coercion) into `deploy/` README and the site's Security page. Done 2026-10-29: Custody paragraph already in deploy/README.md; sentence added to the site Security & privacy points in en.ts.
- [x] B4 (Sonnet, medium) Provenance: in-toto/SLSA-style statement per release signed by the delegated key; signed single-operator digest log (labelled as such); SBOM; rebuild/compare doc. Pass: `pnpm repro` plus a verify script. Shipped: [`provenance.mjs`](../../apps/web/build/provenance.mjs) (DSSE/in-toto/SLSA statement, `provenance.intoto.jsonl`, verified by `verify.mjs`; `--require-provenance` is opt-in), [`digest-log.mjs`](../../apps/web/build/digest-log.mjs) + `deploy/release-log/` (single-operator), [`sbom.mjs`](../../apps/web/build/sbom.mjs) (`sbom.cdx.json`), [`compare-rebuild.mjs`](../../apps/web/build/compare-rebuild.mjs) + [`deploy/REBUILD-AND-COMPARE.md`](../../deploy/REBUILD-AND-COMPARE.md), custody paragraph in [`deploy/README.md`](../../deploy/README.md).
- [x] B5 (Haiku, low) Privacy notice and abuse contact with the short draft procedure (what the Home and ClientHost can observe: use `relay-threat-model` and the observer matrix). Pass: `make check`. Done 2026-10-29: BETA_PRIVACY_NOTICE.md added (draft, not reviewed by counsel), linked from SECURITY.md.
- [x] B6 PR-005 and PR-012 set to `waived_by_adr` (ADR-0183 section 3 work all landed); `om-open-beta-1` is `authorized` in the repo contract. This authorizes nothing public by itself: the website `main` push, release publish and posts still need the operator.

### C. Landing page and brand (operator reviews the result)
- [x] C1 (Sonnet, medium, main session) Replace the alarming Beta block in `apps/web/src/copy/en.ts` (`landingCopy`, around line 647) with a small "Beta" tag on the wordmark; move detail to a Security & privacy page. Remove the stale "does not send real messages yet" line.
- [x] C2 (Sonnet, medium, main session) Rebuild the landing as a scrollytelling page: scroll-driven animation (CSS `animation-timeline: scroll()/view()` with a no-JS fallback), parallax layers, pinned sections that tell the story of "Oh Messenger"; honour `prefers-reduced-motion`; keep the PWA/install behaviour and the CSP. Open the page with what **OM** means ("Oh Messenger") and the "Protect life. Seed it." line; reuse the operator-liked OM description wording. Pass: build, CSP test, Lighthouse perf not worse, 390px and 1440px screenshots.
- [x] C3 (Haiku, low) AI-slop wording sweep over all user-facing copy (`en.ts`, README, SECURITY.md, site): remove filler like "seamless", "empower", "unlock", "robust", em-dash chains, rule-of-three padding. Pass: grep list is empty; operator reads the diff.
- [x] C4 (done 2026-10-29) Name check: did we miss the meaning of OM elsewhere? Grep `OM` in `<title>`, manifest `name`/`description`, README, social bios in `registry/socials.toml`; each should say "OM (Oh Messenger)" at least once. Record misses here.
  - C1/C2 done: `landingCopy` rewritten (small Beta tag, "OM stands for Oh Messenger" in the hero, pinned four-step story, parallax rings, in-page Security & privacy section at `#security`, stale line gone); CSS-only scroll animation with `@supports`/`prefers-reduced-motion` fallback to a static page. Screenshots were reviewed at 390 and 1440px (scratchpad only).
  - C3: grep sweep of README, SECURITY.md, `apps/web`, `packages` found nothing to change (the "unlock your data" strings are about the passphrase, not marketing).
  - C4 fixed: `<title>`, meta/og/twitter descriptions, `og:site_name`, manifest `name` and `description` now say "Oh Messenger". README already has it. **Misses left:** `registry/socials.toml` has no bios (nothing to change); social bios on X/Facebook/TikTok/YouTube and the GitHub repo description are not in the repo, so the operator or a later task checks them; `registry/repos.toml` purpose line for `om` is stale ("engineering not authorized"); README "Status" section is stale too; `om.lonunjie.org` website repo (static HTML) not checked.
- [ ] C5 Preview pushed 2026-10-29 (branch `beta-preview` of `lonunjie/om.lonunjie.org`, commit `0bc18c3`, signed bundle from cf2a5c4 incl. new landing, security.txt, copy notice, provenance): https://beta-preview.om-lonunjie.pages.dev . Awaiting operator review and approval of the `main` push. The bundle verifies under the pinned chain (2-of-3 Ed25519 + required ML-DSA-65 b44d).

### D. "Do what can be done" from the not-done list
- [x] D1 Dispositioned 2026-10-29: L1/L2/L7 stay as recorded in the audit report ("Not changed, with reasons"); H2 applied (ADR-0181). **M3 remainder** (seeds as hex in the profile record) needs a design for a vault-held record key inside Core: post-launch work package, Sonnet high in the main session, not a beta blocker (the beta makes no residue or key-handling claim).
- [x] D2 (Sonnet, medium) "Run your own copy" link from deployed copies back to the official site. Pass: targeted test + deploy kit README. Done 2026-10-29: runtime origin check (`apps/web/src/official.ts`, `CopyNotice`) in the landing footer and Settings menu; hidden on om.lonunjie.org, om-lonunjie.pages.dev (and previews) and localhost; copy in `copyNoticeCopy`; tests in `official.test.tsx`; note in [`deploy/README.md`](../../deploy/README.md). The prerendered landing has no notice (client-side only).
- [ ] D3 **Blocked:** a verified signed release needs the ML-DSA-65 co-signature (operator personal vault) and the Ledger delegation; an agent cannot produce one. Once a signed release exists, a Haiku task stages `release/` into `lonunjie/om.lonunjie.org` on a non-`main` branch and writes the button URL into the README. Pass: `verify.mjs --require-signed`.
- [ ] D4 (operator, 10 min) Click the Deploy button once from the preview branch and tell the agent whether it worked. I (the agent) cannot test that flow.
- [ ] D5 Publish: GitHub release with the signed zip. Operator approves; needs B6 and C5 first.
- [ ] D6 Device link, recovery/multi-device, per-conversation records, signed-prekey rotation: **not beta-blocking**. Open as separate work packages (each its own session, Sonnet medium) after launch; the client keeps rejecting them with `Rejected(unsupported)`.
- [ ] D7 PQ epoch Phase C: same, post-launch.
- [ ] D8 Relay WAF rules: needs a `cf-mint` token with WAF permission; operator grants, then a Haiku task applies the rules from the relay threat model.

### E. Needs the operator
- [ ] E1 Rename already done (personal-vault dated field names). Close the item.
- [ ] E2 Approve the second Opus audit **after** B6, C5, D1 to D3 are done, scoped as in ADR-0183 section 7. One line from the operator is enough.
- [ ] E4 PQ root: **keep `pq-root-b44d71873569f5f4`**. It is already pinned (`PQ_ROOT`, `PINNED_PQ_ROOT`), co-signs the committed delegation and is in `registry/signing.toml`; the ceremony was complete on 2026-10-28 and must NOT be rerun. A second root `pq-root-d2ea2ca94ba18d91` was generated by mistake on 2026-10-29 (an agent told the operator to "run the ceremony" without checking it was done). Operator: trash the vault item whose key id is d2ea (two items share the name "OM release PQ root (ML-DSA-65) 2026-10-28"); `~/om-root-work/pq-root.json` was restored to the b44d descriptor.
- [ ] E3 Later: V-006 half-day device run (kit: [`2026-10-28-v006-device-checks.md`](2026-10-28-v006-device-checks.md)), V-018 round 1 on the harness, counsel (see below), independent human crypto review.

## Q&A for the operator (answers to the 2026-10-29 questions)

- **Anthropic legal:** it is not a built-in skill. It is the Anthropic **Legal** plugin (contract review, compliance check, risk assessment), with sibling plugins `privacy-legal` (PIA, DPA review), `ip-legal` (trademark clearance, OSS review) and `ai-governance-legal`, all in the Anthropic plugin directory and not enabled here. They help draft and triage; they are not a lawyer and cannot satisfy PR-001. ADR-0183 waives legal gates, so nothing waits on them. Useful later: privacy notice and role analysis (B5), OSS licence review.
- **What is the Deploy button?** Cloudflare's "Deploy to Cloudflare" button in a README: one click makes a visitor's own copy of OM (their own Pages site and Home) from the public repo. That is why the source needs a public repo; `lonunjie/om.lonunjie.org` is approved.
- **Did multiple agents audit?** Yes, and my first answer wrongly named only yesterday's passes. There were also heavy OpenAI/GPT design and architecture reviews earlier (run 10a, ADRs 0157-0165 remediation), then the Anthropic Core passes of 2026-10-28 incl. one Opus pass. All are AI reviews (ADR-0167); none is an independent human cryptographer, so the small-print phrase "not independently audited" stays accurate.
- **Why another Opus audit?** The first covered the Core up to `b6e32da`. A second only pays off once the beta candidate is code-complete (provenance, signing changes, M3/H2 fixes), scoped to what changed since. It is deferred; not needed now.
- **Counsel later:** triggers are in ADR-0183 section 2.
