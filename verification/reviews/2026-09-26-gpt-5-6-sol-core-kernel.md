# core_kernel independent review — consolidated pass

- **Reviewer:** OpenAI GPT-5.6 Sol
- **Exact model id:** `GPT-5.6 Sol`
- **Date:** 2026-09-26
- **Base commit:** `7a0004f471e0593faeb5238fab24673f47a8800c`
- **Scope:** the seven queue items in `verification/reviews/BRIEF-2026-09-26-core-kernel-independent-review.md`, replacing the unfulfilled 2026-09-24/25 independent-review briefs as directed. Pre-release criteria, V-018, and later-wave gates V-006/V-007/V-010/V-012/V-020..V-023 are excluded.

## Method and overall result

I read `AGENTS.md`, `SNAPSHOT.md`, and `docs/reports/2026-09-26-verification-executor-report.md` first, then reviewed the cited normative documents, ADRs, frozen schemas/registries, models and prototypes. I also executed the relevant bounded Python evidence on a disposable copy of the export: the ACS table reported 16,200 cases / 48 vectors with no modeled violation; the restore/redelivery explorer reported 13,378 states; the floor vectors passed; the metadata, IntroGate/Sybil, and abuse-rule scripts passed their present self-checks. Passing self-checks are not treated as proof that the encoded rule is the intended rule.

For external construction semantics I checked the current PQXDH specification and the RFC 9474 / RFC 9578 RSA blind-signature profiles. The exported review tree contains no `.git`, so the git-dependent continuity-bundle builder itself cannot be rebuilt from this export; its selection and verification logic, the recorded dry-run evidence, and the tabletop procedure are reviewable directly.

| # | Queue item | Verdict |
|---|---|---|
| 1 | V-002 AccountControl / recovery models | **reject** |
| 2 | V-003 one-to-one session model | **reject** |
| 3 | V-015 metadata exposure | **reject** |
| 4 | V-016 abuse / IntroGate / moderation | **reject** |
| 5 | V-017 continuity bundle / drills | **reject** |
| 6 | ADR-0135 OM-Crypto-v1 construction | **reject** — F-9 reopens V-001 |
| 7 | Claude-authored design since 2026-09-24 | **reject** — cross-cutting findings F-3 and F-4 are dispositive |

The rejected verdicts do **not** mean the whole kernel needs redesign. The executor has a small number of concrete rule/evidence mismatches to repair. I found no defect in the V-005 frozen-codec evidence in this pass; V-005 is not reopened.

## 1. V-002 — AccountControl, recovery observation, authorization table

**Verdict: reject.**

The Python authorization-table model is a good direct mirror of the current §4.1 table, including device-only majority for lifting quarantine. `RecoveryObservation.tla` correctly makes the activation delay verifier-local rather than trusting a Home/custodian timestamp. `ControllersReact` is an acceptable explicit environment assumption for the conditional claim it proves: a live controller that has actually observed the request reacts before modeled time advances. Likewise, a corrected `OwnerMustReact` can be used as a liveness/environment assumption; it must not be read as protection against a request withheld from every controller.

T-041 is an acceptable disclosed residual: if the legitimate owner has no live controller left, there is nobody able to exercise the veto during the delay. T-090 is also acceptable as disclosed: a request withheld from every live controller can mature for a partitioned non-controller verifier after that verifier's own full interval; late verifiers restart their own interval and later contact can expose a fork. These are availability/partition residuals, not authority silently delegated to a Home.

The §4.1 lift-row concern in the executor report is real: its current wording asks a verifier to know provenance that the ACS parent/child representation does not carry.

### F-1 — Recovery method count incorrectly changes veto authority

Severity: blocker

Where: `verification/tla/AccountControl.tla` (`OwnerMustReact`, `RequestRecovery`, `Veto`); `verification/tla/README.md`; compared with `decisions/ADR-0111-identity-recovery-on-by-default.md`, `decisions/ADR-0127-account-authority-transition-rules.md`, and `docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md` §3.

