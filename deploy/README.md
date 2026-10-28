# OM web deploy kit

Decentralized deployment of the static OM web bundle (`apps/web`), per
[ADR-0178](../decisions/ADR-0178-pwa-canonical-store-independent-distribution.md): the PWA installs and updates from
**any** conforming origin; no origin is hard-coded in app logic and the bundle never loads app code from a remote origin.

## Release artifact

```sh
pnpm install --frozen-lockfile
cd apps/web && node build/release.mjs --dev            # unsigned DEV release (testing only)
cd apps/web && node build/release.mjs --signature sig.json   # production: needs a real signature
```

Output (git-ignored) in `apps/web/release/`: `om-web-<version>[-dev]/` and `.zip` + `.zip.sha256`. The folder holds
`site/` (the bundle, `bundle-manifest.json`, `_headers`), `SHA256SUMS`, `verify.mjs`, the wrangler configs and a README.

- **Signing (audit B2):** the client pins only an **offline root key**; the online release key signs under a root-signed,
  expiring, revocable *delegation* embedded in every manifest. The root has not been generated yet (placeholder), so
  production releases currently fail by design. Runbook: [`KEY-CEREMONY.md`](KEY-CEREMONY.md). Without `--dev`, the
  script refuses an unsigned release and says so. `--dev` builds with `OM_BUNDLE_DEV=1`; the manifest is marked `dev`
  and unsigned, and production clients reject it. `--sign-from-vault` signs with the delegated online key (read from the hidden custom field `secret` of the agents' vault item) and needs
  [`apps/web/release-delegation.json`](../apps/web/release-delegation.json) (or `--delegation <file>`). `--signature` injects an externally produced
  `{alg,keyId,sig}` over the manifest signing payload (`signingPayload()` in `apps/web/src/update/manifest.ts`);
  the delegation is still embedded and the whole chain is verified before the release is reported as signed.
- **Verify (no deps, no build):** `node verify.mjs [dir] [--require-signed]` checks `SHA256SUMS`, every manifest hash and size,
  and that `site/` has no unlisted files. With a signature present it verifies the chain pinned root key, delegation, release key, manifest signature
  (fails closed while the root is unset).
  Also check the zip: `shasum -a 256 -c om-web-X.zip.sha256`.
- Never edit files in `site/` (including `index.html`): the service worker rejects any byte that differs from the manifest.

## Targets

| Host | Guide | Headers (CSP etc.) |
|---|---|---|
| Cloudflare Pages / Workers assets | [cloudflare-pages/](cloudflare-pages/README.md) | yes, via `_headers` |
| GitHub Pages | [github-pages/](github-pages/README.md) | **no** (see limits) |
| nginx, Caddy, S3/CloudFront, Netlify, any static host | [generic-static/](generic-static/README.md) | host-specific |

Common requirements: serve at the **origin root** (SW scope and manifest scope are `/`), HTTPS, and the headers from
`apps/web/csp.ts` (the single source; `site/_headers` is generated from it).
