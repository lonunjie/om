# ADR-0130 — Distribution, provisioning and provider trust roots

- **Status:** Accepted
- **Date:** 2026-09-24
- **Decided by:** integrator under operator delegation 2026-09-24 ("do everything and complete it"), from the
  architecture review
- **Source:** [`2026-09-24-claude-opus-architecture-r4.md`](../verification/reviews/2026-09-24-claude-opus-architecture-r4.md) (F-R4-1 .. F-R4-15)
- **Amends:** ADR-0046 (template provisioning), ADR-0102 (T0 exposure, revocation, client origin), ADR-0104 (floor
  governance), ADR-0108 (stale global-unlock text), ADR-0112 (role separation in the reference Home), D-128 (recovery
  default, superseded by ADR-0111), [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../docs/architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md), [`PROVISIONING_AND_BOOTSTRAP.md`](../docs/architecture/providers/PROVISIONING_AND_BOOTSTRAP.md),
  [`CLOUDFLARE_REFERENCE_DEPLOYMENT.md`](../docs/architecture/providers/CLOUDFLARE_REFERENCE_DEPLOYMENT.md), [`SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md`](../docs/architecture/providers/SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md),
  [`VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md`](../docs/architecture/providers/VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md), [`IMPLEMENTATION_INTERFACES.md`](../docs/architecture/providers/IMPLEMENTATION_INTERFACES.md), [`PROVIDER_CONFORMANCE_SUITE.md`](../docs/architecture/providers/PROVIDER_CONFORMANCE_SUITE.md),
  [`CENSORSHIP_AND_NETWORK_RESILIENCE.md`](../docs/architecture/network/CENSORSHIP_AND_NETWORK_RESILIENCE.md), [`LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md`](../docs/architecture/network/LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md), [`CLIENTHOST_UPDATE_LIFECYCLE.md`](../docs/architecture/client/CLIENTHOST_UPDATE_LIFECYCLE.md),
  [`METADATA_THREAT_MODEL.md`](../docs/threat-model/METADATA_THREAT_MODEL.md), the threat-model README, [`PROJECT_STEWARDSHIP_AND_CONTINUITY.md`](../docs/constitution/PROJECT_STEWARDSHIP_AND_CONTINUITY.md),
  [`PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`](../docs/constitution/PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md), [`PRIVACY_GOVERNANCE_AUTHORITY.md`](../docs/constitution/PRIVACY_GOVERNANCE_AUTHORITY.md), [`OPERATORLESS_CORE_AND_ECOSYSTEM.md`](../docs/constitution/OPERATORLESS_CORE_AND_ECOSYSTEM.md),
  [`PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md`](../docs/constitution/PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md), [`NAMING.md`](../docs/constitution/NAMING.md), [`BLUEPRINT.md`](../docs/overview/BLUEPRINT.md), [`SYSTEM_ARCHITECTURE.md`](../docs/overview/SYSTEM_ARCHITECTURE.md),
  [`PRODUCT_AND_UX_PRINCIPLES.md`](../docs/overview/PRODUCT_AND_UX_PRINCIPLES.md), [`DESIGN_RATIONALE_AND_DISCUSSION_SYNTHESIS.md`](../docs/overview/DESIGN_RATIONALE_AND_DISCUSSION_SYNTHESIS.md), [`IMPLEMENTATION_ROADMAP.md`](../docs/process/IMPLEMENTATION_ROADMAP.md),
  [`ARCHITECTURE_PHASE_PLAN.md`](../docs/process/ARCHITECTURE_PHASE_PLAN.md), [`ENGINEER_START_HERE.md`](../docs/process/ENGINEER_START_HERE.md), [`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](../docs/process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md),
  [`ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md`](../docs/process/ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md), [`SECURITY_RELEASE_GATES.md`](../docs/process/SECURITY_RELEASE_GATES.md)

## Context

R4 applied the "Where did authority go?" test to providers, the network layer and the cross-cutting ledgers. Three
authority paths had escaped it: the executable code a T0 user runs (nobody said which origin serves it), the
Deploy-to-Cloudflare template path (it built a mutable branch outside the release trust root), and the
SecurityAdvisory key (one online key could raise a peer-affecting floor and partition the network). Several claims
were traced to gates that do not test them: the ClientHost update lifecycle, censorship resilience, LAN privacy and
a set of conformance suites that do not exist. Public release gates could not be applied by a machine. This ADR
records the decisions; the canonical docs carry the normative text.

## Decision

1. **A T0 user's client never comes from the sponsor by default.** A sponsor's Home or hosting account serves the
   invitee's ClientHost only on explicit opt-in, and then the client treats the sponsor as endpoint trust and says so.
   A sponsored invite offers a native app plus at least two shared ClientHost origins run by independent operators.
   A project-controlled PWA origin, if one exists, is a declared project-controlled service under PR-001, PR-005 and
   PR-012 with release-transparency monitoring, offered next to the others, never alone (HOME §3.5, R-260, T-080).
2. **Every provisioning path activates a verified release.** Templates, CLI, IaC and OAuth installers all deploy a
   signed, digest-pinned release verified against the distribution's TUF targets before activation. A template's
   first step is a verifier that stays behind as the staged update controller; nothing auto-deploys a branch head.
   OAuth scopes are described honestly as account-wide; tokens are session-only by default and never stored in the
   PWA origin. The template repository and any project OAuth client registration join the custody map (R-261, R-262,
   T-081).
3. **The advisory key is a delegated TUF role, and peer-affecting floors need the root threshold.** Advisories split
   `own_release_floor` (own clients, delegated key) from `peer_floor` (CryptoProfile or protocol generation). A peer
   floor applies automatically only with root-threshold signatures, may name only a registry-`preferred` profile or
   generation with at least two independent implementations, and user override is permitted unless threshold-signed.
   Every advisory has `expires_at`. The advisory role joins the custody map (R-263, R-264, T-082).
4. **Roles run by one operator collude by construction** (the reviewer's option b). The reference Home serves the
   ContactDescriptor, IntroGate and mailbox together, so its operator links them; descriptors, UI copy and the
   threat model say so, the METADATA §18 example is qualified, and HOME §3.3 states that the sponsor sees contact
   edges among co-resident users. The reference Home still separates credentials, storage namespaces and logs per
   role as defense in depth (PC-B06), without claiming non-collusion (R-265, T-083).
5. **The ClientHost update lifecycle has its own gate.** V-006 tests the DeviceVault, so a new gate V-023
   `clienthost_update_lifecycle` takes CLIENTHOST §8 as pre-engineering criteria for `reference_client_ui`, with a
   live update and rollback drill before release. Release-transparency logs and monitors are specified as roles:
   monitors are optional and non-authoritative, consumers need two independent observations, and a missing
   observation never blocks messaging. The concrete log protocol is A-046 (R-274; R-095, R-164 and T-019 re-traced).
6. **NetworkTransport is a contract, and resilience claims wait for PR-013.** A transport is an untrusted byte
   carrier with conformance cases NT-01..NT-05. Network-level vendor blocking and discovery-suffix blocking are threats
   T-085 and T-086. V-020 gains a blocked-vendor and blocked-suffix simulation, and R-021, R-028 and R-045 trace to
   it. Claim discipline: no public use of the CENSORSHIP §1 / BLUEPRINT §18A resilience wording until PR-013 passes
   (R-268, R-269).
7. **Public release gates are machine-applicable, and S0 custody has one statement.** PR gates carry `applies_to`
   (scopes, distribution kinds, claims), `criteria` and `independence`; planned releases are declared in
   [`repository_contract.json`](../machine/repository_contract.json), and the validator refuses an authorized or published release with an applicable open
   gate. S0 release-root custody is "2-of-3 root shares, one human custodian; equivalent to 1-of-1 against coercion"
   everywhere (R-273, R-277, T-089).
8. **LAN rendezvous is opt-in per contact and fixed-cardinality.** It runs only for contacts the user marks, is off
   under Hardened and Maximum Privacy, stops for a contact on block or relationship end, advertises exactly K tags per
   epoch (decoys fill the rest) and rotates with the link-layer address where possible. V-015 gains a LAN trace
   criterion (R-270, R-271, T-087, T-088).
9. **A sponsor cannot cut an invitee off instantly.** Sponsor revocation keeps deposits and drains working for at
   least 30 days (T1 eviction parity), enforced by the Home and tested by PC-D06, and the client adds a non-sponsor
   route soon after first run. The sponsor is modeled as a possible proximate adversary (R-266, R-267, T-084).
10. **Conformance suites are registered.** [`machine/conformance_suites.json`](../machine/conformance_suites.json) lists every `conformance:<name>` target
    with its spec document, status (`specified` or `planned`) and the scopes that need it. The validator rejects an
    unregistered suite, a missing spec document, and an authorized scope that still depends on a `planned` suite.
    R-001, R-052, R-162 and R-187 trace to `conformance:provider`; V-017's catch-all traces are split (R-272; W0
    case lists are A-047).
11. **Home API before `provider_reference`.** The normative Home API, a real ProviderCapabilities schema with
    `durability_class`, the missing core interfaces and NetworkTransport are design item A-045, which blocks
    `provider_reference`; "Home API frozen" is a V-020 pre-engineering criterion (R-275). The API is not written here.
12. **Stale text is corrected.** OM-Crypto-v1 is PQXDH + Double Ratchet with no PQ post-compromise healing (BLUEPRINT
    §7, V-001 requirement text); recovery is on by default (ADR-0111; D-128's "off by default" clause is superseded);
    onboarding is tiered, with no Cloudflare-account exception; the global-unlock rule is superseded by per-scope
    unlock and the RELEASE_WAVES §3 table is generated from the ledger; `SponsoredMailboxGrant.may_sponsor_further` is
    always false; the name has six meanings plus 0M; `om.org` becomes a neutral placeholder; the repository is
    `lonunjie/om`. The validator rejects "Triple Ratchet"/SPQR v1 claims and PQ claims for groups outside decision
    history, research and reviews (R-276).

## Consequences

- Requirements R-260..R-277, threats T-080..T-089, design items A-045..A-047 and gate V-023 are added. V-015, V-018
  and V-020 gain pre-engineering criteria; V-001's requirement text is rewritten; PR-005, PR-012 and PR-013 gain
  criteria; every PR gate gains `applies_to`, `criteria` and `independence`.
- The SecurityAdvisory logical schema replaces `recommended_floor` with `own_release_floor` and `peer_floor` and adds
  mandatory `expires_at`. It is not a frozen registry object. [`om-time-fields.json`](../spec/registries/om-time-fields.json) gains a SecurityAdvisory row.
- The W0 MetadataExposureDescriptors gain `descriptor_mailbox_linkage` and mark `contact_graph_edge` as `sometimes`
  for Home roles; the trace model still passes.
- `provider_reference` cannot be authorized until A-045 is resolved and the Home API is frozen; `reference_client_ui`
  cannot be authorized until V-023's pre-engineering phase passes and A-046 is resolved.
