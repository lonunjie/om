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
  and unsigned, and production clients reject it. `--sign-from-vault` signs with the delegated online key (read from the hidden custom field `ed25519-pkcs8-2026-10-28` of the agents' vault item) and needs
  [`apps/web/release-delegation.json`](../apps/web/release-delegation.json) (or `--delegation <file>`). `--signature` injects an externally produced
  `{alg,keyId,sig}` over the manifest signing payload (`signingPayload()` in `apps/web/src/update/manifest.ts`);
  the delegation is still embedded and the whole chain is verified before the release is reported as signed.
- **Verify (no deps, no build):** `node verify.mjs [dir] [--require-signed]` checks `SHA256SUMS`, every manifest hash and size,
  and that `site/` has no unlisted files. With a signature present it verifies the chain pinned root key, delegation, release key, manifest signature
  (fails closed while the root is unset).
  Also check the zip: `shasum -a 256 -c om-web-X.zip.sha256`.
- **Provenance, SBOM, digest log:** a signed release also holds `provenance.intoto.jsonl` (an in-toto Statement v1 with a SLSA v1 provenance predicate in a DSSE
  envelope, signed by the same delegated key and carrying the manifest's delegation; subjects are `site/bundle-manifest.json` and `SHA256SUMS`, so it is the
  one file not listed in `SHA256SUMS`) and `sbom.cdx.json` (CycloneDX, production dependencies, listed in `SHA256SUMS`). `verify.mjs` checks the provenance when
  present; `--require-provenance` makes it mandatory (it is not implied by `--require-signed`, so releases made before it existed still verify). The provenance is
  self-attested by the operator's own machine. [`release-log/`](release-log/README.md) is a signed, hash-chained **single-operator** digest log, not a transparency log.
  How a third party rebuilds and compares: [`REBUILD-AND-COMPARE.md`](REBUILD-AND-COMPARE.md).
- Never edit files in `site/` (including `index.html`): the service worker rejects any byte that differs from the manifest.

## Targets

| Host | Guide | Headers (CSP etc.) |
|---|---|---|
| Cloudflare Pages / Workers assets | [cloudflare-pages/](cloudflare-pages/README.md) | yes, via `_headers` |
| GitHub Pages | [github-pages/](github-pages/README.md) | **no** (see limits) |
| nginx, Caddy, S3/CloudFront, Netlify, any static host | [generic-static/](generic-static/README.md) | host-specific |

Common requirements: serve at the **origin root** (SW scope and manifest scope are `/`), HTTPS, and the headers from
`apps/web/csp.ts` (the single source; `site/_headers` is generated from it).

## Copies link back to the official site

A copy built from this kit shows a small line, "This is a self-hosted copy of OM. The official site is om.lonunjie.org.", on the landing page and in Settings.
It is decided in the browser by comparing the page's origin with the pinned official one (not by the signed manifest or any build variable), so a copy cannot be mistaken for the official site; it is hidden on om.lonunjie.org, its Pages previews and localhost.

## Custody statement

The offline root that authorises every release key is **2-of-3 Ed25519** (two Ledger hardware wallets and one software key) plus a **required
ML-DSA-65 post-quantum co-signature**, and all of it is held by **one custodian**. Against coercion of that person this is equal to a single key. There is
no independent monitoring, no independent mirror and no transparency log for these releases; the digest log is a single-operator file. The bundle is
reproducible from source ([`REBUILD-AND-COMPARE.md`](REBUILD-AND-COMPARE.md)), which lets anyone check the content, not who controls the keys.
