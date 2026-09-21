# Discovery / Resilience Reference Notes — 2026-09-21

These are architectural references, not protocol dependencies.

- RFC 9518 — *Centralization, Decentralization, and Internet Standards*: useful framing that decentralization has dimensions; DNS distributes lookup but retains a globally coordinated namespace. https://www.rfc-editor.org/rfc/rfc9518.html
- RFC 9230 — *Oblivious DNS over HTTPS*: reference for splitting client identity from plaintext resolver query. https://www.rfc-editor.org/rfc/rfc9230.html
- RFC 9458 — *Oblivious HTTP*: generic relay/gateway design in which gateway does not learn client IP and relay does not learn plaintext HTTP content. https://www.rfc-editor.org/rfc/rfc9458.html
- RFC 9576 — *The Privacy Pass Architecture*: candidate architectural model for privacy-preserving anti-abuse/admission proofs. https://www.rfc-editor.org/rfc/rfc9576.html
- RFC 8882 — *DNS-SD Privacy and Security Requirements*: guidance to avoid stable/linkable identifiers and unnecessary disclosure during local discovery. https://www.rfc-editor.org/rfc/rfc8882.html
- RFC 9665 — *Service Registration Protocol for DNS-Based Service Discovery*: reference for unicast discovery registration and public-key-defended registrations. https://www.rfc-editor.org/rfc/rfc9665.html
- RFC 9484 — *Proxying IP in HTTP*: MASQUE-family reference for future proxy transport options; not an OM requirement. https://www.rfc-editor.org/rfc/rfc9484.html

OM deliberately does not copy DNS's single global root and does not mandate OHTTP/Privacy Pass/SRP/MASQUE. These are design inputs for replaceable components.
