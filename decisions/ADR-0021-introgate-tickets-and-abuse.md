# ADR-0021 — IntroGate and Short-Lived Introduction Tickets

**Status:** Accepted

Public discovery records SHALL expose an IntroductionServiceDescriptor rather than a reusable write capability. Senders obtain short-lived/single-use IntroductionTickets before submitting constrained MessageRequests. IntroGate enforces anti-spam/resource policy without access to normal conversation plaintext.
