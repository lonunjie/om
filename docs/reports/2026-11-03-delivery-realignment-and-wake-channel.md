# Delivery: where the shipped default departs from the tiers design, and the wake channel that stops constant polling

- **Date:** 2026-11-03
- **Scope:** `provider_reference`, `reference_client_ui` (one optional `core_kernel` hook). Decision: [`ADR-0185`](../../decisions/ADR-0185-home-wake-channel.md). Specification and privacy review: [`HOME_WAKE_CHANNEL.md`](../architecture/providers/HOME_WAKE_CHANNEL.md).
- **Answers the operator's questions:** why did delivery drift from "each person is their own relay, no centralized actor", and can the exchange with the relay be reduced to what is needed.
- **State:** the wake channel is built, tested and merged. **The Worker is not deployed**; section 5 says what to approve.
- **Sources read:** ADR-0102, ADR-0112, ADR-0130, the Home tiers document, the relay status and threat-model reports, the 2026-11-03 capacity report, `infra/cloudflare-home`, `packages/om-provider-http`.

## 1. Where the shipped default departs from the design

The design (ADR-0102, tiers document): the default first-run path is **T0**, a starter mailbox on the **inviter's** Home; people can **move to their own Home** with one tap; a RelaySet carries **more than one route**, with a fallback on **another provider family** (R-267, R-194); and "the reference distribution must not make one vendor the only zero-signup path".

