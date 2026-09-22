# Rich Text and Hostile Content Security

## Product goal

OM should meet or exceed Telegram-style message formatting while keeping the wire model small, deterministic, accessible and non-executable.

## RichTextV1

Do not send HTML, DOM fragments, CSS or Markdown as authoritative wire content.

Use a semantic tree:

```text
RichTextV1
  blocks[]

Block
  Paragraph(inlines[])
  Quote(inlines[])
  CodeBlock(text, language_hint?)
  List(ordered, items[])       # optional in v1 if implementation is ready

Inline
  Text(text, marks[])
  Link(label_inlines, url)
  Mention(display_inlines, subject_ref)

Mark
  bold
  italic
  underline
  strikethrough
  spoiler
  inline_code
```

Plain messages are one Paragraph containing one Text run.

## Why a semantic tree

It avoids:

- HTML/XSS parser surface;
- Markdown ambiguity/version differences;
- UTF-16/codepoint/range-index mismatches between clients;
- invalid overlapping formatting spans;
- executable embeds.

Use compact integer discriminants in deterministic CBOR on the real wire; the JSON schema is explanatory only.

## Limits

Before allocation/rendering, enforce versioned limits such as:

- total UTF-8 bytes;
- block count;
- inline-node count;
- nesting depth;
- URL bytes;
- code-block bytes;
- mention count.

Exact values are benchmark/configuration constants, not crypto or identity semantics.

## Links

Allowed link schemes are an explicit registry. `https` is normal. `http` may require a warning. `om:` is handled by OM deep-link validation. Dangerous active schemes (`javascript:`, `data:` for arbitrary content, etc.) are rejected.

Display the actual destination on long-press/hover/context action. Themes cannot change link destinations or hide external-link semantics.

## Paste/import

The composer MAY parse Markdown-like syntax or formatted clipboard content locally, but it converts it into RichTextV1 through a sanitizer. Clipboard HTML is never trusted/rendered directly.

## Code and quotes

Code blocks are text only. A language hint is presentation metadata and must never invoke downloaded code/parsers. Syntax highlighting, if added, is local and uses bounded reviewed highlighters.

## Spoilers

Spoiler is a visual disclosure affordance, not encryption/access control. Screen readers and copy/export behavior must have explicit accessible semantics.

## Mentions

Mentions refer to an identity/member reference valid within the encrypted conversation/group state. Do not turn a mention into a public lookup request while rendering.

## Hostile content boundary

Every decrypted attachment remains untrusted:

```text
ciphertext -> authenticated decrypt -> UNTRUSTED BYTES -> validate -> safe decoder/sandbox -> render
```

Never trust filename extension, claimed MIME type or metadata alone.

### Baseline attachment policy

- allowlist supported preview formats;
- hard byte/pixel/duration/decompression limits;
- random internal filenames/opaque IDs;
- no automatic archive extraction;
- no inline active SVG/HTML;
- PDFs/documents open through platform-safe viewer/download policy rather than becoming OM DOM;
- strip or selectively expose metadata according to user policy;
- parsers/decoders are dependency-audited and fuzzed;
- malformed preview falls back to safe generic attachment UI.

## Images/video/audio

Decode through browser/OS sandboxed media facilities where practical. Do not write custom codecs. Protect against image bombs and absurd dimensions before full render where APIs permit.

## Link previews

Automatic direct fetching can disclose recipient IP/activity. Define `LinkPreviewProvider`:

- Recommended: privacy-preserving proxy/provider when configured;
- Hardened: restrictive proxy/no prefetch policy;
- Maximum Privacy: off by default;
- sender can always remove a generated preview before sending.

A preview fetched through the user's own Worker is not automatically anonymous from that infrastructure provider; document the actual threat model.

## No message compression oracle

Do not compress attacker-controlled and secret plaintext together before encryption merely to save wire bytes. Compression must undergo a dedicated side-channel review.
## 2026-09-22 hardening — LinkPreviewProvider SSRF boundary

A server/provider that resolves arbitrary user links is an SSRF-sensitive component. Reference implementations MUST reject or safely isolate loopback, link-local, private/internal ranges, cloud metadata endpoints and unsupported schemes; pin/validate resolved destinations; revalidate every redirect; defend against DNS rebinding; and bound redirect count, response bytes, decompression, content types and time.

Link previews are optional enrichment. Failure to preview never changes message authenticity or link destination semantics. Maximum Privacy remains no-auto-preview by default.

## 2026-09-23 LinkPreview security resolution

Any LinkPreviewProvider is a replaceable optional operator and runs behind a strict SSRF/network sandbox: HTTP(S) allowlist, private/loopback/link-local/metadata/reserved blocking, DNS+redirect revalidation, rebinding resistance, no ambient cookies/auth, and strict redirect/time/byte/decompression/image bounds. Maximum Privacy performs no automatic preview fetch.

## 2026-09-23 expressive inline refinement

`RichTextV1` may contain a bounded `custom_emoji` inline node that references an inert `ExpressiveAssetRef` plus textual fallback. The node cannot contain a remote image URL, HTML, SVG script, CSS, Lottie/TGS program or arbitrary renderer code. Rendering is therefore subject to the same blob-integrity, decoder-budget, accessibility and reduced-motion rules as other expressive assets.

