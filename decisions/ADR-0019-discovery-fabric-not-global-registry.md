# ADR-0019 — Discovery Fabric, Not a Mandatory Global Registry

**Status:** Accepted

OM SHALL use a client-side Resolver and replaceable DirectoryProviders. Every discovery method resolves to a validated ContactDescriptor. Bare global usernames are UI conveniences with retained namespace provenance, not protocol-global identities. OM SHALL NOT require one global username registry, blockchain, or root namespace.
