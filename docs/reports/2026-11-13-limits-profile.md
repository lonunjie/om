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

## Build status (2026-11-13, later session): Home-only slice built, uncommitted

Design sources read for this build: this report (in full), [`AGENTS.md`](../../AGENTS.md) "Design gate", and the existing `infra/cloudflare-home`, `om-home-wire` and `om-provider-http` code. The domain reading list that `design_gate.py needs` prints for `infra/` was **not** read again in this session beyond what the plan above records, so no `Design-Read` trailer is claimed here; the session that commits the code must read the infra domains first. Nothing below is deployed, staged or committed except this report.

Files (all uncommitted, unstaged, except where noted):

- `infra/cloudflare-home/src/limits.ts`: `PERSONAL_LIMITS`, `USAGE_CONFIG` (ceilings and thresholds, with the "check against Cloudflare's free-plan numbers on the day" comment), `profileOf`, `limitsOf`.
- `infra/cloudflare-home/src/usage.ts` (new): `UsageMeter` Durable Object (one SQLite table, per UTC day), `classify`, `usageDocument`, `usageOn`.
- `infra/cloudflare-home/src/index.ts`, `capabilities.ts`, `env.ts`, `http.ts`, `queue.ts` (`adminCheck`), `mailbox.ts`, `intro.ts`, `blobs.ts`: metering at the Worker front door, shedding at `hard`, the gated read, profile-aware limits, advertisement.
- `infra/cloudflare-home/wrangler.jsonc` and `release/template/wrangler.json`: `USAGE` binding and migration `v3` (adds only `UsageMeter`).
- `packages/om-home-wire/src/usage.ts` (new), `index.ts`: capability id, path, `ProviderUsageState`, strict `parseUsageState`, `zeroCostSafe`.
- `packages/om-provider-http/src/provider.ts`, `homes.ts`, `index.ts`: `usageState(mailbox?)`.
- Tests: `infra/cloudflare-home/test/usage.test.ts` (new; PC-C03 and the rest), `test/upgrade.test.ts` (v3), `test/harness.ts` (binds `USAGE`), `packages/om-provider-http/src/usage.test.ts` (new).
- [`machine/file_registry.json`](../../machine/file_registry.json): four new files registered by `om_registry.py sync`.

Decisions (conservative readings; each is for the operator or the committing session to confirm or rename):

1. **Capability id and path** `om.relay.usage.v1` and `/v1/relay/usage`, both defined once in `om-home-wire/src/usage.ts` (`USAGE_FEATURE`, `USAGE_PATH`) so renaming is one edit. A new advertised capability and endpoint with a JSON (not CBOR) body is not in [`home-api.md`](../../spec/protocol/home-api.md); it needs D13 in the relay status (or an ADR, see the open question above) before the code is committed. The read is `GET` with the admin capability of any queue the Home holds as Bearer; answers are `no-store`; a wrong or missing credential is the ordinary `unauthorized`, and a Home without the profile or the counter answers `not_found`.
2. **Where the counter lives:** a new singleton Durable Object `UsageMeter`, because per-queue objects cannot give a Home-wide figure. This needs migration `v3` (append-only, checked by `upgrade.test.ts`) and a `USAGE` binding in the template. A Home deployed from an older template simply lacks it: not advertised, nothing shed.
3. **What is counted:** every request the Worker receives (including shed ones) is one Worker request; Durable Object calls are the meter's own call plus the one routed call (the usage read: meter plus admin check). Not counted: WebSocket messages after the upgrade. (Amended 2026-11-14: the extra per-device calls inside one key fetch are now counted; see "Key fetch fan-out metering" below. The original wording counted a fixed two Durable Object calls per request and undercounted a batched fetch of up to 64 devices.) Stored bytes are not metered; they are bounded per queue and spool by the personal limits. Two metrics only: Worker requests and Durable Object requests per UTC day.
4. **Thresholds:** soft at 80%, hard at 95% of the worse of the two ceilings (from the plan). Ceilings are 100,000 and 100,000 per day as last known, in `USAGE_CONFIG`, overridable with `OM_USAGE_CEILING_WORKER` and `OM_USAGE_CEILING_DO`. They must be checked against Cloudflare's free-plan numbers on the day before any deploy.
5. **What `hard` sheds:** `POST` deposit, queue creation, intro request submission and blob put, with `quota` detail class 3 and `hard=true` (no new error code or class invented). Drains, acks, rotation, revocation, key and descriptor reads, the capabilities document and the usage read are always served, so held mail is never trapped. The shed answer is given before the body is read.
6. **`unknown` never sheds.** If the meter call fails, the request is served and the state reports `unknown`; the free plan's own hard stop is the backstop and a counter fault must not strand mail. A Home that cannot count (profile set, no binding) advertises nothing. Client side, `unknown` is "not safe" under a zero-cost policy (`zeroCostSafe`). Opposite choice (shed on unknown) is one line in `index.ts` `count()` if the operator wants it.
7. **Unrecognised `OM_PROFILE` values** (anything but empty, `default`, `personal`) select `personal`, the stricter profile, rather than silently the default.
8. **Wire shape of the state:** `state` (healthy/soft/hard/unknown), `billable_overage: "never"`, `budget_remaining: 0`, window start and reset (POSIX seconds), per-metric used and ceiling, profile. The reader accepts nothing that does not vouch for zero cost and a window; anything else is `unknown`. No UI line was built.
9. **Personal figures** (one place, `PERSONAL_LIMITS`): queue items 128, queue bytes 8 MiB, deposit capabilities 16 (the one made with the queue counts), blob spool 16 MiB, 32 blobs. They are my sizing for "one owner, a few mailboxes", not from a design text.
10. `HttpProvider.usageState(mailbox?)` uses the admin capability of a given or already-claimed mailbox and returns `unknown` before any queue exists; with several Homes (`createHomesProvider`) it returns the most conservative state. The adapter is not wired into any Core routing or UI.

