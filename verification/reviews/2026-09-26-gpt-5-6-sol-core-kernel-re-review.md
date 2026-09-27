# core_kernel independent re-review — ADR-0136 remediation

- **Reviewer:** OpenAI GPT-5.6 Sol
- **Exact model id:** `GPT-5.6 Sol`
- **Date:** 2026-09-26
- **Base commit:** `fa8370d665eb9e3e6659040f332fb3305b8b2299`
- **Scope:** the eight-item remediation queue in [`BRIEF-2026-09-26-core-kernel-re-review.md`](BRIEF-2026-09-26-core-kernel-re-review.md), limited under ADR-0117 to the artifacts changed for ADR-0136 / findings F-1..F-9 and the reopened V-001 responder-binding criterion.

## Method and overall result

I first re-read [`AGENTS.md`](../../AGENTS.md), [`SNAPSHOT.md`](../../SNAPSHOT.md), [`ADR-0136`](../../decisions/ADR-0136-core-kernel-review-remediation.md), and my prior review [`2026-09-26-gpt-5-6-sol-core-kernel.md`](2026-09-26-gpt-5-6-sol-core-kernel.md), then followed the re-review brief item by item. I treated passing self-checks as evidence about an encoded rule, not proof that the rule itself is the intended one.

Executable evidence run from this export:

- [`acs_authorization_table.py`](../bounded/acs_authorization_table.py): 16,200 bounded cases, 53 vectors, 0 violations;
- [`acs_tla_crosscheck.py`](../bounded/acs_tla_crosscheck.py): 11 traces, 13 constitutional steps, 12 positive replays, 17 negative mutations, 0 failures;
- [`metadata_w0_rules.py`](../bounded/metadata_w0_rules.py): all current checks and mutations passed;
- [`metadata_trace_model.py`](../bounded/metadata_trace_model.py): 144 modeled flow cases, 0 reported violations;
- [`abuse_w0_rules.py`](../bounded/abuse_w0_rules.py): all current checks and mutations passed;
- [`privacy_pass_issuance.py`](../bounded/privacy_pass_issuance.py): RFC 9474/RFC 9578 KATs and the current issuer-transcript mutation passed;
- `continuity_bundle.py check-manifest`: the raw export has no `.git`, so this command cannot run directly there. In a disposable Git worktree made from the exact export, it reported 76 architecture/constitution docs and 0 problems. `build` then produced a 365-file bundle and `verify` accepted it offline.

The V-001 Python checker could not run in this environment because the installed `cryptography` build has no `cryptography.hazmat.primitives.asymmetric.mldsa`; Rust tooling was also unavailable. I therefore reviewed the V-001 source/vectors and the committed native/wasm/Python results for those portions instead of claiming a fresh rerun.

| # | Queue item | Verdict |
|---|---|---|
| 1 | F-1, F-2 / V-002 AccountControl remediation | **reject** — F-10 |
| 2 | F-3 / state-derived quarantine lift | **accept** |
| 3 | F-4 / V-003 K12 normative boundary | **accept** |
| 4 | F-9 / reopened V-001 responder binding | **reject** — F-11 |
| 5 | F-5 / V-015 stable-device metadata | **reject** — F-12 |
| 6 | F-6, F-7 / V-016 referral-token profile and issuance privacy | **reject** — F-13 |
| 7 | F-8 / V-017 continuity manifest | **accept** |
| 8 | ADR-0136 as a whole / authority review | **reject** — F-10..F-13 remain open |

The remediation fixes the original defects in important ways: recovery method count no longer amplifies authority; ordinary quarantine lift is state-derived; K12 is now stated at the right conceptual boundary; type `0x0002` is the correct RFC profile; and the continuity bundle has moved from a small basename heuristic to an explicit manifest. Four narrower issues remain before these queue items can close.

## 1. F-1, F-2 — V-002 AccountControl remediation

**Verdict: reject.**

F-1 is fixed. [`AccountControl.tla`](../tla/AccountControl.tla) now makes a RecoveryPolicy one vote regardless of one-vs-two custody methods, and `CanVeto` lets any usable controller stop a recovery-alone request during the modeled delay. The mutation deliberately restoring method-count authority is appropriately separate from the intended model.

The ordinary F-2 lift rule is also fixed: `Unquarantine` is device-only, recovery never contributes, and quarantined controllers remain in the denominator. A threshold-disabling quarantine enters `frozen`; `FreezeExitOnlyByResolve` constrains exit from the TLA safety freeze.

