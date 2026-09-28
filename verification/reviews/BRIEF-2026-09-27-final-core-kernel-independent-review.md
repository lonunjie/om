# Final `core_kernel` independent-review brief — ACTIVE (amended 2026-09-28 23:00)

> **Independent architecture review — 2026-09-28 23:00 PHT:** Claude Opus 5.5 reviewed the Run 10A drop as the different-vendor *design* reviewer ([`2026-09-28-independent-architecture-review-run10a.md`](../../docs/reports/2026-09-28-independent-architecture-review-run10a.md)). That review found no blocker and added **ADR-0154** (V-002 recovery-timer runtime segments), **ADR-0155** (V-005 non-critical extension validity) and **ADR-0156** (LAN binding moved to A-055/`local_delivery`/V-024; later-wave wire-freeze criteria). The ADR-0140–0153 chain is OpenAI-authored and ADR-0154–0156 are Anthropic-authored, so this brief now needs **both** a non-OpenAI and a non-Anthropic review of the respective artifacts (ADR-0117). A reviewer from neither vendor (for example Gemini, or a human) can satisfy both in one pass. The design review above is not this brief's formal gate sign-off; no independence record was marked satisfied. Engineering remains unauthorized.

> **Run 9B activation — 2026-09-28 21:22 PHT:** ADR-0152 repo-local platform evidence is current. Replaceable Android wake/privacy passes 13/13 scenarios and catches 8/8 message/authority/linkage mutations; desktop/PersonRecord endpoint safety passes 12/12 scenarios and catches 10/10 semantic/profile/export mutations. ADR-0153 clarifies that provider-side registration retirement is best-effort and no universal finite retention bound is claimed without an enforceable lease. V-015 is 16/16 repo-local and V-021 is 10/10 repo-local. Required independence and pre-release records remain open and are not self-approved. This brief is active again; engineering remains unauthorized.

> **Run 9A pause — 2026-09-28 20:42 PHT:** ADR-0152 promotes native desktop into the V1 reference family, standardizes replaceable Android wake adapters and local PersonRecord cleanup/export UX. It intentionally adds a new repo-local criterion to V-015 (15/16) and V-021 (9/10). Do **not** perform the consolidated independent review from this checkpoint; wait for Run 9B evidence and ledger reconciliation. Prior Run-8B evidence remains valid. Engineering remains unauthorized.

> **Run 8B activation — 2026-09-28 19:37 PHT:** ADR-0150 repo-local evidence is current. LAN/WAN reconciliation passes 8/8 scenarios and catches 6/6 route/identity/durability mutations; the local privacy/resource trace passes 9/9 scenarios and catches 6/6 mutations; `om.rich-list.v1` has 34 vectors with 34/34 Python/Node verdict+CBOR agreement; PWA/Android/iOS reference-family evidence passes 8/8 scenarios and catches 5/5 semantic-fork mutations. ADR-0151 also resolves the registered-schema-versus-fail-soft rich-list validation ambiguity. V-003 is 13/14, V-005 is 11/11 repo-local, V-015 is 15/15 repo-local and V-021 is 9/9 repo-local. Required independence records remain open and are not self-approved. This brief is active again for the consolidated different-vendor review. Engineering remains unauthorized.

> **Run 8A pause — 2026-09-28 19:10 PHT:** ADR-0150 promotes optional offline-local message/file delivery, registered bounded `om.rich-list.v1`, and Android/iOS as explicit V1 reference-family targets. It intentionally reopens one repo-local criterion each in V-003, V-005, V-015 and V-021. Do **not** perform the consolidated independent review from this checkpoint; wait for Run 8B to supply/reconcile the LAN route/privacy, rich-list codec/fallback and cross-platform capability evidence. Engineering remains unauthorized.

> **Run 7B reactivation — 2026-09-28 18:21 PHT:** ADR-0149 repo-local hardening evidence is current. V-003 sync hardening passes 8/8 scenarios and catches 6/6 shortcut mutations; V-005 now has a 24-vector amended extension corpus with 24/24 Python/Node agreement plus registry/object-scope checks and is 10/10 pre-engineering passed; V-021 is 8/8 repo-local; V-023 is 9/9 repo-local. This brief is active again for the consolidated `core_kernel` different-vendor review. V-021/V-023 retain their own `reference_client_ui` independent-review obligations and are not part of the core-kernel acceptance batch.

