# Independent architecture review of drop `run10A-v1-boundary-closed`

- **Date:** 2026-09-28 23:00 PHT
- **Reviewer:** Claude Opus 5.5 (Anthropic), acting as the external architecture reviewer
- **Input:** `om-design-20260928-2200-PHT-run10A-v1-boundary-closed.zip`, SHA-256 `904cb78bbf44e8c0493aac873605c1ed1c745040009ad4dd195bd815f321893c`, imported as `checkpoint/20260928-2200` (base `0351ca2`)
- **Changes made by this review:** [ADR-0154](../../decisions/ADR-0154-verifier-timer-runtime-segments-and-web-verifier-liveness.md), [ADR-0155](../../decisions/ADR-0155-non-critical-extension-validity-and-presentation-edit-closure.md), [ADR-0156](../../decisions/ADR-0156-engineering-handoff-closure-lan-binding-and-later-wave-wire-freeze.md) and their ledger, registry, prose, validator and evidence updates
- **Independence:** the drop's ADR-0140–0153 chain is OpenAI-authored, so this is a different-vendor design review of it. ADR-0154–0156 are Anthropic-authored and need a non-Anthropic reviewer under ADR-0117. This review does not mark any gate's independence record satisfied.

## 1. Executive result

**Architecture ready for engineering after this review** for the three W0 scopes (`core_kernel`, `reference_client_ui`, `provider_reference`). No blocker was found. The four MAJOR defects were corrected in the design (three fully, one converted into a tracked, scoped design item), and none of them required a W0 wire change.

Two qualifications keep this from being a blanket "ready":

1. **Offline-local LAN delivery needs one bounded design pass.** It stays in V1, but its wire binding never existed. It is now design item A-055, which blocks only the new W1 scope `local_delivery`. `design_open` is therefore **1**.
2. **Later-wave scopes must each freeze their wire objects before authorization.** They already needed this in spirit. It is now an enforced gate criterion (ADR-0156 §2).

"Ready for engineering" is a design verdict. Engineering authorization still needs each scope's pre-engineering gates, which remain open for independent review and tool-complete evidence. See §7.

## 2. Method and coverage

Read in full: ADR-0144–0153, the Run 10A audit, SNAPSHOT/AGENTS status, `EVENT_MODEL`, `MESSENGER_BASELINE_V1`, `MULTI_DEVICE_SYNC`, `LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE`, `CALLS_AND_REALTIME_MEDIA`, `GROUP_COMMIT_AND_STATE_CONVERGENCE`, `RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK`, the W0 `events`/`common` CDDL, the time and extension registries, and the recovery-timing sections of `RECOVERY_AND_BACKUP_POLICY`, `ACCOUNT_CONTINUITY_AND_CONTROL`, `TIME_AND_EXPIRY_SEMANTICS` and `SECURITY_INVARIANTS`. Also read in full: ClientHost §9, the validator's scope-unlock logic and the design-closure checker.

Targeted checks: time/skew consistency across the registry, a sweep for stale superseded wording, gate criteria for wire-freeze coverage, and registered-extension handling in all four extension checkers and both V-005 strict codecs.

Not re-derived: the security of the cryptographic constructions (PQXDH binding, sealed-delivery HPKE, MLS use, SFrame). Those stay with the specialist-review gates. Other architecture documents were sampled for the cross-cutting questions below rather than read line by line.

Evidence rerun here: the full portable phase-1 suite (44/44 producers, 14 s; the earlier "monolithic timeout" was a limitation of the previous sandbox), and the full V-005 runner without benchmarks (`all_checks_pass=True`, byte-identical to the committed result apart from the skipped V-022 benchmark block, which was left as committed).

## 3. Findings

Severity follows the brief: BLOCKER, MAJOR, MINOR, NOTE. "Wire" means a change to accepted W0 bytes.

