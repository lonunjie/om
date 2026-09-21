# ADR-0025 — Optional Oblivious Discovery Privacy

**Status:** Accepted direction

Directory lookup SHOULD support an OHTTP/ODoH-inspired split mode in which a DiscoveryRelay learns the client network address but not the alias query, while the DirectoryResolver learns the query but not the client network address. The roles SHOULD be independently operated where practical.
