# Current UI/UX/security references — 2026-09-22

These references informed the 13:42 PHT UI/UX checkpoint. They are supporting research, not normative protocol dependencies.

## Accessibility

- W3C ARIA Authoring Practices Guide: https://www.w3.org/WAI/ARIA/apg/
- W3C APG keyboard-interface guidance: https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/
- W3C WCAG / accessibility standards landing pages: https://www.w3.org/WAI/standards-guidelines/

Design consequence: accessibility semantics and keyboard/focus behavior live in shared UI primitives rather than being reinvented per platform shell.

## Adaptive layout

- Android Developers list-detail adaptive layout guidance: https://developer.android.com/develop/adaptive-apps/guides/list-detail

Design consequence: OM layouts adapt to available window size rather than rigid phone/tablet/desktop product forks.

## Web/PWA injection hardening

- MDN Content Security Policy guide: https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/CSP
- MDN Trusted Types API: https://developer.mozilla.org/en-US/docs/Web/API/Trusted_Types_API
- MDN `require-trusted-types-for`: https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/require-trusted-types-for

As of 2026, Trusted Types has much broader baseline availability than in earlier web generations. OM theme files remain declarative regardless; CSP/Trusted Types provide defense in depth for the PWA shell.

## Local fonts

- MDN Local Font Access API: https://developer.mozilla.org/en-US/docs/Web/API/Local_Font_Access_API

Design consequence: if OM later allows selecting fonts installed on the device, the operation is explicit and permission-gated where supported. OM does not enumerate/upload font lists silently and does not depend on the API for basic custom-font support.