| ID | Sev. | Affected | Conflict / ambiguity | Failure scenario | Resolution (applied) | Wire | ADR | V1 scope | Evidence afterward |
|---|---|---|---|---|---|---|---|---|---|
| F-01 | MAJOR | ADR-0147 §5, ADR-0131 §6, `TIME_AND_EXPIRY_SEMANTICS`, `RECOVERY_AND_BACKUP_POLICY`, time registry, SECURITY_INVARIANTS A3 | ADR-0147 credits only "durably measured monotonic runtime" per "runtime/boot" but never defines a segment. Browsers have no monotonic clock that survives a page close. | A PWA contact that opens OM ten minutes a day needs ~432 days to credit a 72 h recovery delay, so a recovered user cannot resume messaging with PWA contacts. Recovery is on by default (ADR-0111) and the PWA is the first V1 client. Two native implementations could also differ by days, depending on whether process death breaks a segment. | ADR-0154: a segment is one OS boot session on a sleep-inclusive clock. `process_only` (web) verifiers under Recommended may credit wall-clock gaps forward of a durable high-water mark and must pass a final authenticated veto sweep. Hardened/Maximum stay strict. Residual is recorded as T-132. | No | ADR-0154 | none | V-002 criterion evidenced (8/8 scenarios, 6/6 mutations). Needs non-Anthropic review, and V-021 must confirm `boot_session` per OS. |
| F-02 | MAJOR | ADR-0149 §1, ADR-0151 §1, `PROTOCOL_EXTENSIBILITY`, [`om-cbor-profile.json`](../../spec/registries/om-cbor-profile.json), both V-005 strict codecs | A schema-invalid *known* non-critical extension rejects the whole event, while a client that predates the registration ignores it and accepts. | From the first post-V1 non-critical registration, a crafted event is visible on old clients and rejected on new ones, including on two devices of the same account. This breaks the event model's Gate-B convergence criterion, and shipped clients cannot be fixed retroactively. | ADR-0155 §1: for non-critical registrations, object/schema failures ignore the extension and the event stays valid. A wire-criticality mismatch, or any failure on a critical registration, still rejects. | No (validity rule only; no valid encoding changes) | ADR-0155 | none | V-005 criterion evidenced: 29/29 extension and 35/35 rich-list Python↔Node vectors; V-005 corpus rerun unchanged. Needs non-Anthropic review. |
| F-03 | MINOR | ADR-0149 §2, ADR-0150 §2, `MESSENGER_BASELINE_V1` | Nothing said whether a create-event location card or list annotation survives a `message.edit`. The `geo:` fallback only had to *contain* the point's token. | After an edit removes the token, new clients keep showing the old card while old clients show the edited text. A fallback naming two points shows one card on new clients and both points on old ones. A create's list annotations get applied to unrelated edit blocks. | ADR-0155 §2–§3: voice and location extensions are create-only; the card is suppressed once the materialized text fails the check; the fallback must name exactly one canonical point; list annotations bind only to the carrying event's text. | No | ADR-0155 | none | New vectors added to the V-005 corpora; covered by the same criterion. |
| F-04 | MINOR | `MESSENGER_BASELINE_V1`, `EVENT_MODEL` | "If no authorized device is online, nothing is sent" is undefined for overdue items, and the clock and epoch a scheduled item carries are unspecified. | One client silently drops overdue items while another sends them with a backdated HLC that wins LWW registers or misorders the timeline. | ADR-0156 §3: device-local, profile-bound queue; clock/epoch/authorization/expiry taken at actual send; sent late with a sender-side marker; discarded on lost eligibility. | No | ADR-0156 | none | R-344 → `conformance:ui_intents` cases before `reference_client_ui` release. |
| F-05 | MAJOR | ADR-0150 §1, `LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE`, [`discovery.cddl`](../../spec/cddl/discovery.cddl) (`lan_rendezvous` opaque), scope table | V1 promises "interoperable local-mode semantics", but no channel, framing, route descriptor, rendezvous KDF/label/epoch or offline first-contact path exists. The design never says whether LAN carries `SealedDeliveryEnvelope` (bound to a relay route) or the bare inner envelope, which starts with the sender's device key (A-054). The feature also sat in no scope. | Two clients ship incompatible LAN protocols, or one ships a clear-sender LAN path that gives every café-Wi-Fi observer a stable device correlator, contradicting ADR-0142 and LAN §7. | ADR-0156 §1 fixes four rules now: no clear sender, rotating LAN identifiers, no LAN-only relationship path, and a registered derivation. The concrete binding is open design item **A-055**, blocking only the new W1 scope `local_delivery` (gate V-024). Route-independent dedupe/backfill stays in `core_kernel`. | No (W0); future binding is additive | ADR-0156 | stays V1, re-sequenced to W1 | A-055 resolution ADR, then V-024 (frozen CDDL/vectors, observer trace, offline first-contact model, browser feasibility). |
| F-06 | MAJOR | GROUP_COMMIT §7, `CALLS_AND_REALTIME_MEDIA`, gates V-004/7/8/10/11/12/13, ADR-0139 | Later-wave wire objects exist only as logical JSON schemas, which ADR-0139 says are not wire authority. No later-wave gate required frozen CDDL, even where the prose says the scope "is not frozen until" the encoding exists. The group-call key channel to joined co-members without a 1:1 relationship is "pairwise … or an equivalent HPKE channel". | A later scope is authorized with engineers inventing encodings for pin/poll/topic/moderation bodies, group control, call signaling, representation/provenance keys 13/14, plugin manifests and custom emoji. Two independent implementations then need a breaking migration. | ADR-0156 §2: each later-wave gate gains a criterion requiring frozen CDDL, registered fields/labels and two-codec vectors for its named objects. `calls` must also choose the non-contact key channel and get specialist cryptographic review. | No | ADR-0156 | none | Per-scope freeze work before each scope's authorization. The calls key channel needs a specialist cryptographer. |
| F-07 | MINOR | ADR-0139, ADR-0148 §2, `MULTI_DEVICE_SYNC` | The account-private sync and HistoryBundle codecs are implementation-owned, with no namespacing. | A third-party client linked to the same account reuses a record-type name with a different encoding and misparses drafts, preferences or history. | ADR-0156 §4: an implementation/codec identifier is bound inside the authenticated framing; unrecognized records are ignored whole; cross-implementation sync is disclosed as unavailable, not silently divergent. | No | ADR-0156 | none | R-345 → V-003. |
| F-08 | MINOR | [`repository_contract.json`](../../machine/repository_contract.json) `engineering_unlock_requires`, [`validate_architecture.py`](../../tools/validate_architecture.py), RELEASE_WAVES §5.3 | The contract and validator let *any* open blocking design item block *every* scope. RELEASE_WAVES §5.3, the queue's per-item `scopes` field and the generated scope table all say only the listed scope is blocked. | Any tracked later-wave design item (such as A-055) would silently freeze `core_kernel` authorization, or tempt someone to close it prematurely. | Validator and contract now block only the listed scopes; an item with no scopes still blocks everything. | No | ADR-0156 | none | Validator run (done). |
| F-09 | MINOR | [`repository_contract.json`](../../machine/repository_contract.json) snapshot, SNAPSHOT.md, AGENTS.md, [`UI_UX_SYSTEM.md`](../architecture/ux/UI_UX_SYSTEM.md) | The contract snapshot was left at 2026-09-27 08:42, so `om_drop import` tagged the drop `checkpoint/20260927-0842` until re-imported with `--stamp`. SNAPSHOT had dated runs stacked above its own title and a "State" block quoting superseded counts. Two lines still said "v1 shows one identity" despite ADR-0148. | Mislabeled history, and readers acting on stale counts or the retired single-identity wording. | Contract snapshot updated; SNAPSHOT restructured with a current State; stale lines fixed. Future drops must update [`machine/repository_contract.json`](../../machine/repository_contract.json) `snapshot`. | No | — | none | none |
| N-01 | NOTE | ClientHost §9, T-074 | A shared ClientHost can ship code to every T0/T1 user it serves. | Capture point for users without their own origin. | Correctly disclosed and bounded: plural hosts, transparency-logged releases, client digest check, named origin, leaving = enrollment. Acceptable. | — | — | — | V-023 / release transparency. |
| N-02 | NOTE | ADR-0140/0142, `CRYPTOGRAPHY_AND_AGILITY` | OM-Crypto-v1 pins `draft-ietf-hpke-pq-05` bytes. | The draft may change before RFC. | Correct handling: pinned bytes under CryptoProfile 1, and any RFC delta becomes a new CryptoProfile. Needs specialist review and tracking of the standard. | — | — | — | V-001 independent cryptographic review. |
| N-03 | NOTE | ADR-0155, T-133 | Sender-asserted criticality. | A future critical registration sent with the bit cleared is seen as unknown non-critical by pre-registration clients. | Inherent to criticality flags; recorded as a residual. | — | — | — | — |
| N-04 | NOTE | ADR-0131 §5, `EVENT_MODEL` groups, V-019 | Deferred SPQR/Triple Ratchet and PQ MLS. | Hidden V1 assumptions. | None found. V1 claims exactly "PQ handshake protection", group non-deniability is explicit, CryptoProfile floors prevent downgrade, and V-019 evidences ReInit feasibility. | — | — | — | — |
| N-05 | NOTE | ADR-0147/0154 | Recovery remains deliberately "false-late" for strict verifiers. | Hardened/Maximum verifiers and native verifiers across reboots accept slowly. | Accepted, principled tradeoff; controller co-signature remains the fast path. | — | — | — | — |

