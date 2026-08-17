# ADR-0001 — Providers Are Plugins

**Status:** Accepted

No infrastructure provider is part of the OM protocol. Relay, directory, storage, backup, recovery, push, and transport implementations sit behind narrow capability contracts. Moving a provider must not change AccountID or canonical conversation state.

Consequences: provider-specific optimization is optional; conformance tests matter more than vendor SDKs.
