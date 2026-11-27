# ADR-0280 — The LAN async-key reply stays unserved until a LAN sealing profile exists (delegated; additive)

- **Status:** Delegated design decision (additive). Independent review remains required. Completes ADR-0279 item 3 by taking the safest path.
- **Date:** 2026-11-28
- **Scope:** `local_delivery`, `scope-engines` only. No wire, registry or audited text changes.

1. The only sealing primitive Core has for an object addressed to a peer, `crypto::delivery::seal`, binds the ciphertext to a relay route (key id, relay address, delivery capability). A LAN reply has none of these. Supplying placeholder route values would invent a binding that no audited text defines, so it is not done.
2. Therefore the responder supplies no fetch grant, and every `fetch_async_key` gets the uniform `Unavailable` error. The responder, its capability table and its wiring (ADR-0279) stay in place, off in releases, so a defined LAN sealing profile can be connected without redesigning them.
3. Serving a bundle over LAN needs its own ADR that defines the sealed object (what it binds, to which key, and its replay and freshness rules), vectors in two codecs, and review. The Home path covers the need until then.
