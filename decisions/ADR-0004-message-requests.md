# ADR-0004 — Unknown Senders Use Message Requests

**Status:** Accepted

Public discovery resolves to a constrained IntroductionEndpoint, never an unrestricted mailbox. Unknown senders can send only a cheap/rate-limited request until accepted. Acceptance creates a private RelationshipBinding/delivery capability.