Problem: The model gives a two-method recovery request stronger authority than a one-method request. `Veto` is enabled only when `pending.methods = 1`, and `OwnerMustReact` requires only a majority-capable owner reaction when `pending.methods = 2`. The README states that two-method requests can only be cancelled by a majority act. That contradicts the normative rule: RecoveryPolicy is exactly one vote regardless of custodian/method composition, and **any live controller can veto recovery acting alone during the activation delay**. Method count is a recovery-availability implementation detail; it must not amplify constitutional authority. The present TLA can therefore prove safety for a different recovery constitution than ADR-0111/0127 specify.

Proposed fix: Remove recovery-method count from authorization/veto semantics. Any pending recovery-alone request must be vetoable by any current live non-quarantined controller. `OwnerMustReact` should require that reaction whenever such a controller has observed an attacker request, independent of one-vs-two recovery methods. Keep multi-method/guardian mechanics only as conditions for producing the single RecoveryPolicy vote. Add a mutation/golden trace demonstrating that changing custodian method count never changes the set of authorized ACS outcomes.

Resolution:

### F-2 — AccountControl.tla does not mirror the quarantine/lift authority rules

Severity: major

Where: `verification/tla/AccountControl.tla` (`Quarantine`, `Unquarantine`, `Authorized`); `verification/bounded/acs_authorization_table.py`; `docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md` §4 and §4.1.

Problem: There are two model/spec divergences. First, `Unquarantine` calls the generic `Authorized(p,V,rv)`, so a recovery vote can contribute to lifting a quarantine, while the normative table and bounded authorization model require a device-only majority. Second, `Quarantine` leaves `frozen` unchanged even when the quarantine would leave fewer non-quarantined controllers than the parent majority; §4 rule 4 requires that transition to trigger safety freeze. These omissions matter because V-002 is supposed to validate the constitutional state machine, not merely the denominator arithmetic.

Proposed fix: Give lift-quarantine a dedicated device-only-majority authorization predicate with `rv = 0`. Make a threshold-disabling quarantine enter `frozen = TRUE` (or model the exact equivalent state transition) and prove that only the specified device-only freeze resolution can leave it. Add cross-check traces against `acs_authorization_table.py` so the executable table and TLA cannot silently encode different authorities.

Resolution:

### F-3 — The normative lift-quarantine row depends on provenance the ACS does not encode

Severity: major

Where: `docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md` §4 rule 2 and §4.1 lift-quarantine row; `decisions/ADR-0127-account-authority-transition-rules.md`; frozen AccountControlState representation.

Problem: The phrase “not counting quarantines the signer initiated” cannot be evaluated from an ACS child and its parent: the state records which devices are quarantined, not which controller initiated each quarantine. Rule 1 already says quarantining a controller does not remove it from the constitutional denominator, so the phrase is also ambiguous/redundant under the current representation. Two honest implementations can consequently differ: one can treat ordinary parent-state device-only majority as sufficient, while another can demand unavailable initiator provenance or maintain non-canonical side state.

Proposed fix: Rewrite the normative row purely in terms of committed state, preferably: quarantined controllers remain in `N`; only current active non-quarantined device signatures count toward the lift; a device-only parent-state majority is required and recovery never counts. If initiator provenance is actually intended to affect authorization, add a canonical authenticated provenance field and corresponding transition rules instead of relying on local history. The simpler state-derived rule is consistent with the existing bounded table model.

Resolution:

## 2. V-003 — one-to-one session model and restore semantics

**Verdict: reject.**

K1–K11 are directionally correct, and the dedicated restore work fixes the important earlier abstraction error: restore enrolls a **fresh DeviceID**, the old endpoint is revoked through account control, ratchet/prekey secrets are not restored, and an import into an installation that already has state must merge rather than overwrite (dedupe union, floor/head maxima). `floor.py` correctly takes the post-restore floor as the maximum of backed-up floor and ACS minimum. The bounded history-gap residual is acceptable: a message addressed only to a lost old DeviceID during the interval before the sender learns the replacement can require history sync or remain unavailable. That is an explicit history/availability gap, not permission to revive the old cryptographic endpoint.

