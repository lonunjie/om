# Cloudflare Pages / Workers assets

Use your own Cloudflare account. Nothing here touches the reference deployment.

## Direct upload (recommended; no build, no git)

```sh
unzip om-web-X.zip && cd om-web-X
node verify.mjs --require-signed
npx wrangler pages deploy ./site --project-name <your-project>   # reads ./wrangler.toml
```

Pages applies `site/_headers` (CSP, Trusted Types, Permissions-Policy, ...). Pages serves `index.html` for unknown paths
when there is no root-level 404, which gives the SPA fallback.

## "Deploy to Cloudflare" button

Verified against <https://developers.cloudflare.com/workers/platform/deploy-buttons/> (2026-10-28):

```md
[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)](https://deploy.workers.cloudflare.com/?url=<PUBLIC_GIT_REPO_URL>)
```

`<PUBLIC_GIT_REPO_URL>` may be a `.../tree/<branch>/<subdir>` URL. Limits that shape the design:

- The button supports **Workers applications only, not Pages**, so it uses Workers static assets
  ([`wrangler.workers.toml`](wrangler.workers.toml), renamed to [`wrangler.toml`](wrangler.toml)).
- The repo must be public and the target subdirectory **self-contained**: it becomes the root of the new repo created in the
  user's account. So the button cannot point at `deploy/` in this repo (no bundle is committed there, and a private repo cannot be used).
  Publish an unzipped, verified release folder (`site/` + [`wrangler.toml`](wrangler.toml)) as its own public repo or branch, then:

```
https://deploy.workers.cloudflare.com/?url=https://github.com/lonunjie/om/tree/<release-branch>/<release-folder>
```

  Replace the repo path with wherever the release folder is actually published. Until a signed release is published, no working button URL exists.
- The deployed copy runs on a `*.workers.dev` or custom domain of the user's choosing; nothing in the bundle names an origin.
