# ADR-0013 — SFrame + adaptive LAN/P2P/TURN/SFU calling

**Status:** Accepted

Call signaling is encrypted OM control traffic. Media is protected with SFrame independently of route. RoutePolicy prefers safe LAN/direct P2P when policy permits, uses TURN for connectivity/privacy fallback, and SFU for scalable group calls. Route changes never silently alter content confidentiality.
