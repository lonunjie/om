# ADR-0046 — Provisioning Is Replaceable and Need Not Use an OM OAuth Service

**Status:** Accepted

Deploy-to-provider templates/public deployment flows are preferred where possible because they provision directly into the user's provider account. OAuth automation is optional.

Provisioning method never enters AccountID or message semantics.