K1–K11 are nevertheless not sufficient at the wire-to-authority boundary because the sender in the model is already represented as an authenticated DeviceID. The concrete V-001 responder does not establish that mapping.

### F-4 — Inbound PQXDH initiator identity is not bound to sender DeviceIdentity before session acceptance

Severity: blocker

Where: `decisions/ADR-0120-one-to-one-session-manager-rules.md` K6/K9; `verification/tla/OneToOneSessions.tla`; `verification/prototypes/v003-sessions/`; `decisions/ADR-0131-core-kernel-final-pre-engineering-hardening.md` rules 1 and 8; the boundary delegated by `verification/prototypes/v001-crypto/README.md`.

Problem: V-003 models a sender as a DeviceID that can already be looked up in the receiver's validated sender ACS; K9 then rejects a DeviceID known revoked. It never models or specifies the preceding wire-to-DeviceID step that turns a peer identity key into that authorized DeviceID. The V-001 evidence explicitly delegates initiator `IK_A` authentication to V-003 / IntroIdentityBinding, but V-003 abstracts it away and reciprocal IntroIdentityBinding binds the stable **account** transition; it does not by itself define how an arbitrary inbound device `IK_A` is authenticated at session acceptance. Thus K9 is not sufficient as an executable rule: an implementation can satisfy the model after assuming exactly the identity mapping whose security it needs to prove.

Proposed fix: Make receiver-side initiator binding normative and executable in the session-manager rules. Before a new inbound session is accepted, resolve the peer identity key to exactly one sender DeviceCredential whose digest/DeviceID is authorized by an acceptable trusted sender ACS head, with the device active and not quarantined/revoked; fail closed otherwise. This may be satisfied by a previously validated reciprocal relationship/device-bundle view or by explicit authenticated evidence carried/referenced by the initiation, but the rule must say which. Strengthen K9/K6 (or add a new rule) and add negative vectors for arbitrary identity keys, swapped keys between credentials, stale/revoked/quarantined credentials, and crossed initiation. F-9 separately covers the concrete ADR-0135/V-001 construction consequence.

Resolution:

## 3. V-015 — W0 metadata-exposure descriptors

**Verdict: reject.**

The new `ratchet_header_shape` disclosure is accurate for mailbox-class Homes: OM-Crypto-v1 exposes the ratchet public key/counters and therefore chain/turn shape. The co-located Home-role statement is also appropriately conservative: discovery publisher, IntroGate and mailbox operated by one Home are one observer, and the descriptor records that linkage.

The KeyMaterialProvider description, however, conflates “number of devices” with a stable per-device identifier that is actually present in the frozen bytes.

### F-5 — KeyMaterialProvider descriptor under-claims the clear stable DeviceID correlator

Severity: major

Where: `spec/cddl/keys.cddl` `AsyncKeyBundle` key 2 (`subject_device_id`); `spec/metadata/w0-metadata-exposure-descriptors.json` notes and `key_material_provider`; `verification/bounded/metadata_w0_rules.py::key_material`; `verification/bounded/metadata_trace_model.py`.

Problem: Every AsyncKeyBundle exposes `subject_device_id` in cleartext. It is a stable per-device value and, for the same device, is the same across bundle refreshes and provider locations. That is more information than `device_count`: it permits longitudinal tracking of a particular device and can support cross-provider device linkage for an observer that sees both sides. The descriptor note says this adds no linkage beyond ACS generation/hash grouping and is “covered by device_count,” but the model has no field representing a stable device correlator. `metadata_w0_rules.py` parses the real bundle and sees key 2, yet its descriptor assertion only checks `stable_account_id` and `device_count`; `metadata_trace_model.py` does not model the bundle field. A mutation could therefore remove the disclosure claim entirely without the present trace vocabulary detecting the actual per-device exposure.

