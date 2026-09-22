# Extension / Content / Bot Ecosystem Research Notes — 2026-09-23

This is non-normative evidence used by the 07:18 PHT OM architecture hardening pass.

## Telegram expressive media

Official Telegram sticker documentation demonstrates a useful product baseline: static image stickers, short animated/video stickers, custom emoji, pack sharing/import and emoji/keyword association. Telegram currently accepts PNG/WebP for static art and has TGS/WebM profiles for animation/video. OM takes the user-facing lessons but deliberately avoids making Telegram-specific TGS/Lottie or Telegram servers/catalogs protocol dependencies.

- https://core.telegram.org/stickers
- https://core.telegram.org/api/stickers
- https://core.telegram.org/import-stickers

## Telegram bots / Mini Apps

Telegram distinguishes bots from ordinary users and exposes explicit launch contexts for Mini Apps. Some Mini App launch modes receive chat/user context while direct-link modes can be constrained and do not automatically read/send chat messages. OM adopts the distinction between installation/launch context and actual data/action authority, but uses an explicit principal/capability model rather than Telegram's centralized bot platform.

- https://core.telegram.org/bots
- https://core.telegram.org/bots/webapps
- https://core.telegram.org/bots/api

## Discord application commands

Discord separates application installation context, interaction context and command permissions. That separation is valuable: where an app is installed should not itself imply every data/action privilege. OM adopts that conceptual separation while avoiding Discord's centralized application registry and Administrator bypass semantics.

- https://docs.discord.com/developers/docs/interactions/slash-commands
- https://discord.com/developers/docs/topics/oauth2

## Matrix Application Services

Matrix Application Services are a useful example of a standardized integration plane independent of one homeserver implementation, but their configured event-observation model can be broad. OM uses the lesson that integrations need published APIs while defaulting to narrower selected-context/service-recipient disclosure rather than a privileged always-observing homeserver hook.

- https://spec.matrix.org/latest/application-service-api/

## WebAssembly Component Model / WASI

The Component Model provides typed import/export contracts; components interact only through declared interfaces. WASI explicitly describes a capability-based sandbox with no ambient authority. WASI 0.3 shipped in June 2026 and is supported by modern Wasmtime/jco tooling. This is a strong fit for OM's desired "explicit capability imports, no secret-holding process ambient access" plugin model, but cross-platform/PWA parity still needs an implementation proof gate.

- https://wasi.dev/
- https://wasi.dev/roadmap
- https://component-model.bytecodealliance.org/design/worlds.html
- https://component-model.bytecodealliance.org/design/components.html

## W3C Verifiable Credentials

VC Data Model 2.0 formalizes issuer/holder/verifier roles for cryptographically verifiable claims. OM does not adopt JSON-LD/VC as a mandatory dependency, but the three-party model is useful for optional creator entitlement presentations: an external issuer asserts a scoped entitlement, a holder presents it, and an independent distributor verifies it.

- https://www.w3.org/TR/vc-data-model-2.0/

## MDN / WebP

Current MDN format guidance documents WebP as supporting static and animated images, transparency and broad current browser support. OM therefore selects bounded WebP as the smallest baseline expressive-media payload profile, while leaving optional future codecs to capability negotiation.

- https://developer.mozilla.org/en-US/docs/Web/Media/Guides/Formats/Image_types
