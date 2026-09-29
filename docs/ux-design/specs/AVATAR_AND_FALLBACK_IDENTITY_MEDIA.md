# OM Avatar and Fallback Identity Media

**Status:** Normative Run 11B specification.

## 1. Goals

Fallback avatars should help recognition without pretending to know personal attributes. They should be stable, distinguishable, theme-safe, and local to the identity the client already knows.

## 2. Never infer human traits

OM MUST NOT infer or encode apparent gender, ethnicity, age, religion, mood, occupation, or other human attributes into generated fallback avatars.

Fallbacks are abstract identity markers, not portraits.

## 3. Seed safety

A deterministic fallback may be derived from a stable opaque identifier already available to the client for display purposes, plus an explicit avatar-style version domain separator.

It MUST NOT be derived from:
- private keys;
- recovery secrets;
- passwords;
- phone numbers or email addresses solely for avatar generation;
- message content;
- behavioral tracking.

Changing the avatar algorithm version must not change identity semantics.

## 4. Distinguishability

Initials alone are insufficient because many people share initials. The default fallback combines:
- optional initials or short group letters;
- one of several non-semantic background hues;
- a deterministic abstract geometric pattern.

Two contacts with the same displayed initials should usually remain visually distinct.

## 5. Color rules

Fallback palettes avoid OM's protected danger/warning/success families as primary identity cues. Avatar color is identity decoration, not status.

Text/foreground contrast must pass normal OM contrast requirements. Identity remains distinguishable without relying on exact hue.

## 6. User images

User-supplied avatars supersede fallback art. Clients should crop/scale locally and avoid destructive re-encoding of originals unless the user chooses to publish an optimized derivative.

Remote avatar fetching and privacy policy belong to protocol/provider design, not the visual spec.

## 7. Accessibility

An avatar image is usually redundant with adjacent contact text and may be hidden from assistive technology. Where the image itself is the only identity cue, the consuming component supplies the accessible name.

## 8. Reference generator

`assets/avatar/v0/generate_avatar.py` is a design/reference generator. It demonstrates the invariant: stable abstract pattern + initials + safe palette. It is not frozen protocol behavior and must not be used to derive cryptographic identity.
