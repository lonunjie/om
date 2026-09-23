# 2026-09-24 — OM architecture hardening review

- **Input:** `design-drops/om-master-planning-design-20260923-0920-PHT.zip`
- **Output:** `design-drops/om-master-planning-design-20260924-0229-PHT.zip`
  (SHA-256 `7e2bbd29370a21230108693c04226c6e19fcd107006b7325824a426fb49cd5d6`)
- **Canonical findings register inside the drop:** `HARDENING_REVIEW_20260924.md`
- **Status:** fixes applied in the drop. Engineering is still not authorized: 0/11 scopes, 21 gates open.

## Evidence

- `python3 tools/validate_architecture.py` → OK, 340 current files, `design_open=0`, `verification_open=21`, `scopes_authorized=0/11`. Validated from a fresh extraction of the zip.
- `python3 verification/run_phase1.py` → clean, 9,392 bounded cases (7,772 prior + 1,620 new ContinuityAssessment cases). The runner is deterministic: re-running leaves the manifest valid.
- Negative tests of the new validator guards on a throwaway copy. Each of these seeded faults was caught: an authorized scope with unmet gates, an authorized scope without an ADR, a backward wave dependency, an untraced requirement, an invalid trace target, and a PQ-for-groups overclaim.
- TLC could not be run here: no Java runtime is installed, only the macOS `java` stub. The toolchain is not vendored, and I did not download it unattended.

## Findings (summary; full table in the drop)

| # | Severity | Weak spot | Fix |
|---|---|---|---|
| F-01 | Critical | V-013/14/15/17 need real implementations, but all gates had to pass before engineering, so the lock could never honestly open | Two-phase gates plus scope-granular unlock (ADR-0108/0109) |
| F-06 | Critical | Every user needed their own Cloudflare account. That fails the Wi-Fi test and makes one vendor a practical 0M choke point | Home tiers; zero-signup sponsored starter mailbox from invites; second non-cloud reference provider (ADR-0102, V-020, PR-013) |
| F-02 | High | All-or-nothing unlock; LEAN rules contradicted MESSENGER_BASELINE_V1 on V1 scope | Delivery waves W0..W4 |
| F-03 | High | Gates had no pass criteria, and the same agent authored both the design and its "verification" | Per-phase criteria and independence levels |
| F-07 | High | For PWA users, the hosting-account password was effectively the E2EE root | R-164 controls |
| F-08/09 | High | No coercive/proximate adversary (silent device linking, seizure); OS/browser leakage (backups, snapshots, keyboards, cloud spellcheck, extensions) | ADR-0103, V-021 |
| F-10 | High | Recovery off plus majority rule means 1-device users lose identity and 2-device users lock the constitution, silently | ContinuityAssessment, informed-off (ADR-0106) |
| F-11 | High | Profiles promised PQ-hybrid to everyone, but v1 groups are classical | Per-type PQ honesty, ReInit path, objective triggers (ADR-0107, V-019) |
| F-12 | High | No vulnerability response for a kill-switch-free, multi-implementation protocol | Advisories plus peer-local floors (ADR-0104, PR-012) |
| F-16 | High | The Wi-Fi usability test was never verified | V-018 comprehension study |
| F-04/05 | Medium | No requirement traceability; threat model was an append-log | 182/182 traced; T-001..T-040 register, validator-enforced |
| F-13 | Medium | No account close/delete/moved/deceased path | AccountRetirement plus minimal LegacyPolicy (ADR-0105) |
| F-14 | Medium | Illegal-content posture on public surfaces was implicit | PR-014; no scanning of private E2EE |
| F-15 | Low-med | Phrase-presence guards; unsigned manifest | Structural guards added; signing recommended |
| F-17 | Low | Drops as zips outside git lose diffs | Recommendation only |

## Operator decisions requested

These were applied as accepted ADRs because they fit the constitution best. Each can be overruled with a superseding ADR:

1. The sponsored starter mailbox is the default first-run path from an invite (ADR-0102).
2. Recovery stays off by default but is informed-off, with a persistent non-blocking continuity check (ADR-0106).
3. Wave order: W0 1:1 kernel → W1 groups/media/export → W2 calls → W3 communities/publications → W4 bots/plugins/bridges (ADR-0108).
4. The second reference provider is a single static binary + SQLite that runs on a VPS, home server or single-board computer (V-020).

## Further suggestions for reaching OM's goals

1. **Grow through hosted Homes, not only personal clouds.** Families, clubs and organizations running one T1 Home for their members is the proven adoption pattern in federated/independent messengers. It pairs with the sponsored mailbox as the viral loop.
2. **Ship W0 early and narrow.** W0 is 1:1, multi-device, attachments and an excellent onboarding experience. Resist adding more architecture passes: the design queue is closed, and the value now is in meeting W0 pre-engineering criteria.
3. **Get outside eyes on the crypto and authority core before W0 unlock.** V-001 and V-002 need independent review. Publishing the W0 spec for public comment, and later a bug bounty, supplies the independence the gates now require. Grant programs for open internet-freedom tooling may fund the audit; verify current eligibility.
4. **Recruit at least one co-steward** before the first public release. R-083 already records honestly that S0 custody is founder-only. This is also the practical bus-factor fix.
5. **Put a JDK and pinned TLC in verification CI** so V-002 can actually run.
6. **Make performance a gate.** "Fast" is in the mission. Add measurable pre-release SLOs (cold start, send-to-delivered latency, idle battery drain) to the `reference_client_ui` gates.
7. **Track design checkpoints in git** instead of zips. Commit each unzipped drop to a non-frozen `design/` tree in this private repo, so checkpoints become reviewable diffs across the agents that edit them. This needs an operator decision because `AGENTS.md` currently keeps drops out of git until freeze.

## Applied

- 2026-09-24: all fixes applied in drop `om-master-planning-design-20260924-0229-PHT.zip` (not committed to git per `AGENTS.md` design-drop rule; this report is the git-tracked record).
