# GitHub Pages

1. Unzip a release, run `node verify.mjs`, push the **contents of `site/`** to a branch (e.g. `gh-pages`) or `docs/`.
2. Add an empty `.nojekyll` file at the root (the bundle has no Jekyll content; this keeps `_`-prefixed paths intact).
3. Use a user/org site (`user.github.io`) or a custom domain so the app sits at the **origin root**. A project site at
   `user.github.io/repo/` breaks the `/` service-worker scope and manifest `start_url`; do not use it.
4. SPA fallback: Pages serves `404.html` for unknown paths. Copy `index.html` to `404.html` **in the release before
   signing/manifesting** (a post-hoc copy is not in the manifest; the app has no deep links today, so this is optional).

## Headers: honest limits

GitHub Pages **cannot set custom response headers**, so `_headers` is ignored. On this host there is therefore **no enforced**
CSP, no Trusted Types requirement, no Permissions-Policy, no `frame-ancestors` (clickjacking is possible), no COOP/CORP,
and `Referrer-Policy`/`nosniff` are not set. The signed-manifest checks still protect update integrity, but they do not replace CSP.

A `<meta http-equiv="Content-Security-Policy">` fallback is **deliberately not added**: it would edit the signed `index.html`
and duplicate the policy in a second place (`apps/web/csp.ts` is the single source, with tests), and a meta CSP cannot carry
`frame-ancestors`. For real protection put a header-capable proxy in front (Cloudflare, Netlify) or use another host.
Treat GitHub Pages as a mirror/fallback, not a hardened origin.
