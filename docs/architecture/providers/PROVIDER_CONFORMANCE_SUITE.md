# Provider Conformance Suite (Home providers)

- **Status:** Specification for V-020 criterion 1 (pre-engineering). The suite's executable runner is W0 engineering.
- **Applies to:** every reference Home implementation: the Cloudflare Worker + Durable Object Home (T1/T2, ADR-0112)
  and the vendor-neutral static binary + SQLite Home (T1/T3, PR-013). A Home that claims OM compatibility runs it too.
- **Rule:** a case either passes or fails. There is no vendor-specific branch, and no case may name a vendor.

The runner drives a Home only through its public provider contracts ([`IMPLEMENTATION_INTERFACES.md`](IMPLEMENTATION_INTERFACES.md): RelayProvider,
BlobStore, IntroGate, discovery publishing, sponsorship). It uses a black-box client and, for metadata cases, a trace
tap on the Home's own logs and network egress. Each case lists the requirements it proves.

## PC-A Capability semantics (R-052, R-092)

| ID | Case | Pass condition |
|---|---|---|
| PC-A01 | deposit with a valid delivery capability | accepted; receipt returned; envelope retrievable only via the matching receive capability |
| PC-A02 | deposit with a forged, truncated or other-mailbox capability | rejected; response indistinguishable in timing class and error code from "mailbox full" (no oracle) |
| PC-A03 | capability rotation | old capability rejected after the overlap window; new one accepted; no AccountID involved |
| PC-A04 | ack/delete | acked envelopes are gone from storage after the stated retention |
| PC-A05 | one capability, one relay | the same capability value presented to a second Home is not accepted there (no cross-relay reuse) |
| PC-A06 | Home never sees AccountID | no request, log line or stored record contains an AccountID or long-term identity key |

## PC-B Multi-tenancy and isolation (ADR-0112, R-187, R-188)

| ID | Case | Pass condition |
|---|---|---|
| PC-B01 | member A's capabilities cannot touch member B's mailbox | all cross-tenant attempts rejected |
| PC-B02 | per-member quota enforced independently | member A at quota does not affect member B |
| PC-B03 | host eviction | evicted member gets ≥30 days of overlap routes and a hand-off notice; deposits still accepted during overlap |
| PC-B04 | Home closure | same as B03 for all members; closure is announced through every member mailbox |
| PC-B05 | no ClientHost in the mailbox deployment | the reference host console deploys no client code; a member-facing ClientHost exists only with explicit opt-in (R-188) |

## PC-C Quotas and zero billing (R-016)

| ID | Case | Pass condition |
|---|---|---|
| PC-C01 | storage, rate, envelope-size and blob-spool limits | each limit rejects excess with a typed "quota" error |
| PC-C02 | budget exhaustion | fails closed; no request path can create a paid resource or overage |
| PC-C03 | usage_state honesty | reported usage matches the runner's own accounting within 1% |

## PC-D Sponsorship (T0, SponsoredMailboxGrant)

| ID | Case | Pass condition |
|---|---|---|
| PC-D01 | ticket redemption | a valid SponsorshipTicket creates exactly one StarterMailbox; a second redemption is rejected |
| PC-D02 | per-sponsor cap | the cap on concurrent sponsored mailboxes is enforced at the sponsor's Home |
| PC-D03 | no chaining | a sponsored mailbox with `may_sponsor_further=false` cannot issue tickets |
| PC-D04 | revocation and expiry | deposits rejected after revoke/expiry; the owner's identity and other routes are unaffected |
| PC-D05 | metadata disclosure | the grant carries a MetadataExposureDescriptor matching the V-015 trace model |

## PC-E Migration and overlap (R-001, R-052)

| ID | Case | Pass condition |
|---|---|---|
| PC-E01 | add a route on another Home | both routes deliver during overlap; no duplicate delivery to the client after dedupe |
| PC-E02 | remove the old route after overlap | deposits to the old route get a typed "moved" response for the forwarding window |
| PC-E03 | T0 → T2/T3 graduation | every contact that drains its mailbox, or sends to the old route, before the forwarding window closes keeps a route; AccountID unchanged (model: [`verification/bounded/home_tiers_model.py`](../../../verification/bounded/home_tiers_model.py)) |

## PC-F Recovery support (ADR-0111)

| ID | Case | Pass condition |
|---|---|---|
| PC-F01 | RecoveryRecord storage | stored under `H(RecoveryHandle)`, encrypted, and not linkable to a mailbox by the Home |
| PC-F02 | RecoveryRequest fan-out | a request reaches every controller route the Home serves |
| PC-F03 | rate limiting | repeated requests are rate-limited per account and deduplicated |

## PC-G Discovery descriptors and IntroGate

| ID | Case | Pass condition |
|---|---|---|
| PC-G01 | descriptor serving | the published ContactDescriptor verifies against its signature; stale versions are not served after replacement |
| PC-G02 | IntroGate ticket | single-use, expiring and revocable; unsolicited requests without a ticket are dropped before expensive work |

## PC-H Failure behavior

| ID | Case | Pass condition |
|---|---|---|
| PC-H01 | storage outage | writes fail with a typed retryable error; nothing is silently accepted and then lost |
| PC-H02 | restart | queued envelopes survive a restart (durability class stated in capabilities) |

## Running it (future)

`conformance/runner` (W0 engineering) executes every case against a Home URL. Before public-production claims,
PR-013 requires a passing report from both reference Homes. The report is published alongside the release's Protocol
Continuity Bundle.
