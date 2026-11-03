# ADR-0186 — Every person sets up their own relay; no built-in default relay; the deploy button belongs to onboarding

- **Status:** Accepted
- **Date:** 2026-11-03
- **Decided by:** operator (their words, 2026-11-03: "all users, even invited ones will need to have their own relay (unless it's a business/company/group invite where the group hosts their own relay and discovery)"; "The built-in default relay (SeedLife's) should be removed and not used"; "R2 comes later, not in onboarding"; "they better know it's Cloudflare, besides, they click it and make/login a Cloudflare account. it's good to be transparent"; "remove from repo and into the onboarding")
- **Amends:** [`ADR-0102`](ADR-0102-home-tiers-sponsored-starter-mailbox-and-provider-diversity.md) (the sponsored starter mailbox is no longer the default first-run path; sponsorship stays defined for hosts that are a business, company or group); [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../docs/architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md) section 7 (the page names the vendor).
- **Does not amend:** any wire object, crypto, key or route rule; [`CLOUDFLARE_REFERENCE_DEPLOYMENT.md`](../docs/architecture/providers/CLOUDFLARE_REFERENCE_DEPLOYMENT.md) (R2 stays out of the free baseline); ADR-0112; ADR-0183.

## Decision

1. **Own relay for everyone.** First run asks the person to set up their own relay (and the discovery address that rides on it); nobody starts on SeedLife's or on an inviter's relay. A group, business or company that runs a hosted relay and discovery for its members (T1) may invite members onto it.
2. **First method: Cloudflare, named.** The page says plainly that "Set up my own, free" uses Cloudflare and that the person will create or sign in to a Cloudflare account. Other methods (script, container, server, home machine, tunnel) are added later as further choices.
3. **R2 later.** The free one-click setup creates the relay only (Worker + SQLite Durable Object). Storage on R2 is an opt-in step afterwards, for people whose Cloudflare account already has billing and who want it.
4. **No built-in default relay.** The build carries no relay address. Until a relay is set up the app says so and offers setup.
5. **The deploy button is for each user.** It leaves the website repository and lives in the onboarding flow, deploying the person's own relay.

## Consequences

- People invited by a friend also set up a relay. The no-signup path of ADR-0102 remains only for hosted groups. The comprehension study (V-018) must test this page; the "Wi-Fi test" (R-010) now rests on the setup being short and guided.
- Delivery between people on different relays is required before this flow is usable: first contact and deposit across relays are built first ([`2026-11-03-first-contact-across-homes-plan`](../docs/reports/2026-11-03-first-contact-across-homes-plan.md)).
- The default-relay build setting, the website-repo deploy button and `release/` are removed. The released site changes only on the operator's approval of a push to the site repository.
