// Single source of truth for the production Content-Security-Policy and companion headers.
// Used by vite.config.ts (preview + the generated Cloudflare Pages `_headers`) and the header test.
// connect-src allows https:/wss: because relay, Home and blob endpoints are user-chosen providers (never hard-coded);
// everything that can execute code or load subresources is limited to 'self'.
export const CSP: Readonly<Record<string, string>> = {
  "default-src": "'none'",
  "script-src": "'self' 'wasm-unsafe-eval'",
  "style-src": "'self'",
  "img-src": "'self' blob: data:",
  "media-src": "'self' blob:",
  "font-src": "'self'",
  "connect-src": "'self' https: wss:",
  "worker-src": "'self'",
  "manifest-src": "'self'",
  "object-src": "'none'",
  "base-uri": "'none'",
  "form-action": "'none'",
  "frame-ancestors": "'none'",
  "require-trusted-types-for": "'script'",
  "trusted-types": "om-bundle",
};

export const cspHeader = (): string =>
  Object.entries(CSP)
    .map(([k, v]) => `${k} ${v}`)
    .join("; ");

export const SECURITY_HEADERS: Readonly<Record<string, string>> = {
  "Content-Security-Policy": cspHeader(),
  "X-Content-Type-Options": "nosniff",
  "Referrer-Policy": "no-referrer",
  "Cross-Origin-Opener-Policy": "same-origin",
  "Cross-Origin-Resource-Policy": "same-origin",
  "Permissions-Policy": "geolocation=(), microphone=(), camera=(self), payment=(), usb=()",
};

/** Cloudflare Pages `_headers` file body. */
export const headersFile = (): string =>
  "/*\n" + Object.entries(SECURITY_HEADERS).map(([k, v]) => `  ${k}: ${v}`).join("\n") + "\n";
