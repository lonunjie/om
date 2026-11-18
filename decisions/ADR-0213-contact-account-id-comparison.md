# ADR-0213 — Compare a contact's AccountID from the contact sheet

- **Status:** Accepted
- **Date:** 2026-11-18
- **Decided by:** operator delegation to fill the display and interaction gap in IS-02 (D129).
- **Scope:** `reference_client_ui` and the local `core_kernel` view contract.
- **Amends:** Nothing. The AccountID, relationship binding, verification evidence model and security floors retain their existing meaning.

## Design sources read

Read in full: [ADR-0010](ADR-0010-algorithm-independent-accountid.md), [ADR-0196](ADR-0196-local-contact-nickname.md), [ADR-0197](ADR-0197-lookalike-name-matching.md), [Identifier Spoofing and Safe Human Display](../docs/architecture/identity/IDENTIFIER_SPOOFING_AND_DISPLAY.md) and [Product and UX Principles](../docs/overview/PRODUCT_AND_UX_PRINCIPLES.md). Relevant sections read: D129 and D139 of the [relay status](../docs/reports/2026-10-28-relay-status.md), section 10 of [Groups and Spaces UX](../docs/ux-design/specs/GROUPS_AND_SPACES_UX.md), sections 14 and 15 of [Privacy and Security Interaction UX](../docs/ux-design/specs/PRIVACY_SECURITY_INTERACTION_UX.md) and the reserved fonts rules in [Themes, Fonts and Personalization](../docs/architecture/ux/THEMES_FONTS_AND_PERSONALIZATION.md). The full mapped code-path reading has not been completed.

## Decision

IS-02 calls for independently reachable safety-number or AccountID verification. The contact sheet offers **Verify account** for a live 1:1 conversation with authenticated AccountIDs on both sides. It reveals the full 32-byte AccountIDs in lowercase hexadecimal, reusing the existing Core representation. It labels each ID by whose account it identifies. The view adds only the current profile's own ID to the active conversation detail response, so either participant can show or read back their own ID. Restored, pending and unbound conversations have no account comparison.

The sheet asks the user to obtain the other person's own ID in person or through a channel they already trust. It compares the full entered value against the bound peer ID, ignoring outer whitespace and ASCII hexadecimal case. It shows match, mismatch or incomplete input in text. The codes use a built-in monospace font and left-to-right direction. Comparison input and result are transient and are cleared when the disclosure closes or the account context changes.

The result describes equality of the IDs. It is neither a saved verification attestation nor a signal to change send policy, accept recovery, relink a conversation or trust a new device. The client issues no message, lookup or provider call. A short fingerprint, a new format for safety numbers and a persisted user attestation are outside this decision.

## Where did authority go?

Core's authenticated relationship binding supplies the peer ID. The person chooses their independent evidence and compares it locally.

## Evidence

The Core loopback test checks each participant's own ID against the other participant's bound peer ID. Shared UI tests exercise full, partial and different IDs, the accessibility of the disclosure and result, and the absence of a check on unbound or restored history.
