# Security / Continuity / Bootstrap References — verified 2026-09-21

This file records moving external facts used by the 21:32 PHT checkpoint. Provider limits and Internet-Drafts must be rechecked before implementation/release.

## Cloudflare bootstrap

- Cloudflare Workers `workers.dev` routing (updated 2026-08-14): every Workers account has a configurable `<account-subdomain>.workers.dev`; each Worker receives `<worker>.<account-subdomain>.workers.dev`. Cloudflare recommends custom domains/routes for production/business-critical use.  
  https://developers.cloudflare.com/workers/configuration/routing/workers-dev/

- Cloudflare OAuth clients (updated 2026-08-20): public third-party clients are supported; browser/mobile/desktop/CLI clients use Authorization Code + PKCE with `S256` and no embedded client secret. OAuth is available on Free accounts.  
  https://developers.cloudflare.com/fundamentals/oauth/create-an-oauth-client/  
  https://developers.cloudflare.com/fundamentals/oauth/

- Durable Objects pricing (updated 2026-08-25): SQLite-backed Durable Objects are available on Workers Free; exceeding a Free-plan limit causes further operations of that type to fail until reset rather than transparent paid overage.  
  https://developers.cloudflare.com/durable-objects/platform/pricing/

- Durable Objects limits (updated 2026-06-01): Workers Free has 5 GB total SQLite-backed DO storage and other documented limits; implementation must recheck current values.  
  https://developers.cloudflare.com/durable-objects/platform/limits/

- R2 get started (updated 2026-04-21): an R2 subscription is required; enabling it uses a checkout flow even though R2 includes free monthly usage.  
  https://developers.cloudflare.com/r2/get-started/

- Cloudflare billing profile (updated 2026-05-29): a primary payment method is required to purchase Cloudflare products/services. R2 is a usage-based service/subscription.  
  https://developers.cloudflare.com/billing/get-started/create-billing-profile/  
  https://developers.cloudflare.com/billing/understand/billing-policy/

## Recovery / crypto agility

- NIST CSWP 39upd1, *Considerations for Achieving Crypto Agility* (final, updated 2026-06-29): crypto agility is the ability to replace/adapt algorithms while preserving security and operations; supports OM's versioned CryptoProfile and algorithm-independent identity direction.  
  https://csrc.nist.gov/pubs/cswp/39/upd1/considerations-for-achieving-crypto-agility/final

- NIST IR 8214C, *NIST First Call for Multi-Party Threshold Schemes* (Jan 2026): threshold constructions are actively being collected/standardized; OM reserves a future threshold-recovery interface rather than inventing bespoke PQ threshold signing now.  
  https://csrc.nist.gov/pubs/ir/8214/c/final

- SLIP-0039 Shamir mnemonic secret sharing: useful prior art; specifies 33-word shares for 256-bit share values. Adoption still requires OM implementation/security review.  
  https://slips.readthedocs.io/en/latest/slip-0039/

## Control-state transparency

- IETF Key Transparency Architecture draft-ietf-keytrans-architecture-09 (2026-06-30): current work-in-progress architecture for consistency/equivocation detection in secure messaging. OM's AccountControlHead gossip borrows the detection-vs-authority separation but does not make this draft a mandatory protocol dependency.  
  https://datatracker.ietf.org/doc/html/draft-ietf-keytrans-architecture-09

## Notifications

- Apple, *Pushing background updates to your app*: background notifications are low priority, not guaranteed, and may be throttled. OM must not claim silent push alone gives reliable instant delivery.  
  https://developer.apple.com/documentation/UserNotifications/pushing-background-updates-to-your-app

- Signal Android project discussions/implementation history confirm a persistent WebSocket-style fallback is used when Play Services is absent; this is useful prior art but OM must validate its own battery/reliability behavior.  
  https://github.com/signalapp/Signal-Android/issues/12490  
  https://github.com/signalapp/Signal-Android/issues/14793

## Software update / supply chain

- The Update Framework security model: rollback/freeze/key-compromise defenses, expiring trust, role compartmentalization and threshold keys.  
  https://theupdateframework.io/docs/security/

- SLSA Build/Provenance: provenance binds artifacts to build process/builder/source/dependencies and higher build levels require stronger provenance isolation.  
  https://slsa.dev/spec/v1.0/requirements  
  https://slsa.dev/spec/v1.2/build-provenance

- in-toto: verifies authorized software-supply-chain steps and signed materials/products metadata.  
  https://in-toto.io/docs/getting-started/

- Reproducible Builds: byte-for-byte independent reproduction can detect build-host/compiler-pipeline compromise and map binaries back to source.  
  https://reproducible-builds.org/docs/definition/  
  https://reproducible-builds.org/specs/source-date-epoch/

## Abuse admission

- RFC 9576, Privacy Pass Architecture: unlinkable authorization proofs can support abuse/resource admission without stable per-client tracking.  
  https://www.rfc-editor.org/rfc/rfc9576.html

## Cloudflare workers.dev automation detail

- Cloudflare API exposes account-level Workers subdomain get/create/delete operations; creating the account `workers.dev` subdomain accepts `Workers Scripts Write`. Script-level API operations can enable/disable a Worker's `workers.dev` route. Combined with public OAuth scopes that map to API-token permission names, this supports automating the personal `workers.dev` locator where the user grants the relevant scope.  
  https://developers.cloudflare.com/api/resources/workers/subresources/subdomains/methods/update/  
  https://developers.cloudflare.com/api/resources/workers/subresources/scripts/subresources/subdomain/
