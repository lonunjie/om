# Provisioning and Bootstrap

## 1. v1 premise: user-owned Cloudflare deployment

> **2026-09-24 (ADR-0102):** this section now describes the **T2 own-cloud tier** only. New users normally start on a vendor-neutral T0 sponsored starter mailbox or a T1 hosted Home, and a second non-cloud reference provider is required before public production. See [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md).

The initial OM deployment model intentionally avoids a centrally operated OM relay fleet:

```text
Alice owns Cloudflare account -> Alice's OM Worker/DO resources
Bob owns Cloudflare account   -> Bob's OM Worker/DO resources
```

OM automates setup. The user owns the infrastructure account and can later replace Cloudflare without changing AccountID.

## 2. workers.dev as the zero-domain personal discovery endpoint

Cloudflare gives Workers accounts a configurable `<account-subdomain>.workers.dev` namespace and each Worker a `<worker>.<account-subdomain>.workers.dev` FQDN.

Therefore v1 can give an individual a real HTTPS discovery/rendezvous endpoint without buying a domain.

Example:

```text
https://om.maj-example.workers.dev/.well-known/om/contact
```

or an equivalent versioned endpoint served directly by the Worker.

This FQDN is a **locator/alias**, not AccountID. Renaming the worker/account subdomain or moving provider changes location, not identity. Existing accepted relationships receive authenticated endpoint updates.

Cloudflare recommends `workers.dev` primarily for personal/hobby/getting-started use; business-critical organizations should use their own domains/routes.

## 3. Personal publisher vs grouped directory

Do not confuse every user's Worker with a global search directory.

v1 roles:

- **PersonalDiscoveryPublisher** — serves one user's signed ContactDescriptor/IntroductionServiceDescriptor at their `workers.dev` FQDN;
- **DomainDirectoryProvider** — organizations/FQDN owners can resolve many aliases, e.g. `alice@example.org`, `bob@example.org`;
- **Direct discovery** — QR/link/contact sharing needs no searchable directory.

The first grouped directories should therefore naturally be companies/communities/domain owners rather than OM operating a universal username registry.

## 4. OAuth provisioning

Cloudflare supports public third-party OAuth clients and Authorization Code + PKCE for browser/mobile/desktop/CLI public clients. That avoids asking users to paste long-lived API tokens. It does **not** give resource-level least privilege: provider OAuth scopes follow API-token permission groups, and the ones provisioning needs (for example `Workers Scripts Write`, §9) are account-wide. A token that can deploy the OM Worker can replace every Worker in that account. OM docs and UI must not describe the grant as limited to OM's resources (ADR-0130 rule 2).

Token custody (R-262):

- the installer uses the access token only during the provisioning session and does not persist it in the PWA's
  origin storage, the ClientHost or any Home role;
- refresh tokens are not requested or kept by default. An optional in-client automation that keeps one stores it
  DeviceVault-sealed, lists it under Settings → Connected services with one-tap revoke, and asks for re-consent after
  the provider's shortest supported lifetime;
- the provider-side grant is revoked (or the user is told how to revoke it) when setup finishes;
- a project-owned OAuth client registration is a custodied asset (PROJECT_STEWARDSHIP §5), not a bootstrap root.

Provisioning flow:

```text
Create OM identity
 -> "Set up my free infrastructure"
 -> Cloudflare sign-in/consent
 -> OM provisions reference resources
 -> health/self-test
 -> publish signed ContactDescriptor
 -> done
```

The official Cloudflare OAuth client is a convenience, not a protocol authority. `ProvisioningProfile` must be replaceable so another OM distribution can use a different OAuth client or manual/CLI deployment.

Existing OM communication must continue if the official provisioning client is revoked or blocked.

## 5. Minimal zero-payment reference stack

To avoid requiring a billing subscription during basic onboarding:

```text
Worker
  HTTPS protocol/discovery/IntroGate surface

SQLite Durable Object
  bounded relay/mailbox state
  ticket/replay/rate-limit state
  small temporary encrypted spool
```