ADR-0136 rule 2's **challenge-signature exception is not an authority leak** as written. During a freeze, the signer set is still restricted to controllers from the parent state, the threshold remains the parent-state majority, and recovery contributes zero. A quarantined controller therefore does not create a new vote or reduce the constitutional threshold; its key is usable only for the narrow resolution path. A coalition able to supply that parent-state majority already represents the constitutional device majority.

There is, however, an important liveness qualification: challenge signatures do **not** prevent one malicious controller from making a two-controller account permanently unavailable by withholding its own signature. With N=2, freeze resolution is still 2-of-2. The exception is useful for larger sets—for example, in N=3 two quarantined honest controllers can resolve a freeze without the controller that quarantined them—but ADR-0136's specific two-controller rationale should not be read as a liveness guarantee.

The blocking problem for this item is that the two executable encodings do not accept the same freeze-resolution children, and the current cross-check is one-way.

### F-10 — Freeze-resolution acceptance diverges between the TLA model and authorization table

Severity: major

Where: [`verification/tla/AccountControl.tla`](../tla/AccountControl.tla) `ResolveFreeze`; [`verification/bounded/acs_authorization_table.py`](../bounded/acs_authorization_table.py) `_authorize` frozen branch; [`verification/bounded/acs_tla_crosscheck.py`](../bounded/acs_tla_crosscheck.py); [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) §5.

Problem: `ResolveFreeze` has one resolution shape: it sets `quarantined' = {}`, explicitly “lifts every quarantine.” The executable authorization table instead accepts any child whose deltas are all `lift_quarantine` when a device-only majority of the parent controllers signs. It does not require all quarantines to be lifted. For a reachable frozen parent with controllers `{d1,d2,d3}` and quarantined `{d2,d3}`, `authorize()` accepts a child that lifts only `d2` and leaves `d3` quarantined when `{d1,d2}` sign. No `ResolveFreeze` transition can produce that child. The prose “lifts the quarantines” does not settle whether all or a sufficient subset is canonical. [`acs_tla_crosscheck.py`](../bounded/acs_tla_crosscheck.py) replays TLA transitions into the table, but does not search for table-accepted transitions absent from TLA, so it passes despite this extra acceptance surface. Two honest verifiers can therefore disagree on whether the same frozen-parent child is valid.

Proposed fix: Choose one normative resolution shape and encode it everywhere. If freeze resolution must lift every quarantine, make the table reject partial-lift children and add a negative vector. If a majority may lift only a subset, define the permitted subset and the resulting freeze-exit condition explicitly, change `ResolveFreeze` accordingly, and prove that the remaining non-quarantined set satisfies the intended threshold. Extend the cross-check with reverse/table-only mutations (at minimum the partial-lift case), rather than treating TLA→table replay as equivalence. Also correct the N=2 explanatory sentence so it does not imply challenge signatures guarantee liveness against a withholding controller.

Resolution:

## 2. F-3 — state-derived quarantine lift

**Verdict: accept.**

The ordinary lift rule is now evaluable from canonical parent/child state. The verifier counts the parent controller set; quarantined controllers remain in N; only active, non-quarantined parent controllers sign an ordinary lift; recovery never contributes; and nothing depends on who initiated a quarantine. §5 cleanly separates the special safety-freeze case. No hidden initiator provenance is required.

For a quarantine-induced safety freeze, whether the parent is below the voting threshold is itself derivable from the parent ACS. Fork freeze remains a different local conflict condition with its own fork-bridge rule. F-10 concerns the **shape of a freeze-resolution child**, not a return of F-3's hidden-provenance defect.

## 3. F-4 — V-003 K12 normative wire-to-DeviceID boundary

**Verdict: accept.**

K12 is now normative and states the missing boundary with the necessary ingredients:

- the inbound `IK_A` must resolve to exactly one DeviceCredential committed by the trusted peer ACS head for the **relationship route that delivered the envelope**;
- the resolved device must be active and not quarantined;
- binding precedes secret-key work, one-time-key consumption, session creation and plaintext release;
- a possibly stale peer head is refreshed/validated without rollback under K7 before processing; until then the envelope remains unprocessed; and
- first contact is the HPKE `MessageRequest` path, so an unbound PQXDH initiation is never the mechanism that creates the first relationship.