Proposed fix: Add an explicit exposure dimension such as `stable_device_id` / `stable_device_correlator` to the logical metadata schema and mark the KeyMaterialProvider at the frequency actually exposed by W0 bundles. Update the threat-model prose and both metadata checkers to derive it from real `AsyncKeyBundle` key 2. Add a mutation proving the model fails if the DeviceID disclosure is omitted or incorrectly downgraded to device-count-only information.

Resolution:

## 4. V-016 — IntroGate/Sybil, referral tokens, room moderation

**Verdict: reject.**

The bounded IntroGate resource model matches the intended best-effort posture: invite/referral capacity remains reserved while unauthenticated public-first-contact availability can degrade under a distributed Sybil flood. That residual is consistent with the design rather than a hidden authority escalation. The RoomModeration model's accepted-order authorization, Space binding, epoch-scoped authority and committed-lift behavior are consistent with ADR-0122/0128 for this review scope.

The Privacy Pass evidence is not standards-conforming as written and does not test the privacy property for which ADR-0132 selected blind RSA.

### F-6 — RFC 9578 type 0x0002 verifier uses the wrong RSA-PSS profile

Severity: major

Where: `verification/bounded/abuse_w0_rules.py` (`PSS0`, `Issuer.token`, IntroGate verification); `decisions/ADR-0132-abuse-calls-and-extension-boundary-closure.md` rule 1; `docs/architecture/discovery/INVITES_INTROGATE_AND_SPAM.md` W0 referral-token profile.

Problem: The script describes token type `0x0002` as RFC 9578 / RFC 9474 `RSABSSA-SHA384-PSS-Deterministic` but signs and verifies with RSA-PSS salt length **0**. RFC 9474 gives the deterministic SHA-384 PSS profile a 48-byte salt; the zero-salt construction is the separately named PSSZERO profile. RFC 9578 token type 0x0002 verification uses RSASSA-PSS with SHA-384, MGF1-SHA-384 and a 48-byte salt. Therefore the current “real RSA” positive evidence is for a different signature profile and cannot count as conformance evidence for the selected token type.

Proposed fix: Replace the zero-salt primitive with the exact RFC 9578 type-0x0002 verification parameters and add published/independently generated known-answer positives and negatives. Keep algorithm/profile names exact so PSSZERO and ordinary 48-byte-salt PSS cannot be interchanged by a future implementation.

Resolution:

### F-7 — Redemption-only token model does not evidence the unlinkable issuance property

Severity: major

Where: `verification/bounded/abuse_w0_rules.py::Issuer.token`; `decisions/ADR-0132-abuse-calls-and-extension-boundary-closure.md` rule 1; `docs/architecture/discovery/INVITES_INTROGATE_AND_SPAM.md` §7 and W0 profile text.

Problem: Omitting blind/unblind mechanics is acceptable **only** if the criterion is explicitly limited to the IntroGate's redemption-side signature verification, challenge binding, epoch handling and replay prevention. The current test issuer directly signs the final `token_input`; that does not model the blind issuance/finalization protocol and therefore supplies no evidence for the unlinkability property that motivates Privacy Pass in ADR-0132. If the issuer and IntroGate observations are ever compared, direct signing would make the exact redeemed input linkable by construction. The present passing test must not be read as validating RFC 9578 issuance privacy.

Proposed fix: State the evidence boundary explicitly and add a separate standards-conforming issuance/finalization test using RFC 9578 blind RSA (or authoritative vectors/library behavior), including a structural test that the issuer transcript does not contain the final token input/signature linkage available at redemption. If issuance unlinkability is deliberately left to a different gate, record that dependency in V-016 rather than crediting this direct-signing model for it.

Resolution:

## 5. V-017 — Protocol Continuity Bundle and tabletop drills

**Verdict: reject.**

