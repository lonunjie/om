# Generic static hosting

Upload `site/` from a verified release (`node verify.mjs --require-signed`) to the **root** of an HTTPS origin. Required behaviour:

- Send the headers in `site/_headers` on every response (`/*`). Values come from `apps/web/csp.ts`; copy them from the file, do not retype:
  `Content-Security-Policy` (incl. `require-trusted-types-for 'script'; trusted-types om-bundle`), `Permissions-Policy`,
  `X-Content-Type-Options`, `Referrer-Policy`, `Cross-Origin-Opener-Policy`, `Cross-Origin-Resource-Policy`.
- `/sw.js` must be served from the root so its scope is `/`, with `Cache-Control: no-cache` (the app registers it with `updateViaCache: "none"`).
- SPA fallback: unknown paths return `index.html` with status 200.
- Do not rewrite, minify or inject into any file (CDN "optimizers", analytics snippets); any change fails manifest verification.
- Keep `_headers` out of public reach if the host would serve it as a file (it is not in the manifest).

nginx:

```nginx
location / { try_files $uri /index.html; }
location = /sw.js { add_header Cache-Control "no-cache"; include om-security-headers.conf; }
# om-security-headers.conf: one `add_header NAME "VALUE" always;` per line of site/_headers
```

Caddy: `header { Content-Security-Policy "..." ... }` plus `try_files {path} /index.html`.
Netlify: copy `_headers` as-is and add `/* /index.html 200` to `_redirects`.
S3/CloudFront: response-headers policy with the same headers; custom error 403/404 -> `/index.html` 200.