Do **not** require R2 or D1 merely because they exist.

Cloudflare documents SQLite Durable Objects on Workers Free; if a Free-plan limit is exceeded, further operations of that type fail rather than automatically becoming paid.

## 6. R2 as optional BlobStore expansion

R2 currently requires enabling an R2 subscription/checkout even though it includes a free usage allowance. Cloudflare billing documentation requires a payment method for purchasing products/services/subscriptions.

Therefore R2 is an optional expansion for:

- larger/offline encrypted attachments;
- encrypted backup/archive;
- larger blob retention.

Online peers should prefer direct P2P/LAN file transfer where policy/network permits. Small bounded offline blobs may use a carefully capped encrypted DO spool until a real BlobStore is configured. Exact size thresholds are implementation/benchmark decisions, not protocol constants.

## 7. D1 and Pages

- D1 is optional for richer organizational/search-directory workloads.
- Pages is optional for richer install/contact landing sites.
- The Worker itself can serve a minimal human install/contact page and machine discovery endpoint in v1.

Reducing components reduces setup friction and attack surface.

## 8. Escape hatch

Cloudflare-specific identifiers never enter canonical identity/conversation semantics. Migration exports signed configuration/state and replaces provider endpoints/capabilities without replacing AccountID.

## 9. workers.dev automation detail

Cloudflare exposes API operations to create/read the account Workers subdomain and enable a script on `workers.dev`; the account-subdomain creation endpoint accepts `Workers Scripts Write`. Since Cloudflare OAuth scope names correspond to API-token permissions, the provisioning profile requests that permission and automates this step after user consent. The permission is account-wide (§4), which is why the token is short-lived and not retained. If Cloudflare changes these scopes/API capabilities, provisioning falls back to an explicit user step without changing OM protocol semantics.

## 10. Operatorless provisioning path

Cloudflare's Deploy-to-Cloudflare mechanism can deploy a public Worker application into the user's own Cloudflare account and automatically provision supported resources from the project's configuration. This is now a preferred v1 path because no OM-operated OAuth/control plane is required.

**Every provisioning method deploys a verified release (ADR-0130 rule 2, R-261).** A template click clones a public
repository and builds it in the user's account, which would bind the deployed Home (and, for T2 PWA users, the
ClientHost) to a mutable forge branch plus build-time dependency resolution, outside the distribution's release trust
root. Therefore:

1. whatever the method (template, CLI, IaC, OAuth installer), what gets activated is a **signed release artifact
   pinned by digest** and verified against the distribution's TUF targets metadata (SOFTWARE_SUPPLY_CHAIN §4) before
   activation. No method activates code that is not a verified release;
2. a template repository contains a small **verifier/deployment controller** as its first step, not the application
   source build. It fetches the pinned release, checks the TUF metadata chain and the digest, and only then deploys
   the artifact. It never auto-deploys a branch head;
3. template deployments receive security fixes through the same staged, owner-pinned update flow as a ClientHost
   ([`CLIENTHOST_UPDATE_LIFECYCLE.md`](../client/CLIENTHOST_UPDATE_LIFECYCLE.md) §4): the controller left in the user's account discovers, verifies and
   stages new releases, so thousands of template deployments are not frozen at their first commit;
4. the deployment template repository is a custodied asset (PROJECT_STEWARDSHIP §5). Its compromise can change the
   verifier, so it is protected like release automation, mirrored, and its verifier is itself a signed, pinned
   release.

`ProvisioningMethod` is replaceable and may include:

- Deploy-to-Cloudflare/template deployment;
- manual Wrangler/CLI;
- Terraform/OpenTofu/IaC;
- optional Cloudflare OAuth client;
- future provider-specific installers.

No provisioning method is part of AccountID or messaging semantics.

A project-owned OAuth client is a convenience only, not the bootstrap root.