What ships (om-web 0.0.10): one Worker, `om-home-lonunjie` on SeedLife's Cloudflare account, free plan, baked into every build by `apps/web/.env.production`. Everyone's mailbox is on it. The same operator also serves the app (`om.lonunjie.org`). Four departures, none recorded in an ADR as a decision (the nearest records are relay-status deviations D1, D10, D12 and the capacity report's "yes, it spends our quota"):

| # | Departure | Effect |
|---|---|---|
| 1 | **No T0.** Sponsorship (PC-D) is unbuilt (D12); the invite carries no ticket; the inviter's Home is not involved. | The only zero-signup path is SeedLife's Worker, which is what the tiers document forbids as the *only* path (section 6.5). |
| 2 | **No move-to-own-Home.** Settings can point a copy of OM at another Home, but both people in a chat must use the same one, so moving cuts you off from everyone on the default. | "One tap, with overlap" does not exist; users are locked to the default in practice. |
| 3 | **One route per mailbox.** Core binds a relationship to a single `RelayEntry`; the adapter refuses to deposit to a route at another Home (`rejected(unsupported)`); `moved` is mapped to `unsupported`. | No fallback route, no cross-Home delivery, no vendor-block resilience (the model's 2% to 43% improvement is unreachable). |
| 4 | **One vendor, one operator for mailbox and app.** Cloudflare is the only family, and SeedLife runs both the Home and the client origin. | ADR-0130 rule 4 (roles run by one operator collude by construction), R-188 and R-260 (a host should not also be the members' client origin; a project origin is offered next to independent origins and a native option, "never alone"). Today it is alone. Quota exhaustion or an account action stops delivery for everyone; end-to-end encryption is unaffected, availability is not. |

"Where did authority go?" today: for availability, to SeedLife. ADR-0183 (open beta, no resilience or independence claims, PR-013 claim-gated) is why this is acceptable as a *labelled beta*, but it records waivers, not a plan to close the gaps. Nothing in this report changes which Home is the default.

## 2. What it takes to close each gap, in dependency order

The order differs from the tiers document's, because later steps ride on earlier ones.

1. **Cross-Home delivery with a RelaySet of several routes (closes 3; prerequisite for 2 and 1).** Core: carry a list of `RelayEntry` per mailbox with priority and a signed lineage (D1 only adopted the single entry); `PollTarget` and `NetOp` name the route's Home. Adapter: a router that holds one `HttpProvider` per Home and dispatches by `relayUri` (a deposit already needs only the capability in the route; polling needs the owner's install secret at that Home); `allocateRoutes` offers routes at the owner's chosen Homes. Policy to decide first: a peer-supplied `relay_uri` makes the receiving client contact a host the peer chose (address disclosure), so connect only after acceptance and show the host. Tests: PC-A05, PC-E, V-020 vendor-block drill. This is the largest item and a Core change under `core_kernel`.
2. **Move to my own Home (closes 2).** Once routes can be several, a move is RelaySet migration with overlap: add a route at the new Home, publish it to contacts, drain the old route until the overlap ends (at least 30 days for a hosted Home, R-187), answer deposits at the old Home with a typed, verified `moved` (PC-E02; the adapter must follow it only with a verified proof). UI: Settings, plain words. Needs a way for a non-technical person to *have* an own Home, which is item 3 or a T1 host.
3. **Second, vendor-neutral Home (closes 4; PR-013; can start any time).** A static binary with embedded SQLite implementing the same HTTP API, plus the conformance runner (`conformance/runner`, PC-A..I), run against both. It is the T1 software for a family or club that wants no cloud vendor, and the first fallback route for item 1. It is independent of Core work, so a second agent can build it in parallel. The wake channel is easy there (any WebSocket server); the feature is advertised, so it is optional for it too.
4. **T0 sponsorship by the inviter's Home (closes 1).** `POST /v1/sponsorship/redeem`, `SponsoredMailboxGrant` quotas, single-use ticket, the 30-day overlap (PC-D06); the invite format carries a `SponsorshipTicket` (a Core invite change); first-run disclosure (section 3.3 of the tiers document); the client-origin choice (section 3.5). A sponsored mailbox is an ordinary RelaySet entry, so this rides on item 1. It only changes who runs delivery once a person other than SeedLife runs a Home, which is why items 3 and 2 come first.

## 3. Recommendation: what to ship first

1. **Now (this change):** the wake channel. It is safe, self-contained, optional, and removes most of the quota spend (section 4). Approve the Worker deploy (section 5); the client is compatible either way.
2. **Same day, operator decisions, no code:** move the Worker to the Workers Paid plan and add WAF rate-limit rules for forged-capability floods (both already listed as open). They remove the hard stop for everyone while the structural work happens.
3. **Next, small engineering:** keep a socket for a hidden tab (no pings, close events and the safety drain only), since hidden tabs now dominate the remaining spend; and find why idle polling ran at about 5 requests a second (section 4).
4. **Then the foundation:** cross-Home delivery with multi-route RelaySet (item 1), with the second Home (item 3) started in parallel as the test of it. Move-to-own-Home and T0 follow. Record the single-default-Home situation in an ADR with a sunset condition, so it reads as a decision with an exit, not as drift.

## 4. Event-driven delivery: what was built and what it measured

**Shape.** The Home's queue object accepts a hibernating WebSocket per connected client (`om.relay.wake.v1`, advertised in capabilities, `OM_WAKE=0` turns it off). A deposit sends the connected sockets one frame: the new sequence number. The client opens sockets only while the app is on screen, authenticates with the receive capability as the first message (nothing in the URL; the queue is named by a routing label in a header), and drains with the ordinary `drain` when told. While a socket is live and nothing newer is known, the runtime's timer still ticks but `poll` makes no request; a failed, refused, absent or hidden channel is plain polling; a five-minute safety drain bounds a lost wake. Tests: PC-W01..W06 in workerd, adapter and hub tests, and a real-Core run in which a one-minute poll timer still completes invite, introduction and a message in about 0.3 s of wake-driven work (the control, against a Home without the feature, does not in 12 s).

**Measured** with the opt-in live end-to-end spec (`pnpm --filter @om/web e2e:live`; Chromium, the production build; two fresh identities, three messages, then 120 s idle with both identities unlocked and the page visible). The spec now also counts sockets and frames and takes `OM_LIVE_IDLE_SECONDS`; `OM_LIVE_RELAY_URL` can point at a local Home (`node infra/cloudflare-home/test/serve.ts <port>`, `OM_WAKE=0` for the control) so the live Home is not spent on comparisons.

| Run | Home requests | During the 120 s idle | Send to visible in the other identity |
|---|---|---|---|
| Live Home, 0.0.10 client (baseline, no idle phase) | 62 in 21.9 s | not measured | 0.9 s, 1.9 s, 2.9 s |
| Live Home, new client (the Worker has no wake): falls back to polling | 60 in 24.3 s (two extra capabilities reads) | not measured | 1.4 s, 2.9 s, 1.9 s |
| Local Home, same Worker bundle, wake off (control) | 676 in 127 s (650 drains) | **596** | 0.92 s, 0.39 s, 0.41 s |
| Local Home, wake on | **38** in 125 s (12 drains) | **0** | 0.14 s, 0.10 s, 0.10 s |

With wake on, the Home also saw 5 socket upgrades (one per mailbox; each one Worker plus one Durable Object request) and 9 client frames (5 authentication frames and the keepalive pings, billed at a twentieth of a request each), so about 44 request-equivalents against 676: **about 94 percent fewer for the same two minutes, and none while idle.** Cloudflare's billing rules are quoted from memory and must be checked on the day ([`HOME_WAKE_CHANNEL.md`](../architecture/providers/HOME_WAKE_CHANNEL.md) section 6).

Two things the numbers show that the earlier estimate missed:

- **Polling cost scales with mailboxes, and idle was 5 requests a second.** One drain per mailbox per tick, five mailboxes for two identities, and about 5 requests a second at idle, more than the 15 s idle pace alone implies. A likely contributor is a flush that also polls every time Core schedules a network retry (every 2 s while any operation is pending); I did not chase it because the wake channel removes the cost, but a client talking to a Home without the feature still pays it. The capacity report's "1,440 requests a day for one hidden tab" is per mailbox.
- **A hidden tab is now the main remaining cost** (60 s polling per mailbox, all day), by design of the brief: sockets exist only while visible. Section 3 item 3.

**Web Push for a closed installed app: evaluated, not shipped** (details in [`HOME_WAKE_CHANNEL.md`](../architecture/providers/HOME_WAKE_CHANNEL.md) section 5): it adds a third party that sees every wake, a stable per-install identifier at the Home, VAPID keys held by whoever runs the Home (a SeedLife identity on every subscription today), a mandatory visible notification on iOS, and a locked vault that cannot show anything but a generic notice. It needs its own ADR, per-profile subscriptions, a declared linkage point and a privacy review (R-429, PR-006). The beta makes no background-delivery claim.

## 5. What the operator must approve to deploy the Worker

Nothing was deployed and no Cloudflare setting was touched. The change is in `infra/cloudflare-home` (`src/queue.ts`, `src/index.ts`, `src/capabilities.ts`) and `packages/om-home-wire`.

1. **Approve `wrangler deploy` of `om-home-lonunjie`** with a short-lived `cf-mint` token (Workers Scripts Write, 900 s), exactly as in the 2026-10-28 deploy plan, step 4. No new binding, no variable, no Durable Object migration (the same four classes; `MailboxQueue` only gains handlers), no stored data change. `wrangler.jsonc` is unchanged.
2. **Rollback** is `wrangler rollback` to the previous version, or setting the variable `OM_WAKE` to `0` and redeploying; clients then poll as before.
3. **After deploying, verify:** `curl -s https://om-home-lonunjie.majal.workers.dev/.well-known/om-provider-capabilities` lists `om.relay.wake.v1`; then `OM_LIVE_IDLE_SECONDS=120 pnpm --filter @om/web e2e:live` should report sockets above 0 and `idleRequests` near 0. Check the Durable Object request and duration graphs in the dashboard over a day.
4. **The web client** (a new om-web release) is a separate decision and follows the release process; it is compatible with the old and the new Worker (checked against the live Home). Shipping the client first changes nothing; shipping the Worker first changes nothing until clients update.
5. Not part of this approval and still open: the Paid plan, WAF rules, the custom domain.

## 6. Checks

`pnpm web:check` (typecheck including `infra/cloudflare-home`, lint, all unit tests, production build, no-harness check), the `infra/cloudflare-home` suite (workerd), `make check` and the architecture validators pass. The earlier stray local Home left on port 8788 by an old end-to-end run was stopped so the hermetic run used the new Worker.

**Browser end-to-end, honestly:** Chromium and Firefox pass in full. In full WebKit runs exactly one spec fails each time (about 1.5 minutes, a timeout), and **which one varies**: `navigation.spec.ts:60` in three runs with the wake channel on, `relay.spec.ts:22` in a run against a Home with `OM_WAKE=0` (the client then polls exactly as 0.0.10 does). Each of these specs passes when run alone on WebKit. So the failure is not specific to the wake channel; it looks like load-dependent WebKit slowness in a long serial run, but I did not establish the root cause and did not compare against a build of `main` before this change. Treat WebKit as a known unstable lane until someone does, and re-run it before releasing the client.

## 7. Not done, deliberately

- The default Home was not changed and the Worker was not deployed.
- No Web Push, no wake for hidden tabs, no change to polling pace, no Core protocol change.
- The departures in section 1 are not closed; section 2 is the plan, and each item is a decision for the operator before it is built.
