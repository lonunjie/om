# ADR-0089 — Baseline expressive media is bounded inert media

## Status
Accepted — 2026-09-23

## Decision
OM models stickers, custom emoji/reactions and looping animation semantically while using bounded static/animated WebP as the baseline interoperable expressive payload profile. Expressive assets are inert; no HTML/SVG script/Lottie/TGS execution is part of the baseline. A sent asset is rendered from its immutable OM BlobID plus fallback metadata and must not depend on a live pack, catalog or entitlement service.