The tabletop scenarios themselves are realistic for the current pre-engineering maturity and correctly distinguish degraded availability/discovery from reassignment of cryptographic authority. The bundle integrity design is also sensible: deterministic file hashes plus one integrity root make offline byte-integrity verification straightforward.

The bundle is not yet complete enough to satisfy the constitution's requirement that a published profile be interpretable from the mirrored artifacts without a live OM registry/site.

### F-8 — Continuity bundle selects only a small basename allowlist of normative semantics

Severity: major

Where: `tools/continuity_bundle.py` `INCLUDE`, `NORMATIVE_DOCS`, `select`, and `cmd_verify`; `docs/constitution/PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md` §3; `docs/process/CONTINUITY_TABLETOP_DRILLS.md`.

Problem: The builder includes all `spec/` and all ADRs but only seven architecture/constitution documents by basename as “normative docs.” It omits core semantic documents needed to interpret frozen objects and behaviors, including at least `ONE_TO_ONE_SESSION_MANAGEMENT.md`, `ASYNC_KEY_MATERIAL.md`, `INVITES_INTROGATE_AND_SPAM.md`, and `DECENTRALIZED_ABUSE_MODERATION.md`. Schemas alone do not define those authorization, lifecycle, provider-observation, or admission semantics. `cmd_verify` checks that listed files are present/hash-correct/parseable, but it has no expected normative manifest or transitive-reference completeness check, so a bundle can verify perfectly while omitting part of the normative protocol needed by an independent implementer. This conflicts with §3's “normative schemas/specs” self-containment goal.

Proposed fix: Replace the basename heuristic with a versioned, explicit profile manifest of every normative artifact required to interpret that profile, including all transitive normative semantic references. Have `cmd_verify` require every manifest member and reject unresolved normative references (or maintain a generated closure with checked exceptions). Add a tabletop/offline reconstruction step in which an implementation reviewer starts from the bundle only and can locate the rules for every frozen W0 object/provider interaction. Release signatures/authentication can remain a pre-release criterion as scoped by this review; this finding is about semantic completeness now.

Resolution:

## 6. ADR-0135 — OM-Crypto-v1 construction pinning

**Verdict: reject.**

The composite signature byte layout itself is sound as a construction choice: profile 1 fixes the component algorithms and exact lengths, both components sign the same domain-separated DS-SIG bytes, exactly one signature entry is allowed for the signer/profile/role, and acceptance requires **both** Ed25519 and ML-DSA-65 halves. This prevents a verifier from treating either half as an independently sufficient signature or silently stripping one algorithm. Certifying the X25519 identity key and the composite signing key together in the ACS-committed DeviceCredential, then requiring dual-signed recipient prekey material, is a reasonable certificate-style replacement for PQXDH's XEdDSA prekey-signature binding.

It is not literally the same proof as PQXDH's original XEdDSA construction: the security argument becomes a composition of (a) trusted ACS/DeviceCredential binding, (b) unforgeability of the required composite device signature, and (c) PQXDH key-establishment assumptions. That change is acceptable in principle, but it only yields party authentication if both peers' identity public keys are actually tied to the intended OM identities at the corresponding acceptance boundary.

F-4 establishes that the session-manager layer currently does not supply the initiator binding that V-001 delegates upward. The concrete construction has the reciprocal half of the same gap:

### F-9 — ADR-0135 authenticates the recipient binding but leaves responder-side `IK_A` authority outside the construction

Severity: blocker

Where: `decisions/ADR-0135-om-crypto-v1-construction-pinning.md` §2–§4; `verification/prototypes/v001-crypto/README.md` “Not covered”; `verification/prototypes/v001-crypto/rust/core/src/store.rs` `receive` / `decrypt` / `accept_initial`; `decisions/ADR-0131-core-kernel-final-pre-engineering-hardening.md` rules 1 and 8.

