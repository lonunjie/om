# provider_reference status: the first reference Home (Cloudflare)

- **Date:** 2026-10-28
- **Scope:** `provider_reference` (ADR-0171), operator-authorized. Author: Claude Sonnet 5.5 subagent. No independent review yet (ADR-0167 applies to crypto; this component adds none).
- **Goal:** the first OM reference relay/provider on Cloudflare (Workers + SQLite Durable Objects), an untrusted store-and-forward mailbox, plus the Core transport adapter and an end-to-end test of two real Core profiles through it.
- **Not done on purpose:** nothing was deployed, no Cloudflare credential was used. The deploy plan is at the end.

## What exists

| Piece | Where | Notes |
|---|---|---|
| Reference Home | `infra/cloudflare-home` | Worker plus four SQLite Durable Object classes: `MailboxQueue` (relay), `KeyPool` (key material), `DescriptorStore`, `IntroGate`. README there. No R2, KV or Queues. |
| Wire layer | `packages/om-home-wire` | Deterministic CBOR (strict), Home API v1 bodies, the typed error registry (checked against [`om-home-errors.json`](../../spec/registries/om-home-errors.json)), sealed-delivery outer checks, `RelayEntry` and `route_binding`. Runs in Workers, Node and browsers. |
| Core transport adapter | `packages/om-provider-http` | `createHttpProvider` implements the Core `Provider` interface (the transport trait of `@om/core-wasm`) over the Home API. No `crates/om-core` change was needed. |
| Web wiring | `apps/web/src/core/{providerConfig.ts,ConfiguredApp.tsx}`, `src/main.tsx` | Production entry resolves its Home from configuration; unset keeps "Not connected to a provider". |
| Threat model | [`2026-10-28-relay-threat-model.md`](2026-10-28-relay-threat-model.md) | What a malicious operator can and cannot see or do. |

Relay behaviour (conformance cases of [`PROVIDER_CONFORMANCE_SUITE.md`](../architecture/providers/PROVIDER_CONFORMANCE_SUITE.md), run against the real Worker bundle in workerd via Miniflare):

- **PC-A01..A06** separate deposit and receive capabilities, held as hashes; forged, truncated and other-queue capabilities answer exactly like a full queue; rotation with overlap; ack and idempotent retry; no cross-Home reuse; no address, log or cookie in the source (guard test).
- **Replay and idempotency (PC-H02/H03).** Exact wrapper retry returns the original receipt and stores one copy; a different wrapper under a used `(delivery_key_id, delivery_id)` and a reused `request_id` with different bytes are `invalid_request`; items and request-id state survive a Home restart (persisted Durable Object state, restarted Miniflare).
- **Limits (PC-C01).** Payload size, per-capability rate (with `retry_after`), queue item quota (512, flagged hard on the free plan), retention bound, capability count, bounded replay and idempotency tables. All typed.
- **Pagination.** Drain pages of at most 64 items or 1 MiB with a continuation cursor; `after` skips what the caller holds; ack frees storage.
- **Key material (PC-K01..K06).** Publish claims a device id; fetch returns bundle then one-time items, each consumed once, throttled; stale generation, wrong device and expiry refused as aids only; best-effort generation revoke.
- **Descriptors (PC-G01).** Replace, tombstoned revoke (not republishable), `not_found` for missing, expired and revoked alike, read rate limit.
- **IntroGate (PC-G02).** Tickets, single-use admission, exact-retry idempotence, `spent_token`, `admission_denied`, payload limit, per-service rate charged only to real tickets, drain with implicit ack.

End to end (`infra/cloudflare-home/test/core.e2e.test.ts`, `shared-runtime.test.ts`, `apps/web/e2e/relay.spec.ts`):

- Two real Core wasm profiles on separate installs and stores: invite, introduction, messages both ways, restart of both sides with empty adapter caches, redelivery harmless, nothing but sealed objects and signed key material reaches the Home, no capability in any URL.
- A response lost after the Home stored a message: Core sees `PendingReconciliation`, the exact retry carries the same `request_id`, the message is delivered once.
- Home unreachable: `PendingNetwork`, then delivered when it returns.
- Browser: the production entry (real Core, vault, adapter) with two identities against the local Home in Chromium, Firefox and WebKit, including a reload and a message after it.