> **Run 7A pause — 2026-09-28 17:33 PHT:** ADR-0149 adversarially hardens the newly promoted V1 extension, account-private-sync, profile-execution and update-signature surfaces. It intentionally reopens one repo-local criterion each in V-003, V-005, V-021 and V-023. Do **not** perform the consolidated independent review from this checkpoint; wait for Run 7B to supply/reconcile those local evidence items. Engineering remains unauthorized.

> **Run 6B reactivation — 2026-09-28 17:18 PHT:** ADR-0148 repo-local evidence is current. V-003 account-private sync passes 6/6 bounded scenarios and catches 4/4 forbidden mutations; V-005 adds a 20-vector voice/location extension corpus with Python/Node agreement; V-021 is 7/7 repo-local; and V-023 is 8/8 repo-local. This brief is active again for the consolidated `core_kernel` different-vendor review. V-021/V-023 retain their own later `reference_client_ui` independent-review obligations and are not part of the core-kernel acceptance batch.

> **Run 5B activation — 2026-09-28 15:22 PHT:** ADR-0147 repo-local evidence is current. The recovery timer model passes 5/5 normative restart/restore scenarios and catches 2/2 forbidden acceleration mutations; the bootstrap lifecycle/restore model passes 5/5 scenarios and catches 4/4 mutations covering independent `request_expires`, ninth-after-prune, and stale-backup resurrection. This brief is active for the consolidated different-vendor review.

## Role and independence

Act as the **different-vendor independent reviewer** required by ADR-0117 for the remaining `core_kernel` pre-engineering review obligations. The remediation/design chain through ADR-0153 and the current focused harness repairs were authored in the OpenAI GPT/Sol chain. **Do not use another OpenAI model as the independent reviewer.** Claude Opus 5.5 (or another non-OpenAI reviewer) is appropriate.

This is one consolidated review. Do not bounce individual gates back and forth. Do not broadly redesign OM unless executable evidence demonstrates a concrete contradiction. Product engineering remains unauthorized.

## Starting state: what is actually left

The machine ledger is authoritative. At this checkpoint:

| Gate | Canonical pre-engineering criteria | Remaining pre-engineering blocker |
|---|---:|---|
| V-001 | passed | none for pre-engineering; pre-release obligations remain |
| V-002 | 9/10 | independent model review / phase-independence record only |
| V-003 | 13/14 | independent model review / phase-independence record only; ADR-0150 LAN/WAN reconciliation is current |
| V-005 | 11/11 | repo-local criteria complete; independent-implementation phase record remains open (do not self-approve ADR-0150/0151 rich-list evidence) |
| V-015 | 16/16 | repo-local criteria complete; independent-review phase record only |
| V-016 | 9/9 | phase independent-review record only |
| V-017 | 3/3 | phase independent-review record only |

Cross-wave feasibility needed by `core_kernel` is already recorded as evidenced for V-004, V-008, V-009, V-011, V-013, V-014 and V-019. Do not turn this review into full later-wave gate passage. V-021 is also 10/10 repo-local for the reference-client scope after Run 9B, but its independent review remains a separate reference-client obligation rather than part of the `core_kernel` acceptance batch.

## Reproducibility: use the right environment for each command

A raw design ZIP deliberately has no Git history and may not have Rust/TLC. Do **not** run the Git-history verification target in a raw extraction and treat the resulting environment failure as a repository failure.

### A. Fresh exported ZIP

From a fresh extraction of this checkpoint, with no `.git` required:

```sh
make preflight-drop
make verify-drop
```

`make verify-drop` must run the portable deterministic evidence profile, architecture validator, docs check and continuity-manifest check. It intentionally does **not** claim Rust/TLC execution or registry-history immutability.

### B. Tool-complete Git worktree

Import the checkpoint into the real repository/worktree with history, then run:

