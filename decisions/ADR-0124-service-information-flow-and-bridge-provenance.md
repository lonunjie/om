# ADR-0124 — Service information flow and bridge provenance

- **Status:** Accepted
- **Date:** 2026-09-24
- **Decided by:** operator (delegated 2026-09-24: "draft the design gap rulings ... check it and accept it if good")
- **Source:** V-011 gaps 5–6 ([`v011-services`](../verification/prototypes/v011-services/README.md)) and V-013
  ([`v013-bridges`](../verification/prototypes/v013-bridges/README.md), contract §10). **Amends:**
  [`SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md`](../docs/architecture/ecosystem/SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md), [`INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md`](../docs/architecture/ecosystem/INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md)
  (supersedes the bridge-scoped part of the 2026-09-23 metadata refinement). **Amended by:** ADR-0129 (rule 2 scope).

## Decision

### Services

1. **A trigger is a disclosure.** The message that invokes a service, such as a mention, is a plaintext disclosure to
   that service. It appears in disclosure receipts and taints the invocation.
2. **Taint, not content filtering.** An invocation that has read Room plaintext, including its trigger, is tainted.
   A tainted invocation gets **no network egress** unless the Room policy explicitly grants `plaintext_egress`.
   Content filtering is not a control, because exfiltration can be encoded.

### Bridges

3. **Fingerprint scope.** `origin_fingerprint` is a domain-separated hash over (origin network, foreign origin
   identifier), keyed with a Room-scoped secret that every admitted bridge can derive. The bridge contract fixes the
   derivation. It is not keyed per bridge, so deduplication works across independent bridges in one Room, and
   it still never gives one fingerprint that is stable across Rooms (the A-013 privacy aim).
4. **Unverified origin.** An event that reaches OM through a relay that strips interop metadata carries the fidelity
   flag `origin_unverified`. The adapter capability `carries_interop_metadata` declares whether a foreign path
   preserves provenance, and a Room policy may refuse bridges or topologies without it.
5. **No bridge-to-bridge relay by default.** Relaying events that one bridge imported out through another bridge is
   off unless the Room policy opts in.
6. **No retroactive export.** A bridge may export only epochs within its own membership intervals. Re-admission does
   not grant history from before the new interval.

## Consequences

V-013 criterion 2 stands. The bridge adapter contract (criterion 1) still needs its non-author review, and that
review must check these rules. V-011 gaps 5–6 are closed as design work.
