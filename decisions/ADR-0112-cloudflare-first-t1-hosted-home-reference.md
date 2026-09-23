# ADR-0112 — Cloudflare is the first T1 hosted-Home reference; communities are the growth path

- **Status:** Accepted
- **Date:** 2026-09-24
- **Decided by:** operator ("we have to start somewhere. for this use case let it be cloudflare")
- **Amends:** ADR-0102

## Decision

1. OM grows through **hosted Homes**: one household member, club or organization runs a Home, and the people they
   invite get mailboxes there. T0 sponsorship and T1 hosting together form the invite loop.
2. The first T1 reference is the Cloudflare Worker + SQLite Durable Object code base, the same one used for T2 own
   cloud, and it is **multi-tenant from W0**. It has per-member Durable Objects and quotas, a host console, zero-billing
   fail-closed behavior, and at least 30 days of overlap routes when a member is evicted or the Home closes (R-187).
3. A T1 host **does not serve its members' client code by default** (R-188). Otherwise one phished host account would
   compromise every member's endpoint (T-042).
4. The vendor-neutral second reference Home (static binary + SQLite, V-020/PR-013) remains required before
   public-production claims. It is the T1 software for communities that prefer no cloud vendor.

## Consequences

`provider_reference` (W0) now includes multi-tenancy and the host console. V-015 needs a MetadataExposureDescriptor for
the T1 host role. V-020's vendor-block drill must count members of Cloudflare-hosted T1 Homes.
