# Architecture review R4: providers, network, and cross-cutting consistency

- **Reviewer:** Claude Opus (claude-opus-5-5), independent reviewer under ADR-0117
- **Date:** 2026-09-24
- **Base commit:** b33c518
- **Scope:** (A) `docs/architecture/providers/` and `docs/architecture/network/`, every doc read in full. (B) Cross-cutting
  consistency of `docs/constitution/`, `docs/overview/`, `docs/threat-model/` and `docs/process/`, checked against each
  other and against [`machine/repository_contract.json`](../../machine/repository_contract.json), [`requirements.jsonl`](../../machine/requirements.jsonl), [`requirement_traceability.json`](../../machine/requirement_traceability.json),
  [`threat_register.jsonl`](../../machine/threat_register.jsonl), [`architecture_design_queue.jsonl`](../../machine/architecture_design_queue.jsonl), [`architecture_verification_gates.jsonl`](../../machine/architecture_verification_gates.jsonl),
  [`public_release_gates.jsonl`](../../machine/public_release_gates.jsonl), [`decisions.jsonl`](../../machine/decisions.jsonl), [`resolved_questions.jsonl`](../../machine/resolved_questions.jsonl), `spec/protocol`, [`spec/cddl/delivery.cddl`](../../spec/cddl/delivery.cddl)
  and [`spec/metadata/w0-metadata-exposure-descriptors.json`](../../spec/metadata/w0-metadata-exposure-descriptors.json).
- **Brief:** reviewer R4. Priority: security and anti-capture flaws ("Where did authority go?"), then contradictions,
  then underspecification that blocks engineering, then AGENTS.md/constitution violations, then minor stale references.
  Claude-authored text (ADR-0110+, sections headed "2026-09-24 ...", [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../../docs/architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md)
  §4.1, §4.2, §6.4 and §8.6, [`PROVIDER_CONFORMANCE_SUITE.md`](../../docs/architecture/providers/PROVIDER_CONFORMANCE_SUITE.md), the W0 MetadataExposureDescriptors) is context only.
  Identity/crypto/discovery/delivery/storage (R1), groups/events/safety/calls (R2) and ecosystem/client/ux (R3) are
  not duplicated here.

**Verdict:** The anti-capture intent is consistent and well stated, and the ADR-0102 Home tiers fix the worst
onboarding problem. But three authority paths escaped the "Where did authority go?" test: the executable code a T0
user runs, the Deploy-to-Cloudflare template path that bypasses the release trust root, and the SecurityAdvisory key
that can partition the network. Several network and censorship claims, and the ClientHost update lifecycle, are traced
to gates that do not test them. `provider_reference` and `reference_client_ui` should not be unlocked until F-R4-1,
F-R4-2 and F-R4-7 are resolved.

## Findings

### F-R4-1 — T0 (and T1-by-default) users have no specified client origin, so "Alice can't read your messages" is unbacked