```sh
make preflight-full
make verify
```

`make verify` is the stronger path: full phase-1 evidence including the Rust-dependent aggregate, TLC, architecture/docs, continuity manifest and registry-history immutability. If the environment is missing a required pinned tool/dependency, report that limitation; **do not overwrite stronger committed evidence with an environment failure**.

For the two focused toolchain profiles, use the preflights first:

```sh
python3 tools/verify_preflight.py --profile v003
python3 tools/verify_preflight.py --profile v005
```

When available, rerun `verification/prototypes/v003-sessions/run.sh` in its pinned environment. The current Python reference machine must replay K12 `BindInitiator` / `ForgeInitiation` behavior under Hypothesis; do not accept a stale K1..K11-only run. Rerun V-005 Rust/CDDL/Wasm only if the current tool-complete worktree requires it or its stronger committed evidence cannot be validated.

Also confirm the focused provider administration trace still reports **8** routine traces + **3** RecoveryRequest fan-out traces, zero violations and **7/7** security mutations caught.

If a failure is clearly a verification-harness defect, fix the harness and rerun. If fixing it would change protocol/design semantics, record a bounded review finding instead of silently changing the architecture.

## Independent semantic review batch

Review the **current tree**, not only old review prose. In particular:

- **V-002 / F-10 — AccountControl selective freeze and recovery timing.** Review ADR-0137, the canonical authorization table, TLA semantics, [`acs_authorization_table.py`](../bounded/acs_authorization_table.py), [`acs_tla_crosscheck.py`](../bounded/acs_tla_crosscheck.py), vectors and current results. Confirm no alternate path accepts a set the canonical policy rejects or vice versa. Also include ADR-0141 RecoveryRecord freshness/advisory semantics and ADR-0147 crash-safe verifier-local timer evidence in [`recovery_durable_timer_adr0147.py`](../bounded/recovery_durable_timer_adr0147.py): downtime/wall time/restored historical timer state must not accelerate recovery-alone activation.
- **V-003 — one-to-one sessions + account-private sync + LAN/WAN reconciliation.** Review K12 initiator binding plus ADR-0143/0144/0145/0146/0147 crash-safe sealed-delivery replay/resume and bootstrap composition, ADR-0148 account-private synchronization boundaries, and ADR-0149 causal/revocation/restore anti-resurrection semantics. Confirm exact-wrapper `RESERVED` retry cannot become cross-wrapper replay acceptance; stale-but-refreshable peer ACS state remains `RESERVED` rather than terminal `REJECTED`; plaintext is not released before durable inner acceptance + `COMPLETE`; `reply_delivery_key` is fresh/request/profile/route/expiry scoped and never promoted into stable authority; exactly one logical bootstrap response can commit per MessageRequest; the winning bootstrap/import/`COMPLETE`/request-close transition is crash-consistent; and an exact completed lost-ack retry is recognized from replay state before live bootstrap-key lookup after the private key is destroyed. Confirm a losing raced wrapper cannot create a second branch; every OPEN exit atomically terminalizes losing RESERVED candidates before key destruction; reply_route is one logical response with exact retries plus at most 8 distinct candidates while OPEN; rejected/GC candidates cannot replenish that lifetime budget; the request deadline is exactly the authenticated route/key minimum with no independent `request_expires`; replay-only state expires at the derived deadline+skew; historical restore cannot reactivate OPEN/RESERVED/private-key/replay-only state; envelope expiry cannot outlive its authenticated context; and no clear-sender fallback exists. Review [`sealed_delivery_adr0147_lifecycle_restore.py`](../bounded/sealed_delivery_adr0147_lifecycle_restore.py) and its result explicitly. Also review [`account_private_sync_adr0148.py`](../bounded/account_private_sync_adr0148.py) and [`account_private_sync_adr0149_hardening.py`](../bounded/account_private_sync_adr0149_hardening.py): same-profile eligible-device records remain non-authoritative; concurrent unsent drafts do not collapse to a timestamp winner; revoked/in-flight records cannot become newly effective; future HLC values cannot pin state; tombstones prevent stale resurrection; resolution binds all draft heads; and historical restore cannot auto-republish old state under the fresh DeviceID. Review [`lan_route_adr0150.py`](../bounded/lan_route_adr0150.py): LAN-first/WAN-retry and WAN-first/LAN-retry materialize one EventID; route changes cannot grant peer eligibility or lower session security; direct blob receipt plus provider backfill preserves BlobID/ciphertext digest and never equates local receipt with storage durability.
- **V-015 / F-12 — provider/resource authority and metadata.** Review ADR-0138/0140/0142 provider-resource capabilities and privacy changes plus ADR-0150 local-delivery privacy. Routine relay/descriptor/key/RecoveryRecord/blob administration must not authenticate with AccountID, DeviceCredential or ACS authority. RecoveryRequest fan-out must explicitly expose AccountID + recovery-attempt to participating mailbox Homes and only there. Review [`lan_privacy_adr0150.py`](../bounded/lan_privacy_adr0150.py): fixed-cardinality/rotation or query privacy, immediate block/removal retirement, temporary invite scope, no stable identity/authority promotion, and bounded unauthenticated discovery/connection/blob work must hold.
- **V-016 / F-13 — decentralized abuse authority and Privacy Pass.** Check repeated nontrivial blind-factor reuse detection, `r=1`, direct-signing and CSPRNG controls. Confirm provider/safety capabilities cannot launder into universal identity or moderation authority. The older Room-model “design gaps” are already resolved by ADR-0121/0122/0123; verify the current normative semantics rather than reopening those historical gaps without a counterexample.
- **V-017 — continuity.** Confirm the existing continuity evidence/review remains applicable to the current semantic tree. ADR-0117 permits citing a prior accepted independent review for unchanged semantics. If ADR-0140..0143 or later verification-process edits invalidate any continuity assumption, review only that affected artifact now.

