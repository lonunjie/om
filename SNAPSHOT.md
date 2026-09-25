# OM Architecture Snapshot

Generated: **2026-09-25 11:54 PHT (UTC+08:00)**, after design-side verification-lifecycle hardening (ADR-0134)

## 2026-09-26 verification executor pass (ADR-0134)

Every repo-local `core_kernel` criterion now has passing evidence. V-001 and V-005 are `passed`. V-002, V-003,
V-015, V-016 and V-017 are open **only** for independent review. V-004, V-008, V-009, V-011, V-013, V-014 and V-019
have `feasibility_status=evidenced`. ADR-0135 pins the OM-Crypto-v1 bytes. Report:
[`2026-09-26-verification-executor-report.md`](docs/reports/2026-09-26-verification-executor-report.md). The single
remaining step before a `core_kernel` unlock ADR is
[`BRIEF-2026-09-26-core-kernel-independent-review.md`](verification/reviews/BRIEF-2026-09-26-core-kernel-independent-review.md).
Engineering is still not authorized.

## State

OM remains in **architecture verification / pre-engineering**. **Engineering is not authorized.** The blocking architecture-design queue is closed; remaining blockers are objective V-* evidence/review obligations.

- blocking architecture design items: **0**;
- open non-blocking/later-wave design items: **3** — A-030 (LegacyPolicy), A-031 (unlinkable AsyncKeyBundle ACS binding), A-036 (possible deniable groups research);
- verification gates V-001..V-023: **23 open**;
- engineering scopes authorized: **0 / 11**; `engineering_authorized=false`;
- machine ledgers: **275 requirements, 84 threats, 156 decisions through D-156 / ADR-0134**.

## 11:54 verification-process hardening

ADR-0134 removes the remaining process ambiguity before handing execution to the repo agent:

- evidence is separated into design-complete, repo-local pre-engineering, post-scope implementation, and genuinely external/human phases;
- isolated `verification/` prototypes are explicitly allowed before scope authorization but are non-production evidence only;
- every W0-impacting later-wave gate now has explicit `feasibility_criteria`, so W0 does not need full W1-W4 gate passage;
- substantive requirements formerly hidden only in free-text `remaining` notes were promoted into canonical gate criteria;
- free-text `remaining` lists were removed; unfinished work is derived mechanically from canonical criteria;
- `criteria_met` must exactly match canonical criteria, and a passed pre-engineering phase must also record satisfied independence;
- independent-review-only work is batched after the verification executor exhausts repo-local evidence, preventing model-to-model ping-pong;
- [`verification/CLAUDE_OPUS_5_5_HANDOFF.md`](verification/CLAUDE_OPUS_5_5_HANDOFF.md) is the one-way execution brief for the next repo pass.

## What changed on 2026-09-25

ADR-0131 closes the remaining W0 core design questions and three newly found cross-boundary gaps:

- dual Ed25519 + ML-DSA-65 binding for PQXDH identity/prekey material;
- exact durable last-resort PQXDH replay ledger with a fail-closed 4096-per-prekey cap;
- AES-256-GCM-SIV Double Ratchet payload profile and explicit unencrypted-header metadata exposure;
- exact v1 PQ wording: hybrid-PQ handshake/HNDL protection, no PQ post-compromise healing;
- recovery-alone activation measured from **each verifier's own first observation**, defeating withheld-request maturity claims without a trusted clock;
- every live/rebasable reduction targets an immutable incarnation, closing stale-reduction/ABA attacks;
- reciprocal `IntroIdentityBinding`, with `ACCEPTED_UNBOUND -> ACTIVE` only after both stable AccountID bindings verify;
- MessageRequest HPKE pins `draft-ietf-hpke-pq-05` MLKEM768-X25519/HKDF-SHA256/ChaCha20Poly1305 rather than inventing an OM hybrid.

ADR-0132 closes referral-token, group-call and executable-extension boundary questions:

- RFC 9578 publicly verifiable token type 0x0002 for recipient-issued referral admission;
- per-sender SFrame call keys distributed only to joined participant DeviceIDs; Room MLS epoch possession alone does not reveal media keys;
- plaintext crossing into a remote mini-app origin is egress itself;
- continuous 1:1 plaintext egress is explicit bilateral signed relationship policy.

ADR-0133 closes the provider/distribution design blockers:

- normative Home API v1, CDDL bodies, typed error registry, real ProviderCapabilities JSON Schema with `durability_class`, and missing provider interface contracts;
- Rekor-v2/C2SP-checkpoint-compatible release transparency with two independent monitor observations while preserving the owner-pinned TUF/root as authority;
- all W0 conformance suites now contain concrete case lists and are registered `specified`.

## Verification status

No implementation, formal-model, browser/platform, crypto, fuzz or environment verification suite was executed during the 11:54 design-hardening pass. The prior 08:59 checkpoint's captured evidence remains in the repository unchanged.

Only architecture/document consistency checks were run after the process changes:

- `python3 tools/validate_architecture.py` — **pass**;
- `python3 tools/om_docs.py check` — **pass**.

Current validator state: **680 current files; design_open=3; verification_open=23; pre_engineering_open=23; engineering_authorized=false; scopes_authorized=0/11**.

## Next work

Claude Opus 5.5 is the current `verification_executor`. It should complete all repo-local V-* evidence in one pass, beginning with the `core_kernel` prerequisites and the seven cross-wave feasibility obligations, then batch only genuine independent-review leftovers. It must not begin product engineering or self-authorize a scope. V-018 remains an external real-user study.
