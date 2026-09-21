# ADR-0022 — Multi-Relay RelaySet

**Status:** Accepted

Accepted relationships MAY advertise multiple simultaneous relays. Each relay receives a distinct opaque capability. Normal delivery selects one healthy route; failover is automatic; optional redundant delivery is allowed for high-reliability policy and deduplicated by authenticated EventID.