Problem: ADR-0135 §2 provides a concrete ACS → DeviceCredential → dual-signed AsyncKeyBundle chain for the **recipient** `IK_B`, and initiators exercise it before session setup. On the responder side, the initial envelope carries `IK_A`; the prototype only checks that the envelope sender key equals the PQXDH header's `IK_A`, derives/decrypts the PQXDH/DR state, inserts a session keyed by that X25519 key, and can return plaintext. The API is not given the initiator's trusted ACS/DeviceCredential/DeviceID and does not prove that `IK_A` belongs to an active authorized peer device. The README expressly says this is outside V-001 and belongs to V-003/IntroIdentityBinding, but F-4 shows that those artifacts do not currently instantiate the missing wire-to-device proof. As a result the construction is asymmetric at the exact party-authentication boundary: possession of a mathematically valid initiator identity key is not yet the same thing as proof that the key is the intended OM peer device. PQXDH's party-authentication guarantee depends on authentication of identity public keys; the certificate-style replacement is sound only if the responder actually applies the sender certificate/ACS binding before attributing the session.

Proposed fix: Extend the construction boundary (or a mandatory, explicitly referenced session-manager precondition) so `accept_initial` cannot release plaintext or commit a new session until `IK_A` is matched to a validated sender DeviceCredential in an acceptable trusted sender ACS view. Specify whether this mapping is established by a pre-fetched verified sender bundle after reciprocal IntroIdentityBinding or by authenticated evidence referenced in the initiation, and pin the mapping to the relationship/account being contacted. Exercise that same input in the V-001 responder path and add negative KATs for unknown, substituted, revoked, quarantined and stale initiator credentials. Document the adapted PQXDH authentication argument as a composition of ACS/credential authenticity + mandatory dual-signature verification + PQXDH. This finding **reopens V-001** under the review brief.

Resolution:

No finding in this pass reopens V-005.

## 7. Claude-authored design since 2026-09-24 — merged architecture review

**Verdict: reject.**

I reviewed ADR-0110..ADR-0130 and ADR-0135 with the frozen CDDL/registries and the V-series evidence relevant to the core-kernel queue, concentrating on authority conservation (“where did authority go?”), contradictory normative rulings, and places where two conforming implementations could make different security decisions.

I did not find an additional independent frozen-CDDL collision or V-005 codec defect. The two cross-cutting findings that are dispositive for this merged design item are intentionally **not duplicated as new remediation tickets**:

- **F-3:** the lift-quarantine authorization prose depends on initiator provenance the canonical ACS does not contain, so honest verifiers can read the rule differently.
- **F-4 / F-9:** V-003 assumes an authenticated sender DeviceID while ADR-0135/V-001 accepts inbound `IK_A` without instantiating that mapping at the responder boundary, so honest clients can differ on whether an unknown initiator identity key is attributable to an authorized peer DeviceIdentity.

The other findings (F-1/F-2/F-5/F-6/F-7/F-8) are evidence/model/descriptor mismatches that should be repaired before their respective independent-review criteria pass, but they do not require changing OM's foundational anti-capture philosophy.

## Gate impact / executor handoff

- V-002 independent review: **not satisfied** until F-1..F-3 are resolved and the corrected model/evidence is rerun.
- V-003 independent review: **not satisfied** until F-4 is resolved at both normative and executable boundaries.
- V-015 independent review: **not satisfied** until F-5 is resolved.
- V-016 independent review: **not satisfied** until F-6 and F-7 are resolved or the evidence boundary is explicitly narrowed with appropriate standards-compliant coverage elsewhere.
- V-017 independent review: **not satisfied** until F-8 is resolved.
- V-001: **reopened by F-9** because ADR-0135 / the concrete responder construction leaves initiator identity binding outside the acceptance path.
- V-005: **remains passed**; I found no codec/frozen-schema finding that reopens it.
- `core_kernel`: remains ineligible for unlock until the executor/integrator applies accepted findings and obtains passing evidence under the repository's lifecycle rules.