Results:

- `infra/cloudflare-home`: `pnpm exec vitest run` 18 files, 153 tests passed (including `test/usage.test.ts`: default profile unchanged, personal profile without counter, advertisement and limits, admin gate, PC-C03 with the reported counts equal to the test's own tally for both metrics (within 1% asserted, exact observed), healthy to soft to hard with deposits and queue creation shed and drain, ack, capabilities and the read still served). `blobs.test.ts` did not fail under load.
- `packages/om-provider-http`: `pnpm exec vitest run src` 116 tests passed; the one "failed file" is a stale compiled `dist/om-provider-http/src/core.e2e.test.js` dated 2026-10-28 that the `src` filter also matches (missing import path in `dist`), unrelated to this change. `packages/om-home-wire`: 18 passed.
- `pnpm exec tsc -b apps/web`: clean. `python3 tools/om_docs.py fmt`: nothing to rewrite. `make check`: passes (after registering the four new files).
- `eslint` on the new and changed files is clean except one `restrict-template-expressions` error in `provider.ts` that exists without my change.
- Not done: relay status D12/D13 edit, a `Design-Read` reading, the `core.e2e` and Core-level use of `usageState`, any deploy.

## Design gate pass (2026-11-13, committing session)

Design sources read in full (first pass): domain `provider_deployment_supply_chain` (all seven documents: CLOUDFLARE_REFERENCE_DEPLOYMENT, IMPLEMENTATION_INTERFACES, HOME_WAKE_CHANNEL, HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY, PROVISIONING_AND_BOOTSTRAP, SOFTWARE_SUPPLY_CHAIN_AND_UPDATES, VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES; ADR-0014, 0018, 0031, 0032, 0034, 0036, 0046, 0049, 0102, 0104, 0112, 0130, 0159, 0185) and domain `relay_reachability` (RELAY_AND_DELIVERY_ARCHITECTURE, MULTI_RELAY_AND_DELIVERY_RESILIENCE, REACHABILITY_AND_OBJECT_REPAIR; ADR-0012, 0022, 0075, 0126), plus [`home-api.md`](../../spec/protocol/home-api.md).

**Also read in full by the committing session (2026-11-13, second pass):** `constitution_governance` (all twelve documents: PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE, LICENSING_AND_CONTRIBUTION_POLICY, PROJECT_STEWARDSHIP_AND_CONTINUITY, OPERATORLESS_CORE_AND_ECOSYSTEM, PRIVACY_GOVERNANCE_AUTHORITY, LEGAL_AND_OPERATIONAL_BOUNDARIES, PROTOCOL_EXTENSIBILITY, NAMING, ARCHITECTURE_RESOLUTIONS_20260923, MESSENGER_BASELINE_AND_ECOSYSTEM_QUEUE, SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY, PROTOCOL_LONGEVITY_AND_0M_CONTINUITY; ADR-0042, 0043, 0048, 0049, 0050, 0069, 0070, 0071, 0078 to 0083, 0094, 0100, 0114, 0122, 0130), `discovery_first_contact` (DISCOVERY_FABRIC, INVITES_INTROGATE_AND_SPAM, CONTACT_SHARING, INTRODUCTION_AND_SERVICE_ACCOUNTS, KEY_TRANSPARENCY_AND_DIRECTORY; ADR-0004, 0019, 0020, 0021, 0025, 0033, 0040, 0126) and `metadata_privacy` (PRIVACY_GOVERNANCE_AUTHORITY, MULTI_RELAY_AND_DELIVERY_RESILIENCE (first pass), NOTIFICATIONS_AND_PUSH, STORAGE_AND_BLOB_ARCHITECTURE, SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION, METADATA_THREAT_MODEL; ADR-0098, 0124, 0126, 0128, 0129, 0130, 0140). All five gate domains are now read.

**Privacy check of the usage counter and read (METADATA_THREAT_MODEL 11 and 20, PRIVACY_GOVERNANCE_AUTHORITY "Scoped", ADR-0098):** the counter is one Durable Object holding per-UTC-day totals of two integers; it records no queue, capability, address, header or timing finer than a day, so it creates no correlator and no per-person data. Counting reads nothing identifying (the Worker already reads no network address). The read is personal-profile only, needs the owner's admin capability as a bearer header (never a URL), is `no-store`, and returns Home-wide figures that the owner of a single-owner Home already has by definition; a multi-tenant Home would not expose it. The `quota` shed at `hard` is the existing typed class with no per-sender state; it tells a depositor only that the Home is full, which a provider-local operational signal (section 11, "provider-local telemetry") permits. The public capabilities document gains only the feature id and `limits_profile`, Home-level and identical for every caller. No leak beyond what the design allows; no ADR needed.

Result: no ADR is needed. home-api.md section 3 permits additional endpoints as advertised optional capabilities, IMPLEMENTATION_INTERFACES fixes what the state must distinguish but no encoding, and ADR-0018 keeps vendor figures out of the protocol (they sit in the Home's `USAGE_CONFIG`). Departure recorded as **D29** in the relay status: the optional read is JSON where home-api.md states canonical CBOR for protocol bodies (the wake channel precedent is CBOR), and the capability id and path are not named by any text. Bearer in the `Authorization` header and `no-store` match home-api.md 3.1. Migration `v3` adds only `UsageMeter` (append-only); ADR-0185's "no storage migration" concerned the wake channel only. Decisions 1 and 2 above stand; D12 now says PC-C03 is met only under the personal profile.

Checks: cloudflare-home vitest 152 of 153 on the full run (PC-S09 in `blobs.test.ts` flaked under load; `blobs.test.ts` alone 14 of 14); om-provider-http `vitest run src` 116 passed (the one failed file is the stale compiled `dist` test noted above); om-home-wire 18 passed; `tsc -b apps/web` clean; `om_docs.py fmt` rewrote 0; `make check` passes.

## Key fetch fan-out metering (2026-11-14)

`POST /v1/keys/fetch` carries up to 64 DeviceIDs and the Worker makes one KEYS Durable Object call per device, so the fixed charge of decision 3 undercounted it (PC-C03 asks the reported usage to match the runner's own accounting). The meter now charges what a fetch actually did:

- At the front door a key fetch is one Worker request and the meter's own Durable Object call, as every request. The routed call is not assumed.
- After the fan-out the Worker adds one meter charge of `calls + 1` Durable Object calls and no Worker request, where `calls` is the number of per-device KEYS calls actually issued (counted before each call, so a device that fails is charged, and devices after a failure are not) and the `+ 1` is the charge call itself. A request that reaches no device (malformed body, wrong method) makes no charge call.
- Ceilings, thresholds and the shed set are unchanged: a key fetch adds nothing to what the Home holds, so it is never shed at `hard`, but it is charged at `hard` too. A charge that cannot be recorded is dropped, like any other meter fault (`unknown` never sheds).
- Design sources: the read for this change covered the five `infra/` gate domains in full (listed in the commit trailer); the passages that decide it are IMPLEMENTATION_INTERFACES "ProviderUsageState" and PC-C03, RELAY_AND_DELIVERY_ARCHITECTURE "Operational budget behavior" and CLOUDFLARE_REFERENCE_DEPLOYMENT "Free-plan and billing guardrails", which require tracking before the hard edge and are silent on how a fan-out is counted. Charging the real fan-out is the reading that matches them, so no new deviation is recorded; D29 stands.
- Tests: `infra/cloudflare-home/test/usage.test.ts` ("key fetch is metered by its real fan-out", "key fetch at the hard stop and without the counter"): 1, 2 and 64 devices, a later device failing, a refused capability, a malformed device id after a good one, a body that reaches no device, the fetch still served and charged at `hard`, and no metering without the counter.