[`OneToOneSessions.tla`](../tla/OneToOneSessions.tla) deliberately abstracts the credential/route machinery into `BindInitiator`; `ForgeInitiation` plus S6 demonstrates why the assumption is security-significant. That model is not, by itself, a full implementation of route pinning or stale-head refresh, but the normative K12 text is sufficiently explicit for V-003. F-11 below is a defect in the concrete V-001 evidence path, not an ambiguity in K12 itself.

## 4. F-9 — reopened V-001 responder binding

**Verdict: reject.**

The repair gets the **ordering** right on the route-aware path: `decrypt_on_route` calls `verify_initiator` before `accept_initial`, and `accept_initial` is where private prekey lookup/use, PQXDH responder work, first DR decryption, one-time-key deletion, ledger recording and session creation occur. The committed negative set covers unknown key, wrong route, revoked, quarantined, stale/rotated credential, uncommitted substitution, ambiguous key ownership and untrusted ACS; the Rust self-check also exercises crossed initiation. The composed authentication argument in ADR-0135 §7 is acceptable in principle: ACS/credential authenticity binds the identity keys, mandatory composite signatures bind recipient prekey material, and PQXDH then proves possession/establishes the session over identity keys that both sides authenticated through those chains. OM does not need to pretend this is XEdDSA's original proof, and §7 correctly does not do so.

The default persisted receive API nevertheless bypasses the relationship-route part of the rule.

### F-11 — V-001's default receive path makes the relationship route optional for new-session authentication

Severity: blocker

Where: [`verification/prototypes/v001-crypto/rust/core/src/store.rs`](../prototypes/v001-crypto/rust/core/src/store.rs) `receive`, `decrypt`, `decrypt_on_route`, `accept_initial`; [`verification/prototypes/v001-crypto/rust/core/src/binding.rs`](../prototypes/v001-crypto/rust/core/src/binding.rs) `verify_initiator`; [`verification/prototypes/v001-crypto/rust/core/src/otk.rs`](../prototypes/v001-crypto/rust/core/src/otk.rs) initiator-binding checks; [`decisions/ADR-0135-om-crypto-v1-construction-pinning.md`](../../decisions/ADR-0135-om-crypto-v1-construction-pinning.md) §7; K12 in [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../../docs/architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md) §3.

Problem: `receive()` clones the device and calls `next.decrypt(env, rng)`. `decrypt()` calls `decrypt_on_route(env, None, rng)`. `verify_initiator()` interprets `route_account = None` as “match any trusted peer,” so a new initial envelope can be accepted through the ordinary persisted receive path when `IK_A` belongs to some trusted peer, even if the envelope was delivered on another relationship's route. This violates ADR-0136 rule 4/K12, which requires the trusted ACS head **for the relationship that delivered the envelope**. The route-mismatch negative exercises `decrypt_on_route(..., Some(other_account), ...)`, while another self-check explicitly shows `decrypt()` accepting the same bound initiation without a route. Thus the route-aware helper proves that the check can be done, but the main receive API does not make it mandatory. This is an identity-confusion acceptance path at the exact boundary F-9 reopened V-001 to close.

Proposed fix: Make relationship context mandatory for every public/persisted **new-session** receive path. For example, replace `receive(env, ...)`/initial `decrypt(env, ...)` with an API that requires the relationship/account route and refuses an initial envelope if that context is absent; an already-established session may use the account/device binding stored with the session. Remove `Option`/`None` as a valid initial-session authentication mode, or make `None` reject whenever no session already exists. Add a full persisted-path negative that delivers Alice's otherwise valid initiation on Carol's route through the same API that commits to `Vault`, and assert byte-identical state, no one-time-key/ledger/session change and no plaintext release.

Resolution:

## 5. F-5 — V-015 stable-device metadata

**Verdict: reject.**

The executor's two extensions are real:

- each OM-Crypto-v1 1:1 envelope begins with the sender's clear `EncodeEC(IK)` and the relay layer carries that envelope, so mailbox-class Homes obtain a stable sender-device correlator on each such envelope; and
- an APNs/FCM token (or analogous subscription endpoint) is a per-install/device linkage point at the push provider. [`NOTIFICATIONS_AND_PUSH.md`](../../docs/architecture/client/NOTIFICATIONS_AND_PUSH.md) already acknowledges that it can link profiles when the same token is handed to more than one party.

The original F-5 KeyMaterialProvider claim is also fixed: `AsyncKeyBundle.subject_device_id` is an explicit stable per-device value, not merely `device_count`.

