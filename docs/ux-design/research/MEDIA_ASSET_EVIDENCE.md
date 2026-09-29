# Media Asset Evidence Notes — Run 11B

## QR quiet zone

DENSO WAVE's QR guidance specifies a four-module-wide clear margin around all sides of the QR symbol. QR Code is standardized as ISO/IEC 18004. OM therefore keeps the default QR itself visually conservative and moves branding outside the symbol.

References:
- DENSO WAVE, QR Code "Securing Margin": https://www.qrcode.com/en/howto/code.html
- DENSO WAVE, QR Code standardization: https://www.qrcode.com/en/about/standards.html/index.html

## SVG is not inherently inert

SVG can include script and external resources depending on embedding/processing mode. W3C's SVG Integration/SVG 2 security model distinguishes modes in which scripting and external references are disabled. OM asset packs therefore treat third-party SVG as untrusted markup and require a static sanitized subset rather than assuming an `.svg` extension is safe.

References:
- W3C SVG Integration: https://www.w3.org/TR/svg-integration/
- W3C SVG 2: https://www.w3.org/TR/SVG/
- W3C SVG Security notes: https://www.w3.org/wiki/SVG_Security

## Interpretation

These references justify constraints, not a claim that OM's exact card geometry, avatar palette, or manifest shape is scientifically optimal. Those are product design candidates to be tested.
