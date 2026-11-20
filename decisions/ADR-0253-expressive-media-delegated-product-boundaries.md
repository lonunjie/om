# ADR-0253 — Expressive-media product boundaries delegated to engineering (Q2, Q7 to Q11)

- **Status:** Accepted by delegation (2026-11-20), additive. No wire change. It amends no audited text and weakens no cryptographic floor. Every default is fail-closed: where the design is silent, the feature is left out.
- **Date:** 2026-11-20
- **Scope:** engineering scope `expressive_media` (ADR-0250), build-plan questions Q2 and Q7 to Q11.
- **Class:** Engineering and product boundaries. Companion of ADR-0252.

## Rulings

1. **Decoders (Q2).** OM ships no WebP codec and decodes no pixels in its own parsers. In the reference runtime the pixel decode is the platform's: the browser-native decoder in a worker for the PWA, the operating-system decoder elsewhere, reached only after the OM header walker and the `om.expressive.budget-v1` checks pass, under the 64 MiB resident ceiling and the 5 s limit of ADR-0251. In the verification prototype the candidate decoders are the reference libwebp (`dwebp` and `webpinfo`) and the pure-Rust `image-webp` crate, used only to cross-check the header walker's claims and to run the image-bomb corpus; slice 1 records the exact versions it pins beside its fixtures, and a pin change needs only a note there. The browser engines remain a manual-matrix check, not CI.
2. **No entitlement code (Q7).** None in W1: no entitlement presentation, proof scheme, verifier, price or commerce field. The design's registry-only entitlement interface stays unbuilt until a proof scheme is chosen by its own ADR and the operator's yes.
3. **Local only (Q8).** Installed packs, favourites and picker state are **local** to the device and profile. The account-private sync of ADR-0148 carries a closed allowlist (drafts, mute, archive and folders, bookmarks and pins, follow state, appearance) that does not include them; adding to it would amend audited text, so it is not done here. The Devices screen says that packs are per device. The question of extending the allowlist is recorded for the operator.
4. **Pack install needs explicit confirmation (Q9).** A pack named in a message is never installed by one tap or automatically. Installing shows the title, controller count and threshold, provenance, licensing, item count and size and asks for an explicit confirmation naming the pack; viewing a message with an asset from that pack never requires installing. Updates to an installed pack are shown as a lineage step and need the same confirmation. No fetch from a third-party URL happens to render a message.
5. **No external GIF source (Q10).** No GIF, APNG, AVIF or WebM import, no encoder and no catalog or search service are built. Loops and stickers come only from installed packs; local normalization of the person's own images waits for a later ADR that selects an encoder.
6. **No per-room disable (Q11).** The design has no room policy slot for expressive media, and a new one would extend group state. It is left out of W1: group stickers are ordinary `message.create` content under the existing attachment limits and personal blocking. Recorded for the operator.

## Consequences

Slice 1 to 6 UX and Core follow these; any change to rulings 3, 5 or 6 needs an operator decision and, where it touches audited text, an amending ADR.
