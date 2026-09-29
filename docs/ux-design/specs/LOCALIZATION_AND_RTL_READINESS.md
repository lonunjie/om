# Localization and RTL Readiness

**Status:** Normative internationalization contract

## Goal

OM's interaction language should survive translation, text expansion, different writing directions, culturally different formatting, mixed-script names/messages, and locale-specific input without becoming a different product.

## Internationalize before translating

All user-facing strings must be externalizable. Do not construct sentences by concatenating translated fragments. Use locale-aware plural/select/message formatting appropriate to the target platform.

Names, handles, group titles, message text, filenames, and user-authored labels are **opaque user content**, not Western first-name/last-name fields.

## Layout direction

Use logical concepts:

- leading / trailing;
- start / end;
- before / after;

rather than hard-coded left/right wherever direction follows language.

System frameworks should perform normal RTL layout reversal. OM-specific exceptions follow the icon contract: directional navigation/reply/forward symbols may mirror when meaning follows reading flow; cameras, microphones, QR codes, checkmarks, media thumbnails, and other physically/non-directionally meaningful artwork do not mirror automatically.

## Mixed-direction user content

On the web:

- use `dir="auto"` or `<bdi>` around unknown-direction names, handles, filenames, snippets, and user-authored text when isolation is needed;
- do not infer global interface direction from one user's name/message;
- preserve the original Unicode string;
- avoid inserting raw bidi-control characters except in narrow standards-backed cases where markup cannot be used.

Equivalent isolation must exist on native platforms.

## Search and Omnibar

The Omnibar may normalize for matching but never rewrites stored user content.

Safe candidate-generation techniques may include:

- Unicode normalization;
- locale-aware case handling;
- diacritic-insensitive secondary matching where appropriate;
- tokenization suited to scripts that do not use spaces;
- optional transliteration aliases only when local, reversible, and clearly secondary.

Exact/strong literal matches outrank fuzzy/transliterated matches. Search should not silently merge distinct identities because transliteration produces the same Latin spelling.

## Dates, times, numbers, and names

Use locale-aware system formatters for:

- date/time;
- relative time where linguistically appropriate;
- number grouping;
- file sizes;
- durations;
- phone-number presentation where relevant.

Protocol/storage timestamps remain machine formats internally; UI formatting is locale-specific.

## Text expansion and vertical needs

No essential control assumes English string length. Validation includes:

- approximately 2× pseudo-expanded strings;
- RTL pseudo-language;
- accented/tall pseudo-language;
- 200% text;
- long names/group titles;
- multiline button/setting labels where needed.

Prefer wrapping or responsive reflow to truncating essential actions. Truncation may be used for long user content when the full value remains discoverable/accessibly named.

## Translation of message content

OM does not automatically send message contents to a remote translation service merely because the UI locale differs.

If translation is later offered:

- user intent is explicit;
- local/on-device processing is preferred;
- remote processing discloses what leaves the device;
- the original message remains available;
- translation is never mistaken for the sender's original wording.

## Cultural semantics

Core status meaning never depends on a culturally ambiguous color or illustration alone. Theme/localization review may tune decorative imagery and wording, but cannot redefine danger, verification, failure, or success semantics.
