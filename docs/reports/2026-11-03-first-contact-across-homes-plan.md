# First contact and delivery across Homes: design-checked plan

- **Date:** 2026-11-03
- **Scope:** `core_kernel` (invite and first-contact profile), `provider_reference` (adapter, Home), `reference_client_ui` (accept screen). No code in this change.
- **Why it exists:** [`2026-11-03-delivery-realignment-and-wake-channel`](2026-11-03-delivery-realignment-and-wake-channel.md) planned cross-Home delivery without reading the discovery design. The operator caught it. This plan is written from the design, with each claim tied to its source.
- **Design sources read for this plan:** [`DISCOVERY_FABRIC.md`](../architecture/discovery/DISCOVERY_FABRIC.md) (all), [`INVITES_INTROGATE_AND_SPAM.md`](../architecture/discovery/INVITES_INTROGATE_AND_SPAM.md) (all), [`CONTACT_SHARING.md`](../architecture/discovery/CONTACT_SHARING.md), [`KEY_TRANSPARENCY_AND_DIRECTORY.md`](../architecture/discovery/KEY_TRANSPARENCY_AND_DIRECTORY.md), [`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](../architecture/delivery/MULTI_RELAY_AND_DELIVERY_RESILIENCE.md) sections 1 to 6, [`PROVISIONING_AND_BOOTSTRAP.md`](../architecture/providers/PROVISIONING_AND_BOOTSTRAP.md) sections 2 and 10, [`CLOUDFLARE_REFERENCE_DEPLOYMENT.md`](../architecture/providers/CLOUDFLARE_REFERENCE_DEPLOYMENT.md) (model and guardrails), ADR-0019, ADR-0032, ADR-0043, ADR-0181, [`spec/cddl/discovery.cddl`](../../spec/cddl/discovery.cddl) and [`delivery.cddl`](../../spec/cddl/delivery.cddl). **Not read in full:** [`INTRODUCTION_AND_SERVICE_ACCOUNTS.md`](../architecture/discovery/INTRODUCTION_AND_SERVICE_ACCOUNTS.md) (grepped only), [`RELAY_AND_DELIVERY_ARCHITECTURE.md`](../architecture/delivery/RELAY_AND_DELIVERY_ARCHITECTURE.md) (grepped), [`REACHABILITY_AND_OBJECT_REPAIR.md`](../architecture/delivery/REACHABILITY_AND_OBJECT_REPAIR.md), ADR-0126 and ADR-0142 to 0147 beyond what ADR-0181 and the discovery docs quote. Read those before building the parts that touch them.

## 1. What the design says

1. **Identity is cryptographic; names are aliases; discovery is rendezvous; location is replaceable** (Discovery Fabric section 1). Every discovery path ends in a signed `ContactDescriptor` (section 7). There is no global root, directory or registry (ADR-0019, ADR-0043).
2. **An invite carries the way to its introduction service.** "It can carry enough information to find one or more `IntroductionService` endpoints and authenticate the invitation" (Fabric section 6). The `Invite` object has `intro_services[]` (Invites section 2). The descriptor has `introduction_services[]` (`intro-service-ref = {0: uri, ?1: key}`) and optional `relay_hints[]` ([`discovery.cddl`](../../spec/cddl/discovery.cddl)).
3. **The inviter's own Home is the introduction service.** A personal Worker's address `<worker>.<subdomain>.workers.dev` is the v1 locator (ADR-0032, Provisioning section 2). The locator is "a locator/alias, never AccountID". First contact through such a publisher is trust on first use, and the client says so (Key Transparency, "Trust on first use").
4. **Delivery is to the receiving side's relays, wherever they are.** A RelaySet is "the receiving side's set for one accepted relationship" (Multi-Relay 2.1); each side polls its own relay and the sender deposits to the route the recipient gave. The request carries a `reply_route` (a `relay-entry`, [`discovery.cddl`](../../spec/cddl/discovery.cddl)) at the requester's own relay. Several entries per set are for failover and rotation (Multi-Relay 4 and 6), not a precondition for talking across relays.
5. **Operatorless:** nothing in establishing or keeping a relationship may require an OM-operated service (ADR-0043). `monthly_paid_budget = 0` (Cloudflare Reference, guardrails). Each person owns their Worker account.

## 2. What ships, against that

| Design | Shipped (core 0.0.x, om-web 0.0.10, default Home) |
|---|---|
| Invite carries introduction service endpoints | Link is `om:invite/` + `invite_id (16) \|\| SHA-256(intro key) (32)`, 48 bytes, no locator (ADR-0181 section 2.1). The accepting client asks **its own** configured Home for the invite. |
| Descriptor lists the inviter's real `introduction_services` | A Core "stand-in" entry (`crates/om-core/src/intro.rs`, `flows.rs:1641`, deviation D10: "the introduction service route is still the pass-2 stand-in"). |
| Requester's `reply_route` is a relay entry at the requester's Home | Routes come from the adapter's `allocateRoutes`, all at the one Home. |
| Sender deposits to the recipient's route wherever it is | The adapter refuses a route at another Home: `rejected(unsupported)` (`provider.ts:260`). |
| Accepted peers exchange private routes / relationship fetch capabilities (D4) | D4 is unbuilt (relay-status "What remains"). |

Consequences: two people on different Homes cannot complete first contact, so an own Home is useless today; and the "same Home for both" warning in Settings is the shipped product's only answer.

**Correction to the earlier report.** It called a multi-entry RelaySet "the largest item" and a prerequisite. Reading Multi-Relay 2.1 and 4 shows the opposite: with **one** route per relationship, cross-Home delivery needs only (a) the invite and descriptor naming a Home and (b) an adapter that deposits to a route at its own Home and polls the owner's. The signed RelaySet lineage (several entries, versions, failover) is the resilience layer on top, and comes later.

## 3. Plan, in order

No wire (CDDL) change is needed: `introduction_services`, `relay_hints`, `relay-entry` and `reply_route` are already frozen.

**Step 1: the invite names the inviter's Home (closes departure 5).**
- Decide the link format (section 4 below). Core replaces the stand-in with a real `intro-service-ref` whose URI is the inviter's introduction service (their Home, from the provider offer) in the descriptor and in the link.
- Core: `AcceptInvitation` names the Home to ask (`NetOp` carries the introduction service ref); `put_invite` and `get_invite` name their Home. Link parse yields a locator.
- Adapter: a **router** holding one `HttpProvider` per Home (the configured Home plus any Home named by an invite or a route), dispatching by URI. It stores nothing new on disk beyond what Core already persists. Only `https` (or loopback in tests); no redirects; the existing probe is used before the first request to a new Home.
- UI: the accept screen names the host before anything is sent ("This invite uses `alice.workers.dev`") and says first contact through a personal publisher is trust on first use. No silent connection to a host chosen by someone else; the first request to a new Home follows the user's tap on Accept.

**Step 2: deposit to a route at another Home; poll your own (closes departure 3 for the single-route case).**
- Adapter: `deposit` goes to the Home named in the route's `relay_uri` through the router instead of `rejected(unsupported)`. A route carries its capability (given), so no secret leaves the owner's Home. `moved` stays typed.
- Core: `reply_route` and the Accept's mailbox route are the owner's actual route (any Home); nothing else changes because delivery keys already bind to the route (`route_binding`, D1).
- Relationship fetch capabilities (D4): device bundles are fetched "with the per-relationship capability" (Invites 5.1). Build D4 here, with the key pool at the Home that issued the capability, because a peer's bundle lives at the peer's Home.

**Step 3: prove it.** Conformance: DS-01 and DS-05 (`DISCOVERY` table), PC-A05 (no cross-Home reuse), V-020 vendor-block drill as far as one route allows. End to end: two workerd Homes on two ports, two real Cores: invite, accept, introduction, messages both ways, restart both, nothing sensitive at the wrong Home, a Home going down stalls only its owner's incoming mail. Browser: `relay.spec.ts` variant with two Homes. Live: a second Home needs a second Worker, which is a Cloudflare account change. That needs the operator's approval or a Worker in someone else's account.

**Step 4: a person can have their own Home without a terminal.** The Deploy-to-Cloudflare template of Provisioning section 10: a small verifier as the template's first step, deploying only a signed, digest-pinned release checked against TUF metadata, staying as the update controller; free plan, `OM_HARD_FREE_TIER=1`, no pasted API token. Depends on the release signing and TUF material ([`SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md`](../architecture/providers/SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md)), which is a separate body of work. It can run in parallel with steps 1 to 3 and does not touch Core.

**Step 5: move to my own Home, then resilience.** With steps 1 and 2, a move is publishing a new route at a new Home and draining the old one through the overlap (Multi-Relay "migration overlap", R-187). Then the signed RelaySet lineage with several entries and failover (Multi-Relay 2.1 and 4), then the second vendor-neutral Home (PR-013), then T0 sponsorship by the inviter's Home (Tiers section 4.1), which rides on step 1 because the sponsor is the inviter's Home.

## 4. Decisions the operator has to make (everything else follows the design)

1. **Link format.** ADR-0181 section 2.1 deliberately shrank the link to 48 bytes and put the introduction service in the descriptor it stores. But the invitee cannot find the descriptor without a locator, and the Fabric says the invite carries it. Recommendation: a new ADR amending ADR-0181 so the link is `invite_id || SHA-256(intro key) || locator`, the locator being the inviter's introduction service URI (up to 2,048 bytes in the CDDL, in practice a short `workers.dev` address), with the digest still authenticating the descriptor so a wrong locator can only fail, never substitute. This is a client-profile change, not a wire change.
2. **Who may be contacted on an invitee's tap.** Recommendation: any `https` Home, after the host is shown, with the existing capabilities probe first and nothing identifying sent before Accept; the user's tap is the consent. A deny-list or allow-list is not part of the design and is not proposed.
3. **Second Home for live testing:** approve one more Worker in SeedLife's Cloudflare account for a test, or use a volunteer's own account. Local two-Home tests need neither.

## 5. What this plan deliberately does not do

- No Paid plan, no WAF, no money or settings on SeedLife's account.
- No global directory, no OM-run lookup service, no default list of Homes (ADR-0019, ADR-0043).
- No change to which Home is the default, until step 1 and 2 ship and the sunset ADR says so.
- No RelaySet lineage work before it is needed for resilience.