**Remaining after this review:** BLOCKER 0. MAJOR 1, tracked as A-055; it blocks only `local_delivery` and not any W0 scope. MINOR 0 open. NOTE 5.

## 4. V1 boundary verdict

- **Promote nothing.** I independently agree with Run 10A. SMS/MMS/RCS, mutual-contact PSI, LegacyPolicy activation and additional codecs all have stable V1 sockets (TransportAdapter/provenance, recognition-evidence model, dormant critical ACS extension, content-pack extension points). Deferring them costs no retrofit. No post-V1 item's omission forces a breaking V1 redesign; the closest candidate, PQ ratchet/MLS, is covered by CryptoProfile agility plus V-019.
- **Move nothing out.** Offline-local LAN delivery is the only V1 item whose inclusion outran its specification (F-05). Its architectural value (transport-neutral EventID dedupe and blob backfill) is already in the W0 core, so it stays a V1 commitment. Only its binding is re-sequenced to W1 behind A-055. The desktop/Android/iOS reference-family promotions add verification burden but no protocol risk, because native shells stay non-privileged PlatformAdapters.
- **W0–W4 is coherent after F-06 and F-08.** Waves order delivery of one semantic contract. Every scope now has to freeze its own wire objects before authorization, and design blockers block only the scopes they name.