A-054 is an acceptable **non-blocking** disposition for W0's clear sender key. W0 now declares the exposure instead of promising sender hiding; a sealed-sender outer envelope changes the crypto/transport profile and can reasonably be a later design item. The metadata accounting still misses other W0 provider observations, however.

### F-12 — `device_auth` exposes additional stable DeviceCredential correlators that V-015 still marks `no_by_design`

Severity: major

Where: [`spec/protocol/home-api.md`](../../spec/protocol/home-api.md) §§2–3 (`device_auth` and operations); [`spec/cddl/common.cddl`](../../spec/cddl/common.cddl) `signature.0` signer key/device id; [`spec/metadata/w0-metadata-exposure-descriptors.json`](../../spec/metadata/w0-metadata-exposure-descriptors.json) `discovery_publisher`, `recovery_record_store`, `blob_store`; [`verification/bounded/metadata_w0_rules.py`](../bounded/metadata_w0_rules.py) `device_correlator`; [`verification/bounded/metadata_trace_model.py`](../bounded/metadata_trace_model.py).

Problem: W0 defines `device_auth` as a signature by an active, non-quarantined DeviceCredential plus the referenced ACS head. The common signature encoding contains a signer key/device id, and the provider must resolve that signer against the credential/head to authenticate the operation. W0 uses `device_auth` for descriptor publication, RecoveryRecord writes, key-bundle publication, relay capability mint/revoke, and blob-location repair (or relationship policy). The KeyMaterialProvider and mailbox roles are already marked `always` for independent reasons, but `discovery_publisher` and `recovery_record_store` still say `stable_device_correlator: no_by_design`, and `blob_store` says the same despite its device-authenticated repair path. `metadata_w0_rules.py::device_correlator` checks only the AsyncKeyBundle and 1:1 envelope bytes; [`metadata_trace_model.py`](../bounded/metadata_trace_model.py) omits descriptor-publication, recovery-record-write and repair authentication flows. Therefore both models pass while these roles can observe a stable DeviceCredential signer during control-plane operations. The sender-IK and push-token extensions are correct but incomplete.

Proposed fix: Extend the descriptor/accounting model to include authentication-layer metadata, not only request bodies/data-plane envelopes. Mark each affected role at the frequency its W0 operations actually expose a stable DeviceCredential correlator (`sometimes` is likely appropriate for roles where only writes/repair use `device_auth`; derive the exact value from the operation model). Add modeled flows and mutations for descriptor publication, RecoveryRecord put and blob repair. Keep KeyMaterialProvider/mailbox claims as the stronger existing disclosures where applicable.

Resolution:

## 6. F-6, F-7 — V-016 referral-token profile and issuance unlinkability

**Verdict: reject.**

F-6 is fixed. Type `0x0002` is now bound to the standards-conforming `RSABSSA-SHA384-PSS-Deterministic` profile with a 48-byte PSS salt, and `token_key_id` is derived from the id-RSASSA-PSS SPKI. The RFC 9474 and RFC 9578 vectors are the right kind of evidence, and the PSSZERO mutation demonstrates that the two profiles are no longer conflated.

The intended W0 unlinkability property should be stated narrowly and concretely:

> A redeemed referral token must not, from the blind-RSA protocol transcript itself, let the recipient/issuer—or an IntroGate that obtains/colludes with the issuer's issuance transcripts—map that redemption back to the particular issuance transaction/contact that received the blind signature.

That property does **not** hide from the recipient/issuer which contact participated in an issuance; does not hide the IntroGate's redemption-side network address, timing, token use or ticket; does not defeat timing/cardinality correlation when observations are combined; and does not hide an optional `ReferralAttestation` from the recipient after the sealed request is decrypted. The “IntroGate ... learns nothing about Alice's contacts” wording in §6 should therefore be read as a protocol-identifier/admission-proof claim, not a universal traffic-analysis claim. The metadata threat model already treats collusion/timing separately.

The new evidence is materially better than the old direct-signing model, but its unlinkability regression check is not strong enough to establish the property it is named for.

### F-13 — The issuer-transcript checker passes reusable nontrivial blinding factors that make issuances linkable as a set

Severity: major

Where: [`verification/bounded/privacy_pass_issuance.py`](../bounded/privacy_pass_issuance.py) `blind`, `transcript_problems`, `issuer_transcript_unlinkable`; [`verification/bounded/abuse_w0_rules.py`](../bounded/abuse_w0_rules.py) `issuer_transcript_unlinkable`; [`INVITES_INTROGATE_AND_SPAM.md`](../../docs/architecture/discovery/INVITES_INTROGATE_AND_SPAM.md) concrete referral-token profile.

