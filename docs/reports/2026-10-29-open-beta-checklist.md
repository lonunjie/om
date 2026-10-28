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
- [ ] B1 (Haiku, low) Rewrite [`SECURITY.md`](../../SECURITY.md): scope, contact `hi@lonunjie.org`, 7-day acknowledgement, 90-day embargo, interim advisories as signed git tags, beta wording per ADR-0183 section 6. Pass: `make check`.
- [ ] B2 (Haiku, low) `security.txt` (RFC 9116, `Expires` <= 1 year) in `apps/web/public/.well-known/`, plus implementer contact list file. Pass: file served by the build; `pnpm test` targeted.
- [ ] B3 (Haiku, low) Custody statement (S0: 2-of-3 shares, one custodian, equal to 1-of-1 against coercion) into `deploy/` README and the site's Security page.
- [ ] B4 (Sonnet, medium) Provenance: in-toto/SLSA-style statement per release signed by the delegated key; signed single-operator digest log (labelled as such); SBOM; rebuild/compare doc. Pass: `pnpm repro` plus a verify script.
- [ ] B5 (Haiku, low) Privacy notice and abuse contact with the short draft procedure (what the Home and ClientHost can observe: use `relay-threat-model` and the observer matrix). Pass: `make check`.
- [ ] B6 When B1 to B5 land: set PR-005 and PR-012 to `waived_by_adr` with `waiver: ADR-0183`, flip `om-open-beta-1` to `authorized`, run `make check`.

### C. Landing page and brand (operator reviews the result)
- [ ] C1 (Sonnet, medium, main session) Replace the alarming Beta block in `apps/web/src/copy/en.ts` (`landingCopy`, around line 647) with a small "Beta" tag on the wordmark; move detail to a Security & privacy page. Remove the stale "does not send real messages yet" line.
- [ ] C2 (Sonnet, medium, main session) Rebuild the landing as a scrollytelling page: scroll-driven animation (CSS `animation-timeline: scroll()/view()` with a no-JS fallback), parallax layers, pinned sections that tell the story of "Oh Messenger"; honour `prefers-reduced-motion`; keep the PWA/install behaviour and the CSP. Open the page with what **OM** means ("Oh Messenger") and the "Protect life. Seed it." line; reuse the operator-liked OM description wording. Pass: build, CSP test, Lighthouse perf not worse, 390px and 1440px screenshots.
- [ ] C3 (Haiku, low) AI-slop wording sweep over all user-facing copy (`en.ts`, README, SECURITY.md, site): remove filler like "seamless", "empower", "unlock", "robust", em-dash chains, rule-of-three padding. Pass: grep list is empty; operator reads the diff.
- [ ] C4 Name check: did we miss the meaning of OM elsewhere? Grep `OM` in `<title>`, manifest `name`/`description`, README, social bios in `registry/socials.toml`; each should say "OM (Oh Messenger)" at least once. Record misses here.
- [ ] C5 Operator reviews the preview at `https://beta-preview.om-lonunjie.pages.dev` after a non-`main` push; then approves the `main` push.

### D. "Do what can be done" from the not-done list
- [ ] D1 (Sonnet, medium) Audit leftovers M3 (seeds as hex in the profile record), H2 follow-ups, L1/L2. Source: [`docs/reports/2026-10-28-core-kernel-opus-audit.md`](2026-10-28-core-kernel-opus-audit.md) (Applied section lists what is open). Pass: `cargo test --workspace`, `make core-check`.
- [ ] D2 (Sonnet, medium) "Run your own copy" link from deployed copies back to the official site. Pass: targeted test + deploy kit README.
- [ ] D3 (Haiku, low) Stage the Deploy-to-Cloudflare `release/` folder in `lonunjie/om.lonunjie.org` via the cutover tool ([`deploy/site-stage.mjs`](../../deploy/site-stage.mjs)), and write the button URL into the README. Push to a non-`main` branch only. Pass: `verify.mjs --require-signed`.
- [ ] D4 (operator, 10 min) Click the Deploy button once from the preview branch and tell the agent whether it worked. I (the agent) cannot test that flow.
- [ ] D5 Publish: GitHub release with the signed zip. Operator approves; needs B6 and C5 first.
- [ ] D6 Device link, recovery/multi-device, per-conversation records, signed-prekey rotation: **not beta-blocking**. Open as separate work packages (each its own session, Sonnet medium) after launch; the client keeps rejecting them with `Rejected(unsupported)`.
- [ ] D7 PQ epoch Phase C: same, post-launch.
- [ ] D8 Relay WAF rules: needs a `cf-mint` token with WAF permission; operator grants, then a Haiku task applies the rules from the relay threat model.

### E. Needs the operator
- [ ] E1 Rename already done (personal-vault dated field names). Close the item.
- [ ] E2 Approve the second Opus audit **after** B6, C5, D1 to D3 are done, scoped as in ADR-0183 section 7. One line from the operator is enough.
- [ ] E3 Later: V-006 half-day device run (kit: [`2026-10-28-v006-device-checks.md`](2026-10-28-v006-device-checks.md)), V-018 round 1 on the harness, counsel (see below), independent human crypto review.

## Q&A for the operator (answers to the 2026-10-29 questions)

- **Anthropic legal:** it is not a built-in skill. It is the Anthropic **Legal** plugin (contract review, compliance check, risk assessment), with sibling plugins `privacy-legal` (PIA, DPA review), `ip-legal` (trademark clearance, OSS review) and `ai-governance-legal`, all in the Anthropic plugin directory and not enabled here. They help draft and triage; they are not a lawyer and cannot satisfy PR-001. ADR-0183 waives legal gates, so nothing waits on them. Useful later: privacy notice and role analysis (B5), OSS licence review.
- **What is the Deploy button?** Cloudflare's "Deploy to Cloudflare" button in a README: one click makes a visitor's own copy of OM (their own Pages site and Home) from the public repo. That is why the source needs a public repo; `lonunjie/om.lonunjie.org` is approved.
- **Did multiple agents audit?** Yes, several Anthropic-model passes, including one Opus pass that found B1/B2/H1/H2. They are all one vendor and AI-written code reviewing AI-written code, so "not independently audited" is the accurate phrase. It goes in the small print, not a banner.
- **Why another Opus audit?** The first covered the Core up to `b6e32da`. A second only pays off once the beta candidate is code-complete (provenance, signing changes, M3/H2 fixes), scoped to what changed since. It is deferred; not needed now.
- **Counsel later:** triggers are in ADR-0183 section 2.
