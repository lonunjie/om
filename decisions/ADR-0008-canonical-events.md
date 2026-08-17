# ADR-0008 — Small Immutable Canonical Event Model

**Status:** Accepted direction

OM durable conversation semantics use a small immutable event vocabulary with local materialized views. Authoritative identity/group control state remains in its own protocols. Ephemeral signals and local UI state are not durable conversation events.

Baseline durable kinds: message.create, message.edit, message.delete, reaction.set, receipt.update.
