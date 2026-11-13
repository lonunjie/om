# Personal limits profile and ProviderUsageState (PC-C03): plan, not built

Status: **not implemented.** The design gate for `infra/` needs five domains and about 330 KB of documents and ADRs
to be read in full. That does not fit one session next to the build, so this session read the provider and delivery
documents, wrote the design below from them, and stopped before any code. No code was committed, so no `Design-Read`
trailer was claimed.

## Design sources read

Read in full:

- [`CLOUDFLARE_REFERENCE_DEPLOYMENT.md`](../architecture/providers/CLOUDFLARE_REFERENCE_DEPLOYMENT.md), [`IMPLEMENTATION_INTERFACES.md`](../architecture/providers/IMPLEMENTATION_INTERFACES.md) (including "ProviderUsageState"),
  [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md), [`HOME_WAKE_CHANNEL.md`](../architecture/providers/HOME_WAKE_CHANNEL.md), [`PROVISIONING_AND_BOOTSTRAP.md`](../architecture/providers/PROVISIONING_AND_BOOTSTRAP.md)
  (domain `provider_deployment_supply_chain`, part)
- [`RELAY_AND_DELIVERY_ARCHITECTURE.md`](../architecture/delivery/RELAY_AND_DELIVERY_ARCHITECTURE.md), [`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](../architecture/delivery/MULTI_RELAY_AND_DELIVERY_RESILIENCE.md), [`REACHABILITY_AND_OBJECT_REPAIR.md`](../architecture/delivery/REACHABILITY_AND_OBJECT_REPAIR.md)
  (domain `relay_reachability`, documents only)
- [`PROVIDER_CONFORMANCE_SUITE.md`](../architecture/providers/PROVIDER_CONFORMANCE_SUITE.md) PC-C table; `infra/cloudflare-home/src/limits.ts`

Not read (so the gate's trailer cannot be claimed for `infra/` yet):

- `provider_deployment_supply_chain`: [`SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md`](../architecture/providers/SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md), [`VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md`](../architecture/providers/VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md),
  ADR-0014, 0018, 0031, 0032, 0034, 0036, 0046, 0049, 0102, 0104, 0112, 0130, 0159, 0185
- `relay_reachability`: ADR-0012, 0022, 0075, 0126
- `metadata_privacy`: everything (PRIVACY_GOVERNANCE_AUTHORITY, METADATA_THREAT_MODEL, NOTIFICATIONS_AND_PUSH,
  STORAGE_AND_BLOB_ARCHITECTURE, SPACES_ROOMS..., ADR-0098, 0124, 0128, 0129, 0140)
- `discovery_first_contact`: everything
- `constitution_governance`: everything

Next session: run `python3 tools/design_gate.py needs infra/cloudflare-home/src/usage.ts`, read the list (about 85k
tokens), then build. A cheaper split is to give the plan below to a session that does only that reading plus the build.

## What the design says (requirements, no invention)

1. `ProviderUsageState` is returned by `usage_state()` on `RelayProvider`, `BlobStore` and `MediaProvider`. It must
   distinguish at least: healthy/unhealthy; soft quota approaching; hard free-tier failure threshold; billable
   overage threshold; configured budget remaining; reset time/window; unknown/unverifiable. Routing must fail safely
   when cost state is unknown and a hard zero-cost guarantee is required ([`IMPLEMENTATION_INTERFACES.md`](../architecture/providers/IMPLEMENTATION_INTERFACES.md)).
2. PC-C03: reported usage matches the runner's own accounting within 1%.
3. Free SQLite Durable Object limits fail further operations of that type until reset; a reference deployment "must
   track usage and fail over before hard failure" ([`RELAY_AND_DELIVERY_ARCHITECTURE.md`](../architecture/delivery/RELAY_AND_DELIVERY_ARCHITECTURE.md), "Operational budget behavior";
   Cloudflare profile "Free-plan and billing guardrails"). `monthly_paid_budget = 0` is enforced before billable use.
4. Limits are implementation parameters, never protocol constants, advertised in `ProviderCapabilities.limits`.
5. Silent on: numbers, the shape of the wire encoding of usage state, and any UI line. So no UI line is built (the
   Design gate says do not invent product surface), and the figures are decided below as implementation parameters.

## Proposed vertical (smallest honest slice)

Home-only, `infra/cloudflare-home`, no new tier, no spending, no dependency on SeedLife's account.

1. `src/limits.ts`: add `PERSONAL_LIMITS` (one owner, a handful of mailboxes: queue items, queue bytes, deposit
   capabilities and blob spool all smaller than the multi-person defaults), selected by an env var `OM_PROFILE=personal`.
   Default stays today's values. Advertised through the existing `limits` object.
2. `src/usage.ts`: a counter kept in the existing Durable Object SQLite (one small table: day bucket, Worker
   requests, DO requests, stored bytes). The Home counts what it serves itself, so the figure is its own accounting,
   not a call to Cloudflare's analytics (which needs a token the Home must not hold).
3. State machine from counts and a configured ceiling (`OM_USAGE_CEILING_*`, free-plan figures **checked on the day**
   from Cloudflare's docs before being written as defaults): `healthy`, `soft` (>= 80% of ceiling), `hard` (>= 95%,
   new deposits shed with typed `quota`, drains and acks still served so mail is never trapped), `unknown` (counter
   unreadable). `billable_overage` is `never` (the Home has no paid resource) and `budget_remaining` is 0 by policy.
4. A typed read operation as an advertised optional capability (for example `om.relay.usage.v1`, same pattern as
   `om.relay.wake.v1`), requiring the admin capability; never public, never in a URL.
5. Adapter `usage_state()` in the provider package mapping the response; unknown or unsupported maps to `unknown`,
   and routing treats `unknown` as not safe under a zero-cost policy.
6. PC-C03 test in workerd: drive N deposits and drains, compare the reported counter to the test's own tally within 1%
   (exact, since the Home counts itself).
7. Relay status: move `live ProviderUsageState` out of D12 only when 1 to 6 are merged. If the capability id or the
   counter semantics differ from a normative text, record it as D13 citing the passage.

## What remains

Everything above. First: the gate reading list. Then steps 1 to 7. Unknowns needing the operator or a fresh check:
Cloudflare free-plan figures on the day, and whether an advertised capability id such as `om.relay.usage.v1` needs an
ADR or only a feature-note like ADR-0185 (an ADR for a new capability would be a stop-and-ask, not an improvisation).
