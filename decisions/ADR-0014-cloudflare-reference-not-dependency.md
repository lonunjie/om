# ADR-0014 — Cloudflare as reference provider, not dependency

**Status:** Accepted

Workers/Durable Objects may implement a relay, R2 may implement encrypted blob storage, and Realtime may implement STUN/TURN/SFU. OM wire semantics and identity never depend on Cloudflare. Quota/cost behavior lives in provider policy/adapters.