## 5. Hidden-coupling audit

No mandatory OM-operated service, identity authority, global username registry, central relay, push or recovery service was found. Couplings that remain, all disclosed and replaceable:

1. **Shared ClientHost to T0/T1 users** (N-01): code-delivery trust, mitigated by plurality and transparency.
2. **Passkey RP ID to distribution origin:** passkey-only recovery is "distribution-bound". The design keeps an origin-independent method available.
3. **iOS wake to a distribution-operated APNs gateway:** a platform fact. `none` is valid and push never carries authority or plaintext.
4. **Release automation to two independent transparency monitors:** missing observations pause auto-deploy only, and never disable clients.
5. **T0 sponsored mailbox / Cloudflare first T1:** bounded by the required second non-cloud reference provider (monoculture = 0M risk).
6. **Account-private sync to one implementation:** drafts and preferences are soft-locked to one client family. This is non-authoritative, now disclosed rather than silently divergent (F-07).
7. **OM-Crypto-v1 to the HPKE-PQ draft bytes** (N-02).
8. **Group calls to an unspecified non-contact key channel** (F-06), gated before `calls`.
9. **LAN delivery** (F-05), gated before `local_delivery`.

## 6. Engineering-ambiguity audit

Every remaining place where an engineer would otherwise have to make a protocol or architecture decision:

| Place | Owner now | Blocks |
|---|---|---|
| LAN wire binding (route descriptor, channel/framing, rendezvous KDF/label/epoch, offline first-contact key serving, chunking, K) | A-055 + V-024 | `local_delivery` |
| W1–W4 wire encodings (GroupControl*, pin/poll/topic/moderation bodies, Space/Publication, call signaling and sender-key package, ServicePrincipal/RepresentationGrant/key 13, BridgePrincipal/key 14, plugin manifest, custom emoji/reaction_asset) | ADR-0156 criteria on V-004/7/8/10/11/12/13 (V-014 archive already) | each later scope |
| Group-call key channel to non-contact co-members | V-007 + specialist review | `calls` |
| Which OS versions provide `boot_session` elapsed clocks | V-021 platform matrix (ADR-0154) | `reference_client_ui` evidence, not design |
| Account-private sync/HistoryBundle codec | intentionally implementation-owned, namespaced (F-07) | none |
| Privacy-preset defaults, UI tokens/framework, IndexedDB vs OPFS | declared measurement choices behind frozen abstractions | none |
| GroupProfile calibration (`sequencing_timeout`, `past_epoch_window`) | defaults given; V-004 calibrates | `groups` |

For the W0 scopes I found no remaining place where two careful implementers following the documents would build incompatible wire behavior.

## 7. Closure recommendation

**Design changes required before W0 engineering:** none remaining. F-01–F-04 and F-07–F-09 are applied, and F-05/F-06 no longer touch W0.

**Before `local_delivery`:** resolve A-055 with an ADR that satisfies ADR-0156 §1, then evidence V-024.

**Before each later-wave scope:** satisfy its ADR-0156 wire-freeze criterion. For `calls`, also obtain specialist cryptographic review of the non-contact key channel.

**Non-design steps still required before any scope is authorized:**

1. A non-Anthropic review of ADR-0154, ADR-0155 and ADR-0156 (ADR-0117).
2. The consolidated core_kernel independence review ([`BRIEF-2026-09-27-final-core-kernel-independent-review.md`](../../verification/reviews/BRIEF-2026-09-27-final-core-kernel-independent-review.md)). This review is a design review of the OpenAI-authored chain, not that brief's formal gate sign-off.
3. Tool-complete reruns where the brief asks for them. TLC was not rerun in this pass.

**Gates that legitimately need implementation, environment or specialist evidence:** V-001 independent cryptographic review and production-build exclusion; V-006 passkey/platform recovery matrix; V-015 real packet/provider/browser traces; V-017 continuity drill; V-020 second independent provider; V-021 per-OS `boot_session` confirmation; V-022 performance; V-007 runtime calls; V-024 real-LAN interop; every gate's `pre_release_criteria`.
