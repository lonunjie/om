# ADR-0279 — LAN async-key fetch responder (delegated; additive)

- **Status:** Delegated design decision (additive). Independent review remains required. Narrows ADR-0268 item 5 for the responder only.
- **Date:** 2026-11-28
- **Scope:** `local_delivery`, in the `scope-engines` variant only. No wire, registry or audited text changes; the `fetch_async_key` frame already exists (typed, exact re-encoding).

1. The responder answers a `fetch_async_key` frame only for a capability that this device issued earlier to that relationship, that is unexpired and unused, and only while the relationship is active and still grants async-key fetch authority (ADR-0158 section 7 step 5). Anything else gets the same `Unavailable` error frame, with no detail about which check failed.
2. Issued capabilities live in a bounded device-local table holding only a digest of each capability, the relationship, an expiry and a used flag. At most 64 entries; oldest expired entries are dropped first; a full table refuses to issue. Use marks the entry before the bundle is sealed, so a replay or a crash cannot serve it twice. Blocking, ending or retiring the relationship destroys its entries.
3. The reply is the existing signed async-key bundle, unchanged and sealed to the requester. The responder never mints, extends or widens authority and never serves a different relationship's bundle.
4. The module is sans-IO, behind `scope-engines`, off in releases, and no host supplies it in a release. The wasm main module is untouched.
5. **Offline first contact stays out of v1.** The reference invite carries no LAN secret, so it needs a new invite field, which is a wire change that this ruling cannot make. It needs its own wire ADR and review.