Also check that the current tree has not reintroduced an accepted F-1..F-13 defect.

## Gate updates, only if the review actually accepts them

The machine ledger is authoritative. Write one named review record under `verification/reviews/` with a per-gate disposition and concrete file/result citations. Then update only the corresponding pre-engineering review fields/evidence:

- V-002: mark the independent-review criterion met and set `pre_engineering_status=passed`, independence `satisfied` only if the review accepts it.
- V-003: the ADR-0150 route-reconciliation criterion is current; an independent review may now satisfy the remaining review criterion if accepted.
- V-015: the ADR-0150 local-delivery privacy/resource criterion is current; independent review may now consider phase passage, including the ADR-0147 lifecycle trace and ADR-0150 local path.
- V-016: 9/9 criteria are already met; only set phase passage/independence after acceptance.
- V-017: 3/3 criteria are already met; only set phase passage/independence after acceptance.

V-001 already has a passed pre-engineering phase. V-005 has all 11 repo-local criteria met, including ADR-0150/0151 rich-list evidence, but its independent-implementation phase record remains open; do not mark that independence satisfied solely from the OpenAI-authored Python/Node pair. Do **not** mark overall gate `status=passed` because pre-release obligations are separate. Cross-wave gates already at `feasibility_status=evidenced` remain evidence-only unless their own phase contract says otherwise.

If anything is rejected, leave that gate open and record the smallest bounded finding/remediation necessary.

## Authorization boundary

Do **not** set `engineering_authorized=true`, and do not authorize `core_kernel` yourself. If accepted review makes every machine prerequisite true, report exactly that **`core_kernel` is eligible for an explicit scoped unlock ADR**. The operator/integrator decides whether to accept that later ADR.

## Deliverable

Return one fresh ZIP plus SHA-256 with:

1. the named independent review record;
2. only accepted gate-ledger updates and regenerated gate view;
3. [`SNAPSHOT.md`](../../SNAPSHOT.md) / checkpoint updates;
4. successful `make verify-drop` from a raw extraction;
5. successful `make verify` from a tool-complete Git worktree, or a precise report of any genuinely unavailable external tool while preserving stronger prior evidence.

Avoid unrelated redesign, history copies and agent-to-agent bounce loops.