Counts: 72 new vitest tests (wire layer 12, adapter 9, Home 46 including the adapter-against-Home and two-Core cases, web configuration 5) plus the browser spec `relay.spec.ts` on three engines. Whole suite: see "Checks".

## Deviations and spec questions (none resolved silently)

| ID | Item | What was done | Needs |
|---|---|---|---|
| D1 | **Route stand-in vs spec `relay_uri`.** Core binds delivery keys to the fixed route name `om-core:mailbox/v0` plus its 16-byte mailbox; it has no `RelayEntry`/`RelaySet`. | The Home and adapter implement the spec route object: `RelayEntry` encode/decode and `route_binding = SHA-256(OM-CBOR [label, relay_uri, deposit_capability])`, with vectors computed independently. `HttpProvider.routeEntry(mailbox, expiresAt)` returns `{relay_uri: "<base>/", capability: <32-byte deposit capability>}`. The relay itself never sees a binding, so it works unchanged. | Core adoption: M1 `RelaySet`. Core must then bind keys to `routeEntry()` instead of the stand-in. Not done: a Core change needs the concurrent first-contact worker's files. |
| D2 | **One shared id in Core, separate capabilities in the spec.** Core uses one random mailbox id both parties know; the spec has distinct deposit and receive capabilities. | The Home enforces the separation. The adapter derives `queue_id = SHA-256("om-relay-queue-v0" \|\| mailbox)[:16]`, `deposit = queue_id \|\| mailbox` (the peer can compute it), and `receive`, `admin` = `queue_id \|\| HMAC(install secret, label \|\| mailbox)`. The 32-byte install secret lives in IndexedDB (`om-provider-secrets`). Every capability begins with its 16-byte routing prefix (a Home needs it to find the Durable Object); `opaque-cap` allows 16..96 bytes. | Core-held receive and admin capabilities once Core grows a provider-admin concept. |
| D3 | **Trust-on-first-use claims.** A queue is created by the owner's first poll; a device's key pool by its first publish; a descriptor id by its first put. | Idempotent for identical input; a second claimant gets `unauthorized`; the adapter exposes `lostMailboxes()`. A deposit to an unclaimed queue is indistinguishable from a full queue and is retried with local backoff. | UI to surface a lost mailbox. A claim made before the owner's first drain pass can be stolen by a peer who knows the id (threat model 4.4). |
| D4 | **Anonymous one-time prekey fetch.** Spec: relationship capability; anonymous gets request-key material only. | Fetch takes a placeholder capability and returns bundle plus one-time items under a per-device throttle (burst 8, one per 10 minutes). | Core relationship fetch capabilities (ASYNC_KEY_MATERIAL 2.1). |
| D5 | **Bundle publish and fetch shapes.** Spec: one signed AsyncKeyBundle containing its one-time items; response is a list of bundles. Core pass 2 keeps one-time items outside the bundle. | Feature `om.keys.pool.v1`: `{bundle, opks[], pqopks[]}`; fetch returns a flat list `[bundle, opk?, pqopk?]` per device. The Home reads device id, generation and expiry from the bundle structurally (Core's real bundles parse; tested) and verifies nothing. | Reconcile when Core moves one-time prekeys into the bundle. |
| D6 | **Drain request/response fields.** CDDL gives `RelayDrainRequest` an unnamed optional uint and `RelayDrainResponse` an optional opaque bstr. | Request key 3 = `after` (highest sequence the caller holds); response key 2 = continuation (8-byte big-endian sequence) present only while more remain; `RelayItem` key 2 = sequence. Core acknowledges cumulatively, the Home by item id: the adapter caches ids from polls and, after a restart, re-reads the window below the sequence (at most 256 ids) to learn them. | Name these fields in the CDDL (a spec amendment, not made here). |
| D7 | **Queue creation and descriptor put/delete are not in the Home API table.** | Advertised optional features `om.relay.queues.v1`, `om.discovery.put.v1` (retention hint travels in the request, since Core's invite descriptor is not a ContactDescriptor with `expires_at`), `om.discovery.delete.v1`, `om.intro.v1` (drain with implicit ack), `om.keys.fetch-throttled.v1`. | ADR or amendment if they should become normative. |
| D8 | **Descriptor validation.** Spec: `invalid_signature`. | The Home treats descriptors as opaque (clients are authoritative, home-api.md 3.2). `invalid_signature` is never emitted. | Fine for the reference; a signed-ContactDescriptor path arrives with H2. |
| D9 | **Deposit object check.** | Only a profile-1 `SealedDeliveryEnvelope` or a `MessageRequest` is accepted; profile-1 lengths are enforced. Other profile ids are checked for structure only. | New profiles add their lengths. |
| D10 | **IntroGate not called by Core.** | Tickets, requests and drain are implemented and tested; Core still deposits its `MessageRequest` into the inviter's mailbox from the invite descriptor (H2). `ticket_id` = 8-byte service prefix plus 8 random bytes (the frozen 16-byte id must carry routing). | The first-contact worker's flow adopts these; threat model 4.1-4.2. |
| D11 | **PlatformAdapter config.** The brief asked for relay configuration "via PlatformAdapter config". `PlatformAdapter` has no network-configuration family and `om-platform-api` is being changed by another worker. | Configuration lives in `apps/web/src/core/providerConfig.ts`: a user-chosen URL stored in the browser (written by `saveRelayUrl`, which a Settings screen should call), else the build default `VITE_OM_RELAY_URL`. https only, no credentials, query or fragment; plain http only for a loopback host in a build with `VITE_OM_ALLOW_LOOPBACK_RELAY=1`, which a production build refuses. | A Settings control for the URL and, if wanted, a `providerConfig` family in `om-platform-api`. There is deliberately no link- or query-based way to set it (it would let a link repoint the client). |
| D12 | **Not implemented, not advertised:** sponsorship (PC-D), multi-tenant host console (PC-B), recovery records (PC-F), blobs, `moved` forwarding (PC-E02), live `ProviderUsageState` (PC-C03), release-integrity cases (PC-I). | `moved` from a Home is mapped to `unsupported` by the adapter, never followed. | W0 conformance runner work; PR-013 needs the second (vendor-neutral) Home before any resilience claim. |

Also recorded: `wrangler` is pinned to 4.100.0 and `miniflare` to 4.20260611.0 (wrangler 4.120 and later depend on a miniflare 5 alpha line); `@cloudflare/workers-types` 4.20260611.1. `pnpm-workspace.yaml` now lists `infra/*` and sets `allowBuilds` to deny the esbuild, sharp and workerd install scripts (the install otherwise fails on pnpm 12; nothing needs them). Test-only files are not in the production bundle (`check-no-harness` also refuses the new e2e relay marker).

## Checks (2026-10-28)

- `pnpm web:check` (typecheck including `infra/cloudflare-home`, lint, 451 tests, production build, no-harness check): green.
- `make check`: green on a clean checkout of the `provider_reference` commits. In a shared working tree it also lists files other sessions have not registered yet; register only your own entries.
- `make web-e2e` (Chromium, Firefox, WebKit): 19 of 22 pass, including the new `relay.spec.ts` on all three engines. **`core.spec.ts` (the in-page loopback) fails on all three since ADR-0181 first contact (commit 318d6bb)**: it sends the first message the moment Alice accepts, before the relationship is bound, and Core answers `Rejected(unavailable)` (the same early-send race the relay run exposed, because a real network is slower than a loopback). A 3 s wait before sending makes it pass; I did not change that file. It belongs to the first-contact work: either the spec waits for the bound state, or Core queues a send while `ACCEPTED_UNBOUND`.

## Deploy plan (for the lead; nothing below has been run)

1. **Decide who operates it.** The operatorless rule means a user deploys their own Home ([`CLOUDFLARE_REFERENCE_DEPLOYMENT.md`](../architecture/providers/CLOUDFLARE_REFERENCE_DEPLOYMENT.md)). A SeedLife-run Home is a T1 host/sponsor role ([`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md)), an explicit operator decision, and `lonunjie/AGENTS.md` says production deploys that change what visitors see need go-ahead. A first test Home on `workers.dev` changes nothing visitors see.
2. **Config.** `infra/cloudflare-home/wrangler.jsonc` is complete except the placeholder `"name": "om-home-replace-me"`. Bindings: four Durable Object namespaces (`QUEUES`, `KEYS`, `DESCRIPTORS`, `INTRO`) created by migration `v1` (`new_sqlite_classes`); no KV, R2, Queue or D1. Variables: `OM_PROVIDER_ID`, `OM_HARD_FREE_TIER` (`"1"` on the Free plan). No secret and no account id is stored. `observability` is off; keep it off.
3. **Name.** Test: `om-home-<suffix>.<account-subdomain>.workers.dev` (zero domain, nothing in DNS). If SeedLife later hosts one: `home.om.lonunjie.org` as a Workers Custom Domain, with the record added to `infra/dns.tf` in `hq` (the zone has three record owners; never edit DNS in the dashboard) and a `routes` entry `{ "pattern": "home.om.lonunjie.org", "custom_domain": true }` in `wrangler.jsonc`. Do not reuse a Pages hostname.
4. **cf-mint token.** The Mac Keychain minter issues a short-lived token; mirror `bin/lonunjie-tofu`:
   ```sh
   cd /Users/maj/dig/lonunjie/om/infra/cloudflare-home
   export CLOUDFLARE_API_TOKEN="$(~/dig/bin/cf-mint token-mint --permission "Workers Scripts Write" --ttl 900 --name om-home-deploy 2>/dev/null | python3 -c 'import sys,json;print(json.load(sys.stdin)["result"]["value"])')"
   export CLOUDFLARE_ACCOUNT_ID=<from result.policies[0].resources of the same mint, per registry/tools.toml>
   pnpm exec wrangler deploy
   unset CLOUDFLARE_API_TOKEN
   ```
   Check the exact permission-group names with `cf-mint` before use (the Workers deploy needs account-level Workers Scripts edit; a custom domain also needs DNS Write on the zone, which `bin/lonunjie-tofu` already mints). Never print the token.
5. **Verify after deploy** (the same cases against the live URL, no credential needed):
   ```sh
   curl -s https://<name>.<subdomain>.workers.dev/.well-known/om-provider-capabilities | python3 -m json.tool
   cd /Users/maj/dig/lonunjie/om/apps/web && VITE_OM_RELAY_URL=https://<name>.<subdomain>.workers.dev pnpm build   # then open the preview and complete the two-identity flow
   ```
   Confirm in the Cloudflare dashboard that Workers Logs and Logpush are off, and set WAF rate-limit rules for forged-capability floods (the Worker deliberately cannot, section 5 of the threat model).
6. **Wire the client default** only after the operator accepts the privacy trade-offs: `VITE_OM_RELAY_URL` in the Pages build for `om.lonunjie.org`, or leave unset and let users enter their own Home.
7. **Plan limits to verify on the day** (volatile, do not encode): Workers Free Durable Object request, row-write and storage limits. A hard stop surfaces as a typed failure, never as data loss.

## What remains

1. Core: adopt `RelayEntry`/`RelaySet` (D1), relationship fetch capabilities (D4), the IntroGate flow (D10). Each is a Core change owned by the first-contact and M1 work.
2. A Settings control for the Home URL and for `lostMailboxes()` (D11, D3).
3. The conformance runner (`conformance/runner`), the second vendor-neutral Home (PR-013), and the PC-B, C03, D, E, F, I cases.
4. Independent review of this component (it handles capability and replay logic but no new cryptography).

## Deployed (2026-10-28, operator approved SeedLife's Cloudflare account)
- First test Home: `https://om-home-lonunjie.majal.workers.dev` (Worker `om-home-lonunjie`, free plan `OM_HARD_FREE_TIER=1`, Workers Logs off). Deployed with a cf-mint token (900 s, Workers Scripts Write), token not stored. Capabilities endpoint answers healthy.
- A first deploy briefly went out under the placeholder name `om-home-replace-me` (sed flag error); it was redeployed under the real name and the placeholder Worker deleted.
- Not yet done: WAF rate-limit rules for forged-capability floods; client default `VITE_OM_RELAY_URL` (left unset until the privacy trade-offs are accepted); custom domain `home.om.lonunjie.org`.