Problem: The production-shaped helper currently samples a fresh blinding factor with `secrets`, which is correct, and the document explicitly says RFC blindness relies on a uniformly random blinding factor. But the **test claimed as F-7 evidence does not enforce independence/freshness of that factor**. Its mutation sets `r=1`, which collapses to the old unblinded/directly recognizable case. I reran the same issuance machinery with `r=2` reused for four otherwise normal issuances. `transcript_problems()` returned `[]`. Yet after seeing the redeemed token signatures the issuer can compute, for each issuance transcript/token candidate pair, `r_candidate = blind_sig * sig^-1 mod n`; the true four pairings all exposed the repeated value `2` (four occurrences among the 16 candidates), giving a strong linking marker. The current “every transcript is consistent with every token” check cannot catch this: RSA algebra yields *some* candidate `r` for every pair, so pairwise consistency is not evidence that the actual blinding factors were independently sampled. Thus a plausible regression that breaks issuance unlinkability still passes the named unlinkability check.

Proposed fix: Keep the RFC KATs, but add a deterministic freshness/linkability regression. Exercise several issuances with distinct known coprime blinding factors as the positive control and a mutation that reuses the same nontrivial factor (not only `r=1`) as the negative control; the issuer-side candidate matrix must detect the reuse/linkable matching structure. In the normative W0 text, state the cryptographic observer/property boundary above and explicitly separate it from network/timing/cardinality and post-decryption ReferralAttestation leakage. The executable implementation may continue using `secrets`/a CSPRNG; the purpose of the deterministic mutation is to ensure the evidence suite fails if factor freshness is accidentally removed or statefully reused.

Resolution:

## 7. F-8 — V-017 continuity manifest and offline reconstruction

**Verdict: accept.**

The explicit manifest repairs the original small-basename allowlist problem. It enumerates every current `docs/architecture/**` and `docs/constitution/**` document (76 in this tree), while the bundle's existing includes carry specs, schemas, registries, decisions, vectors, machine ledgers and licenses. The checked exceptions are identified and justified as non-normative evidence/process/context rather than silently omitted semantics.

In a disposable Git worktree made from the exact export, `check-manifest` reported `OK (76 doc(s), 0 problem(s))`. `build` produced a 365-file continuity bundle, and `verify` accepted that bundle offline. Source review confirms that `verify` requires the manifest's document members and resolves relative normative references inside the bundled bytes, while `check-manifest` detects architecture/constitution doc-set drift in the repository. D7 is appropriately candid that its object/role→document mapping is still a human-maintained semantic coverage check (G7); that is not a return of F-8 because the normative doc set itself is now explicit and mechanically guarded.

The direct `check-manifest` command failing in the exported drop due to absent `.git` is expected for a repository-tree/ref check; the **offline bundle verifier** itself does not require Git.

## 8. ADR-0136 as a whole — authority and interpretability

**Verdict: reject.**

The authority direction of ADR-0136 is generally sound:

- recovery remains one constitutional vote and any live controller can veto recovery acting alone during the delay;
- the challenge-signature freeze exception does not lower the parent-state device majority and gives recovery no freeze authority;
- ordinary quarantine lift no longer depends on hidden provenance;
- K12 puts peer-device authority at the trusted ACS/DeviceCredential boundary rather than relay/directory identity;
- the metadata and Privacy Pass changes do not grant providers protocol authority; and
- the continuity manifest changes packaging/evidence, not authority.

I found no new conceptual contradiction with ADR-0111's one-vote/veto rule, ADR-0127's parent-state transition authority, or ADR-0131's DeviceIdentity/PQXDH binding direction. The composed authentication argument in ADR-0135 §7 is acceptable **provided the mandatory route-pinned responder precondition is actually enforced**.

The queue cannot close yet because honest implementations/evidence can still diverge at four concrete points: F-10 gives two accepted sets for freeze-resolution children; F-11 leaves a public/persisted responder path that does not enforce the relationship route; F-12 under-declares provider-visible DeviceCredential correlation; and F-13's named issuance-unlinkability check accepts a reusable-blinding-factor construction that is linkable as a set. None requires replacing OM's constitution or operatorless model. They are bounded remediation items, but F-11 remains a blocker at the identity-attribution boundary and the other three are major verification/specification gaps.