Severity: blocker
Where: [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../../docs/architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md) §3.1 lines 37-46, §3.3 lines 62-64, §7 lines 138-150;
[`SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md`](../../docs/architecture/providers/SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md) line 118 ("Reference PWA therefore also requires minimal self-owned
ClientHost"); [`CLIENTHOST_UPDATE_LIFECYCLE.md`](../../docs/architecture/client/CLIENTHOST_UPDATE_LIFECYCLE.md) §1; [`OPERATORLESS_CORE_AND_ECOSYSTEM.md`](../../docs/constitution/OPERATORLESS_CORE_AND_ECOSYSTEM.md) §3 lines 65-78;
[`machine/requirements.jsonl`](../../machine/requirements.jsonl) R-161, R-046.
Problem: The PWA security model assumes each user owns their ClientHost origin. T0 removes the user's
infrastructure account, and the W0 reference client is PWA-first (ADR-0044). The T0 flow never says which origin
serves Bob's executable code. There are only three possibilities, and each moves authority somewhere undeclared:
(a) the sponsor's origin, which makes Alice part of Bob's endpoint trust base, so the §3.3 promise "Alice can't read
your messages" is false; (b) a project-operated PWA origin, which becomes one hosting account whose compromise is
endpoint compromise for most users, a mandatory project-operated runtime service in practice (tension with R-046,
ADR-0043) that no PR gate or custody entry covers; (c) native apps only, which makes app stores the zero-signup
gatekeeper. R-188 fixes this for T1 only, and only as a Claude-added rule. No threat entry, MetadataExposureDescriptor or
V-018 flow covers the T0 client origin.
Proposed fix: Add a HOME_ONBOARDING §3.5 "Where a T0 user's client comes from". Extend R-188 to T0 (a sponsor never
serves the invitee's ClientHost unless the invitee explicitly opts in). If a project-controlled PWA origin exists,
declare it a project-controlled service under PR-001/PR-005/PR-012, with release-transparency monitoring, and require
at least one independently operated mirror origin and a native option in the same invite, so the project origin is
never the only path. Add a threat entry (T0 client-origin compromise), make §3.3's wording conditional on the chosen
origin, and add the origin choice to V-018's onboarding flow.
Resolution: accepted — HOME §3.5 (never the sponsor's origin by default; native + ≥2 independent origins; a project origin is a declared PR-001/005/012 service, never alone), §3.3 wording conditional on origin; R-260, T-080, V-018 criterion; ADR-0130 rule 1.

### F-R4-2 — The preferred provisioning path bypasses the release trust root, and OAuth scoping is overstated

Severity: blocker
Where: [`CLOUDFLARE_REFERENCE_DEPLOYMENT.md`](../../docs/architecture/providers/CLOUDFLARE_REFERENCE_DEPLOYMENT.md) line 20, lines 79-90; [`PROVISIONING_AND_BOOTSTRAP.md`](../../docs/architecture/providers/PROVISIONING_AND_BOOTSTRAP.md) §4 line 48,
§9 line 110, §10 lines 114-126; [`SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md`](../../docs/architecture/providers/SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md) §3-4; [`CLIENTHOST_UPDATE_LIFECYCLE.md`](../../docs/architecture/client/CLIENTHOST_UPDATE_LIFECYCLE.md) §2, §4;
AGENTS.md 21:32 rule "Do not ship a public binary whose trust reduces to one mutable CI workflow"; R-037;
[`PROJECT_STEWARDSHIP_AND_CONTINUITY.md`](../../docs/constitution/PROJECT_STEWARDSHIP_AND_CONTINUITY.md) §5.
Problem: Deploy-to-Cloudflare is named the *preferred* path (ADR-0046). It clones a public Git repository at
click time and builds it in the user's account. That deployed code (Home, and for T2 PWA users the ClientHost) is
bound to a mutable forge branch plus build-time dependency resolution. It is not bound to a TUF-signed release, so
§3/§4's threshold root, provenance and rollback protection never apply. There is also no update path: thousands of
cloned repositories do not receive security fixes, which undercuts the ADR-0104 straggler story. Separately,
§4 claims OAuth can request "narrowly scoped permission to create/update only needed resources", but §9 names
`Workers Scripts Write`, which is account-wide. Token custody (PWA storage, and so the ClientHost origin) is
unspecified, and a project-owned OAuth client registration is not in the custody map.
Proposed fix: Require every provisioning method, including templates, to deploy a signed release artifact pinned by
digest and verified against the distribution's TUF targets before activation. A template's first step is a verifier,
and it never auto-deploys a mutable branch. Define the update channel for template deployments using the CLIENTHOST
§4 staged flow. Correct the §4 scope claim, specify where OAuth tokens live and how long they last, and add "deployment
template repository" and "OAuth client registration" to the §5 custody map. Add a threat entry and a V-020 or PR-005
criterion ("no deployment path activates code that is not a verified release").
Resolution: accepted — PROVISIONING §4/§10 and CLOUDFLARE: verified, digest-pinned releases only, verifier-first template that stays as update controller, account-wide OAuth scopes stated, token custody; custody map (STEWARDSHIP §5); R-261, R-262, T-081, PR-005 criterion, V-020 criterion, PC-I01..03; ADR-0130 rule 2.

### F-R4-3 — The SecurityAdvisory signing key can partition the network and is not a managed trust root

Severity: major
Where: [`VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md`](../../docs/architecture/providers/VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md) §5 line 47, §6 lines 51-59;
[`spec/protocol/security-advisory.logical.schema.json`](../../spec/protocol/security-advisory.logical.schema.json) (`issuer_key_ref`, `recommended_floor.user_override_permitted`,
no expiry); [`SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md`](../../docs/architecture/providers/SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md) §4 lines 42-58; [`PROJECT_STEWARDSHIP_AND_CONTINUITY.md`](../../docs/constitution/PROJECT_STEWARDSHIP_AND_CONTINUITY.md) §5;
[`PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md`](../../docs/constitution/PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md) §6, §11; threat T-035.
Problem: Clients subscribe to their own distribution's advisory channel by default, and an accepted
`recommended_floor` puts every relationship below it into "Needs update". Profile-class advisories "may forbid
overrides". Whether acceptance is automatic is unspecified. A distribution's advisory key can therefore, with no user
action, cut its users off from every peer on another CryptoProfile or protocol generation. If `min_crypto_profile`
names a profile only that distribution implements, it becomes a lever for LONGEVITY §11 dominant-client capture. The
key is outside the TUF role hierarchy, absent from the custody map, has no threshold, and the schema has no
expiry. So one stolen online key is a network-wide denial-of-service switch, which is close to the kill switch
ADR-0104 rejects. T-035 models only stragglers, not a malicious floor.
Proposed fix: Delegate advisory signing from the distribution's TUF root, and require the root threshold for any
peer-affecting floor (crypto profile or generation). Allow a floor to name only a profile that is `preferred` in the
public registry and has at least two independent implementations. Separate `min_release` (own clients only) from
peer-affecting floors in the schema. Add `expires_at`. Make "user override permitted" the default unless the advisory
carries threshold signatures. Add the key to the custody map and add a threat entry "malicious or compromised
floor-raising advisory".
Resolution: accepted — advisory key is a TUF role delegated from the root (SUPPLY_CHAIN §4); VULNERABILITY §6.1 peer floors need the root threshold, preferred profile with ≥2 implementations, own_release_floor split, expires_at, override by default; schema updated; custody map; R-263, R-264, T-082; ADR-0130 rule 3.

### F-R4-4 — Co-locating the discovery publisher with the relay breaks non-composability, and T0 §3.3 overclaims

Severity: major
Where: [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../../docs/architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md) §2 line 21 (Home = relay + IntroGate + discovery publisher),
§3.3 line 62; [`PROVISIONING_AND_BOOTSTRAP.md`](../../docs/architecture/providers/PROVISIONING_AND_BOOTSTRAP.md) §3 line 40, §5 lines 70-78; [`CLOUDFLARE_REFERENCE_DEPLOYMENT.md`](../../docs/architecture/providers/CLOUDFLARE_REFERENCE_DEPLOYMENT.md) lines
24-29; [`PRIVACY_GOVERNANCE_AUTHORITY.md`](../../docs/constitution/PRIVACY_GOVERNANCE_AUTHORITY.md) "Non-composable" lines 41-43; [`METADATA_THREAT_MODEL.md`](../../docs/threat-model/METADATA_THREAT_MODEL.md) §18 line 328;
[`SYSTEM_ARCHITECTURE.md`](../../docs/overview/SYSTEM_ARCHITECTURE.md) lines 119-121; [`resolved_questions.jsonl`](../../machine/resolved_questions.jsonl) Q-014 (ContactDescriptor "carries AccountID").
Problem: In the reference design, one Worker serves the signed ContactDescriptor, which carries AccountID per Q-014,
and also runs the IntroGate and the mailbox. The operator of that account (the user's cloud vendor, the T1 host or the
T0 sponsor) therefore links AccountID to every mailbox deposit and drain by construction. That contradicts the
non-composability rule, the METADATA §18 UI example ("This relay can see ... but not your OM account ID"), and the
non-collusion assumptions in the W0 descriptors ("resolver relay and publisher do not collude"), which the reference
deployment violates by design. §3.3 also claims the sponsor's Home cannot see "the identities behind other contacts'
opaque deposit capabilities". That fails whenever a contact is also on that Home: the same client IP or session
drains one mailbox and deposits to another, so the edge is observable.
Proposed fix: Either (a) require role separation in the reference Home (distinct Workers/DOs, hostnames and
credentials for publisher, IntroGate and mailbox, with no shared request logs) and add a conformance case, or (b)
declare "same-operator roles are colluding by construction" in HOME §2, the MetadataExposureDescriptors and the UI
copy, and remove the METADATA §18 example claim for co-located Homes. Rewrite §3.3 to say the sponsor can see
contact edges among co-resident users.
Resolution: accepted (option b) — same-operator roles declared one observer in HOME §2, METADATA §18/§20, SYSTEM_ARCHITECTURE, PRIVACY_GOVERNANCE and the W0 descriptors (descriptor_mailbox_linkage, contact_graph_edge); §3.3 names co-resident edges; per-role separation kept as hygiene (PC-B06); R-265, T-083; ADR-0130 rule 4.

### F-R4-5 — The T0 sponsor holds an unmodeled proximate-adversary position

Severity: major
Where: [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../../docs/architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md) §3.3-3.4 lines 62-72; [`docs/threat-model/README.md`](../../docs/threat-model/README.md) line 144
(proximate adversary); [`threat_register.jsonl`](../../machine/threat_register.jsonl) T-022/T-023; R-162.
Problem: The inviter who sponsors a mailbox is often exactly the proximate adversary (partner, parent, employer) that
the 2026-09-24 threat classes introduce. The sponsor sees the timing and volume of every incoming message, and §3.4
lets them revoke the mailbox at will. The client then only "guides the user to another tier", so inbound contact can be
cut instantly with no overlap guarantee. T1 eviction gets at least 30 days of overlap (R-187), but T0 gets none. No
threat entry covers sponsor surveillance or sponsor-revocation coercion.
Proposed fix: Add a threat entry (sponsor as proximate adversary). Give sponsor revocation a minimum overlap/forwarding
window, as for T1 eviction (enforced by the Home and tested by conformance). Have the client add a second route that is
not the sponsor's soon after first run, and say so in the §3.3 disclosure. Add a V-018 comprehension item for "your
inviter can see when you get messages and can close this mailbox".
Resolution: accepted — HOME §3.4: 30-day sponsor revocation overlap enforced by the Home (PC-D06), early non-sponsor second route, §3.3 disclosure; threat-model README; R-266, R-267, T-084, V-018 comprehension item; ADR-0130 rule 9.

### F-R4-6 — Provider contracts are too thin for two independent Homes to interoperate

Severity: major
Where: [`IMPLEMENTATION_INTERFACES.md`](../../docs/architecture/providers/IMPLEMENTATION_INTERFACES.md) line 3, lines 12-43, line 90; [`spec/protocol/provider-capabilities.logical.schema.json`](../../spec/protocol/provider-capabilities.logical.schema.json)
(prose placeholders, not a JSON Schema); [`spec/cddl/delivery.cddl`](../../spec/cddl/delivery.cddl) (objects only); AGENTS.md "Core interfaces";
[`PROVIDER_CONFORMANCE_SUITE.md`](../../docs/architecture/providers/PROVIDER_CONFORMANCE_SUITE.md) lines 8-10, 81-85 (context).
Problem: PR-013 requires two reference Homes (Cloudflare and a static binary) that third-party clients can use. But
there is no Home wire API: no deposit/drain/ack/rotate/redeem endpoints, no request authentication, no typed errors
("quota", "moved", "retryable") and no encoding. The conceptual `RelayProvider` has no way to mint or revoke
per-contact deposit capabilities (needed for block), no sponsorship redemption (ADR-0102), no RecoveryRecord store or
fan-out (ADR-0111), no BlobLocationRecord repair (ADR-0075), and no forwarding "moved" state. AGENTS.md lists
`KeyMaterialProvider`, `BackupStore`, `RecoveryStore`, `StoragePool` and `CryptoProvider` as core interfaces, but none
is sketched. `NetworkTransport` (Q-019) has no interface at all. The conformance suite says it drives Homes "only
through public provider contracts" that do not exist, so each Home will define its own and they will diverge.
Proposed fix: Add a normative `spec/protocol/home-api` (operations, auth, CDDL request and response bodies, error
registry, versioning) covering relay, IntroGate, descriptor publishing, sponsorship, recovery records and blob spool.
Turn ProviderCapabilities into a real schema with `durability_class`. Add the missing interfaces and NetworkTransport
to IMPLEMENTATION_INTERFACES. Make the conformance runner target that API. Put the Home API freeze under V-005/V-020
pre-engineering criteria for `provider_reference`.
Resolution: deferred — A-045 (Home API, ProviderCapabilities schema with durability_class, missing interfaces) blocks provider_reference; "Home API frozen" added to V-020; IMPLEMENTATION_INTERFACES lists the missing contracts; R-275.

### F-R4-7 — The ClientHost update lifecycle, the W0 endpoint trust root, has no gate that tests it

Severity: major
Where: [`CLIENTHOST_UPDATE_LIFECYCLE.md`](../../docs/architecture/client/CLIENTHOST_UPDATE_LIFECYCLE.md) §8 "Freeze gate"; [`requirement_traceability.json`](../../machine/requirement_traceability.json) R-095 → [V-006, PR-005],
R-164 → [V-021, PR-005]; [`threat_register.jsonl`](../../machine/threat_register.jsonl) T-019 → [V-006]; [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../../docs/architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md)
§5 item 3 line 120; [`SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md`](../../docs/architecture/providers/SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md) §6.
Problem: V-006 tests DeviceVault wrapping, and PR-005 tests TUF metadata. Neither criterion covers service-worker
lifecycle, staged or atomic ClientHost switch, rollback/freeze, emergency minimum version or compromised-origin
migration, which is the §8 freeze list. R-164's "compare the release digest with ≥2 independent release-transparency
observations" names observers that no document specifies (who runs them, their protocol, their failure behaviour) and
no gate checks. By P-11, these obligations exist only as prose, although they are the main defence for the W0
reference client.
Proposed fix: Add a verification gate (or extend V-006) with the §8 list as `pre_engineering_criteria` for
`reference_client_ui`, and a pre-release criterion for a live update/rollback drill. Specify the release-transparency
log and monitor roles, including that a monitor is optional and non-authoritative. Re-trace R-095, R-164 and T-019 to
it.
Resolution: accepted — new gate V-023 clienthost_update_lifecycle (CLIENTHOST §8 list; live update/rollback drill pre-release) for reference_client_ui; transparency log/monitor roles in SUPPLY_CHAIN §6 (protocol choice A-046); R-095, R-164, T-019 re-traced; R-274; ADR-0130 rule 5.

### F-R4-8 — Censorship-resilience claims are not backed by an interface, a threat entry or a gate

Severity: major
Where: [`CENSORSHIP_AND_NETWORK_RESILIENCE.md`](../../docs/architecture/network/CENSORSHIP_AND_NETWORK_RESILIENCE.md) §1 line 12, §4 line 59, §9; [`BLUEPRINT.md`](../../docs/overview/BLUEPRINT.md) §18A line 150;
[`docs/threat-model/README.md`](../../docs/threat-model/README.md) line 85; [`requirement_traceability.json`](../../machine/requirement_traceability.json) R-021, R-028, R-045 → [V-017] only;
[`public_release_gates.jsonl`](../../machine/public_release_gates.jsonl) PR-013 ("before public-production resilience claims").
Problem: The goal "no single technical or administrative switch should disable the entire OM network" is stated as
architecture, but: (1) no `NetworkTransport` contract exists outside the one-line Q-019 resolution; (2) the register
has no threat for state-level blocking (blocking `*.workers.dev`, vendor IP ranges, SNI filtering, forge or store
removal), since T-020 covers only a vendor's own action; (3) V-017's criteria (continuity bundle, registry checks,
tabletop drills) test none of R-021/R-028/R-045; (4) the V-020 model shows a Cloudflare-wide block leaves about 2% of
relationships reachable without fallbacks, and PR-013 lets a public release ship on one vendor provided it makes no
"resilience claim". Discovery (workers.dev locators, invite landing pages) is a single suffix-level choke point for
T2 users.
Proposed fix: Add NetworkTransport to IMPLEMENTATION_INTERFACES with conformance cases (a transport is an untrusted
byte carrier; failover without semantic change). Add threat entries for network-level blocking and discovery-suffix
blocking. Trace R-021/R-028/R-045 to a gate whose criteria include a blocked-vendor/blocked-suffix simulation. Either
make PR-013 apply to any public production release of the reference distribution, or add a claim-discipline rule
that forbids the §1 and §18A wording until PR-013 passes.
Resolution: accepted — NetworkTransport contract with NT-01..05 (IMPLEMENTATION_INTERFACES); CENSORSHIP §1 claim discipline and §1.1 blocking threats; T-085, T-086; V-020 blocked-vendor/suffix simulation; R-021/028/045 re-traced; PR-013 and BLUEPRINT §18A wording; R-268, R-269; ADR-0130 rule 6.

### F-R4-9 — LAN rendezvous ignores contacts as trackers and advertisement-set fingerprinting

Severity: major
Where: [`LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md`](../../docs/architecture/network/LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md) §2 lines 26-32, §7 lines 67-73; [`ARCHITECTURE_RESOLUTIONS_20260923.md`](../../docs/reviews/ARCHITECTURE_RESOLUTIONS_20260923.md)
Q-011/Q-018 (pairwise tags, 15-minute epochs); [`docs/threat-model/README.md`](../../docs/threat-model/README.md) line 84; [`requirement_traceability.json`](../../machine/requirement_traceability.json)
R-027 → [V-015, PR-008]; no threat-register entry.
Problem: The listed properties treat every contact as benign. Any related party, such as an estranged partner or an
ex-employer who is still a contact, can passively detect the user's physical presence on any shared LAN. Nothing says
LAN rendezvous is off, opt-in or per-contact by default, and nothing says blocking or removing a contact stops
advertising or answering its tags. A device that advertises one pairwise tag per contact also leaks its contact count,
and a tag set that rotates in sync from the same MAC/IP address is linkable across epochs by a passive Wi-Fi observer.
V-015's criteria do not mention LAN, and PR-008 is a platform matrix, not a privacy test.
Proposed fix: Define the default (for example on only for contacts the user marks, and off under Hardened), require
tag retirement on block or relationship end, and choose a querier-initiated or fixed-cardinality design so
advertisement size carries no information. Add threat entries (contact-as-tracker, tag-set linkability) and a
LAN trace criterion in V-015.
Resolution: accepted — LAN §2.1: opt-in per contact, off under Hardened/Maximum, retired on block, fixed-cardinality tags, rotation with MAC; T-087, T-088, V-015 LAN trace criterion; R-270, R-271; ADR-0130 rule 8.

### F-R4-10 — The traceability ledger accepts phantom conformance suites and overloads V-017

Severity: major
Where: [`tools/validate_architecture.py`](../../tools/validate_architecture.py) line 588 (`t.startswith("conformance:")` is always valid);
[`requirement_traceability.json`](../../machine/requirement_traceability.json) (10 `conformance:*` targets such as `security_floor`, `ui_themes`, `identity`,
`discovery`; 17 requirements traced to `V-017` alone, among them R-048, R-075, R-076, R-158); AGENTS.md absolute
rule 14.
Problem: Only one conformance suite is specified ([`PROVIDER_CONFORMANCE_SUITE.md`](../../docs/architecture/providers/PROVIDER_CONFORMANCE_SUITE.md), PC-*), and no requirement traces to
it. The other `conformance:*` targets name suites that do not exist, but the validator accepts them. V-017's criteria
(bundle script, registry checks, tabletop drills, independent rebuild) do not test "no hidden distribution privilege"
(R-048), "implementable from public artifacts" (R-075) or "no mandatory proprietary extension" (R-076/R-158). The
ledger therefore reports full traceability while a large part of the constitution has no proof obligation. That is the
drift P-11 and ADR-0109 exist to prevent. Rule 14 (provider migration and restore require conformance tests) is not
traced to the suite that implements it.
Proposed fix: Add a [`machine/conformance_suites.json`](../../machine/conformance_suites.json) registry. The validator rejects a `conformance:<name>` target
unless the suite is registered with a spec document. Trace R-001/R-052/R-187/R-162 to `conformance:provider`. Split
V-017's catch-all requirements into criteria V-017 actually has, or move them to review targets with named
independent reviewers.
Resolution: accepted — machine/conformance_suites.json (specified/planned, spec doc, scopes); validator rejects unregistered suites and scope authorization over planned ones; R-001/052/162/187 → conformance:provider; V-017 catch-alls split; W0 case lists deferred to A-047; R-272; ADR-0130 rule 10.

### F-R4-11 — Overview, roadmap and ledger text contradict accepted ADR-0118, ADR-0111 and ADR-0102

Severity: major
Where: [`BLUEPRINT.md`](../../docs/overview/BLUEPRINT.md) line 55 ("PQXDH + Triple Ratchet class"), line 192 ("Recovery is `none` by default"),
line 196 (every user owns a Cloudflare account via OAuth/PKCE); [`PRODUCT_AND_UX_PRINCIPLES.md`](../../docs/overview/PRODUCT_AND_UX_PRINCIPLES.md) line 92
("Cloudflare onboarding exception"); [`DESIGN_RATIONALE_AND_DISCUSSION_SYNTHESIS.md`](../../docs/overview/DESIGN_RATIONALE_AND_DISCUSSION_SYNTHESIS.md) lines 96, 241;
[`IMPLEMENTATION_ROADMAP.md`](../../docs/process/IMPLEMENTATION_ROADMAP.md) 21:32 items 2-3 (recovery default none, OAuth bootstrap); [`OPERATORLESS_CORE_AND_ECOSYSTEM.md`](../../docs/constitution/OPERATORLESS_CORE_AND_ECOSYSTEM.md)
§3; [`architecture_verification_gates.jsonl`](../../machine/architecture_verification_gates.jsonl) V-001 `requirement` ("PQXDH + Triple Ratchet / SPQR-class");
[`decisions.jsonl`](../../machine/decisions.jsonl) D-128 (accepted, "recovery stays off by default").
Problem: ADR-0118 made OM-Crypto-v1 PQXDH + Double Ratchet (no PQ post-compromise healing). ADR-0111 made recovery on
by default. ADR-0102/0046 made onboarding tiered with templates preferred over OAuth. The overview and roadmap still
promise Triple-Ratchet-class PQ properties, which is a security overclaim close to what R-173/A-028 forbid. They also
describe recovery off by default and Cloudflare-account onboarding. Engineers are told to read these documents first,
and the V-001 requirement text and D-128 still contradict the pre-engineering criteria and ADR-0111.
Proposed fix: Add dated reconciliation notes (or edits) to BLUEPRINT §7, §21:32, PRODUCT_AND_UX "Cloudflare
onboarding exception", IMPLEMENTATION_ROADMAP 21:32 and OPERATORLESS §3. Rewrite the V-001 `requirement` to match
ADR-0118. Mark D-128's "off by default" clause superseded. Add a validator guard that rejects "Triple Ratchet"/SPQR as
a v1 claim outside decision history.
Resolution: accepted — BLUEPRINT §7 and 21:32, PRODUCT_AND_UX, DESIGN_RATIONALE, IMPLEMENTATION_ROADMAP, OPERATORLESS §3 corrected; V-001 requirement rewritten; D-128 superseded_in_part; validator Triple Ratchet/SPQR/PQ-group guard (tools commit); R-276; ADR-0130 rule 12.

### F-R4-12 — Public release gates are not machine-applicable, so the AGENTS trust-root rule cannot be enforced

Severity: major
Where: [`public_release_gates.jsonl`](../../machine/public_release_gates.jsonl) (entries have no scope, applicability, criteria or independence fields);
[`ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md`](../../docs/process/ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md) §3 line 45; [`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](../../docs/process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md) §2 line 22;
AGENTS.md 21:32 rule on public binaries; PR-005 vs [`PROJECT_STEWARDSHIP_AND_CONTINUITY.md`](../../docs/constitution/PROJECT_STEWARDSHIP_AND_CONTINUITY.md) §4 S0 line 47 and
[`PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`](../../docs/constitution/PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md) §6 line 126.
Problem: The contract says a PR gate blocks "the affected public feature/claim/distribution", but no machine field says
which one. The validator therefore cannot stop a W0 public binary shipping with PR-005 (TUF/threshold root) or PR-012
(advisory channel) open, which is exactly the AGENTS rule. Custody during S0 is also stated three ways: "1-of-1"
(STEWARDSHIP, PHILOSOPHY), "founder-held 2-of-3 offline" (PR-005, SUPPLY_CHAIN 2026-09-23) and a threshold claim.
A 2-of-3 threshold held by one person is still 1-of-1 against coercion, and public wording should say so.
Proposed fix: Give PR gates `applies_to` (scopes, distribution kinds, claim ids), `criteria` and `independence`
fields like V-gates. Add a validator rule that a release manifest for a scope requires its PR gates. Reconcile S0
custody to one statement ("2-of-3 shares, one human custodian; equivalent to 1-of-1 against coercion").
Resolution: accepted — every PR gate has applies_to/criteria/independence from a contract vocabulary; public_releases in repository_contract; validator refuses an authorized release with an applicable open gate (SECURITY_RELEASE_GATES, CONTRACT §3); one S0 custody statement in STEWARDSHIP, PHILOSOPHY, SUPPLY_CHAIN, PR-005, stewardship.json; R-273, R-277, T-089; ADR-0130 rule 7.

### F-R4-13 — Engineering-unlock rules and the wave table contradict the ledger

Severity: minor
Where: [`ARCHITECTURE_PHASE_PLAN.md`](../../docs/process/ARCHITECTURE_PHASE_PLAN.md) line 72 ("every `blocks_engineering` verification gate is `passed`") and line 68
("All 17 blocking gates"); [`ENGINEER_START_HERE.md`](../../docs/process/ENGINEER_START_HERE.md) line 8 ("until a later ADR sets `engineering_authorized=true`");
[`SYSTEM_ARCHITECTURE.md`](../../docs/overview/SYSTEM_ARCHITECTURE.md) line 248 ("V-001 through V-017"); [`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](../../docs/process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md) §3 lines
37-38 (omits V-022, which the ledger lists for `reference_client_ui` and `provider_reference`).
Problem: These describe the superseded global all-gates lock or stale gate sets, while ADR-0108 and the contract make
per-scope flags the real control. An engineer reading the phase plan gets the wrong unlock rule.
Proposed fix: Add a superseded note pointing to RELEASE_WAVES §4 and regenerate the §3 table from the ledger, as
[`tools/render_gates.py`](../../tools/render_gates.py) already does for the gate list.
Resolution: accepted — PHASE_PLAN and SYSTEM_ARCHITECTURE superseded notes, ENGINEER_START_HERE per-scope rule; RELEASE_WAVES §3 table now generated by tools/render_gates.py (checked by the validator).

### F-R4-14 — SponsoredMailboxGrant chaining is "false by default" in prose but a constant in the schema and CDDL

Severity: minor
Where: [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../../docs/architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md) §3.2 line 57; [`sponsored-mailbox-grant.logical.schema.json`](../../spec/protocol/sponsored-mailbox-grant.logical.schema.json)
(`"const": false`); [`spec/cddl/delivery.cddl`](../../spec/cddl/delivery.cddl) field 9 (`false`).
Problem: The prose implies a sponsor may enable chaining. The frozen CDDL makes it impossible. Implementers will
disagree on whether a `true` grant is invalid or merely non-default.
Proposed fix: Change the prose to "always false in baseline; T1 Homes grant mailboxes under their own policy, not by
chained sponsorship".
Resolution: accepted — HOME §3.2: may_sponsor_further always false in baseline; T1 Homes grant under their own policy.

### F-R4-15 — Stale references

Severity: minor
Where and fix:
- [`SNAPSHOT.md`](../../SNAPSHOT.md) "design_open=0", "V-001..V-021": A-024..A-028 are open and V-022 exists. Regenerate.
- AGENTS.md Part 2 "Preserve all six OM meanings" vs [`PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`](../../docs/constitution/PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md) §8 and
  [`ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md`](../../docs/process/ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md) line 59 ("seven"): pick one count (six plus 0M, per ADR-0094).
- [`PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md`](../../docs/constitution/PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md) line 37 "om.org": use a neutral placeholder; `lonunjie.org` is the real domain.
- [`PROJECT_STEWARDSHIP_AND_CONTINUITY.md`](../../docs/constitution/PROJECT_STEWARDSHIP_AND_CONTINUITY.md) line 5 and [`machine/stewardship.json`](../../machine/stewardship.json) `lonunjie/OM` vs the actual `lonunjie/om`.
- [`BLUEPRINT.md`](../../docs/overview/BLUEPRINT.md) line 118 "Default OM service can hide all of this" implies an operated OM service; reword to
  "the default client".
Resolution: accepted — SNAPSHOT regenerated; "six meanings plus 0M" in AGENTS, NAMING, PHILOSOPHY §8, CONTRACT, READMEs, contract rule; om.org → neutral placeholder (LONGEVITY); lonunjie/om in STEWARDSHIP and stewardship.json; BLUEPRINT "the default client".

## Checked and sound

The constitutional core is coherent and mutually consistent: P-01..P-13, the 0M reading, "capability support does not
imply project operation", jurisdiction-neutral role boundaries, and the licensing stack (MPL core, AGPL reference
providers, Apache normative artifacts, DCO with no CLA) agree with each other and with ADR-0070/0080/0082. The
LONGEVITY model (self-contained continuity bundles, immutable registry IDs, explicit fork lineages, no master key
after catastrophic breaks) is sound and correctly separates project, distribution and protocol succession.
[`TRANSPORT_ADAPTERS_AND_INTEROP.md`](../../docs/architecture/network/TRANSPORT_ADAPTERS_AND_INTEROP.md) holds the no-silent-downgrade and bridge-boundary rules. The zero-payment
guardrails (no mandatory R2, fail-closed budgets, no free-tier numbers in protocol) are consistent across the provider
docs. The vulnerability-response class split and the "no kill switch" principle are right; only the floor mechanism's
key governance needs work (F-R4-3). The W0 scope and gate mapping in [`repository_contract.json`](../../machine/repository_contract.json) matches the gate
ledger, apart from the stale table noted in F-R4-13. Every threat in the register references existing requirements
and gates.
