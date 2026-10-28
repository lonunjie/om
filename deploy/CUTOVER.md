# Cutover: `om.lonunjie.org` becomes the OM app (beta)

Today `https://om.lonunjie.org` is the static marketing site (`lonunjie/om.lonunjie.org`, Cloudflare Pages project
`om-lonunjie`, git-connected, no build, serves `public/`, push to `main` = production). The app becomes the site
(ADR-0178): the signed web bundle is served at the origin root; with no identity it shows the prerendered landing.

Nothing below touches production until step 6, which is the operator's.

## Serving model (no Pages settings change)

The Pages project keeps serving `public/` from git. For a release, the repo's `public/` is replaced by the verified
`site/` of the signed release folder (bundle, `bundle-manifest.json`, `_headers`). Never edit a file in it: the service
worker rejects any byte that differs from the manifest.

The same repo is public, so it also hosts the "Deploy to Cloudflare" source: `release/` at the repo root holds the
release folder's `site/` plus [`wrangler.toml`](cloudflare-pages/wrangler.toml) (Workers static assets variant), so the button URL is
`https://deploy.workers.cloudflare.com/?url=https://github.com/lonunjie/om.lonunjie.org/tree/main/release`
(button limits: [`cloudflare-pages/README.md`](cloudflare-pages/README.md)).

## Steps

1. Build and sign: `cd ~/dig/lonunjie/om/apps/web && node build/release.mjs --sign-from-vault` (online key
   `ed25519-pkcs8-2026-10-28`; delegation v1 expires 2027-01-26; the PQ co-signature is required). Then
   `node verify.mjs --require-signed` inside the produced folder.
2. Stage on a branch of `om.lonunjie.org` (never `main`): `node deploy/site-stage.mjs <release folder> <website repo>`
   refuses an unverified folder, replaces `public/` with `site/`, and writes `release/` (`site/` + [`wrangler.toml`](cloudflare-pages/wrangler.toml)).
3. Push the branch. Pages builds a preview at `https://<hash>.om-lonunjie.pages.dev` (noindex).
4. Verify on the preview URL: headers (CSP, Trusted Types, Permissions-Policy) equal `apps/web/csp.ts` output; the SW
   registers at scope `/`; manifest and icons load; the PWA installs; first load shows the landing with the beta banner;
   the SW update check passes against the manifest signature; offline reload works; no request leaves the origin except
   the configured Home.
5. Rollback: revert the cutover commit on `main` (Pages redeploys the old `public/`) or roll back to the previous
   deployment in the Pages dashboard. Installed PWAs keep working offline and update only to manifests that verify
   against the pinned roots.
6. **Operator only:** merge the branch to `main` (push = production). A custom domain for the Home
   (`home.om.lonunjie.org`) comes later via `infra/dns.tf` in hq, never the dashboard.

## Checks on the preview

- Exactly one CSP header (the `_headers` file in `public/`).
- Old marketing deep links disappear; unknown paths get the SPA fallback.
- The old site registered no service worker, so nothing stale is cached on returning visitors.
