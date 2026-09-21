# ADR-0015 — Defer SMS/MMS/RCS while preserving TransportAdapter

**Status:** Accepted

First implementation is OM-native secure messaging. SMS/MMS and RCS remain future TransportAdapters so store/carrier approvals cannot block initial delivery. No silent security downgrade to carrier transport.
