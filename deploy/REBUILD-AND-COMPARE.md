# Rebuild and compare (for a third party)

Goal: check that a published signed web release contains the bytes that the public source at one commit produces. This proves the **content**
is reproducible from source; it does not prove the source is good, and **signatures cannot be rebuilt**: the manifest's `delegation` and
`signature` fields, `SHA256SUMS`, `provenance.intoto.jsonl` and `sbom.cdx.json` are made by the custodian and are only checked, never reproduced.
You need Node 24, pnpm 12.11.2 and the Rust toolchain from [`rust-toolchain.toml`](../rust-toolchain.toml) (for the Core wasm).

1. Get the published release (folder or zip) and verify it as published. This checks the signature chain against the root pinned in `verify.mjs`:
   ```sh
   cd om-web-X.Y.Z && node verify.mjs --require-signed --require-provenance
   ```
2. Read the commit the custodian says it was built from: it is `externalParameters.commit` inside `provenance.intoto.jsonl` (base64 `payload`),
   and the same commit appears in `release-log/digests.jsonl` (see [`release-log/README.md`](release-log/README.md); single-operator, not independent).
   ```sh
   node -e 'const e=JSON.parse(require("fs").readFileSync("provenance.intoto.jsonl","utf8"));console.log(JSON.parse(Buffer.from(e.payload,"base64")).predicate.buildDefinition.externalParameters)'
   ```
3. Check out that commit and install exactly the locked dependencies:
   ```sh
   git clone https://github.com/lonunjie/om && cd om && git checkout <commit>
   pnpm install --frozen-lockfile
   ```
   The provenance statement lists the sha256 of `pnpm-lock.yaml` and `Cargo.lock` at that commit; compare them with `shasum -a 256`.
4. Check that two clean builds agree on your machine, then build the release content unsigned, with a clean environment as [`release.mjs`](../apps/web/build/release.mjs) does:
   ```sh
   pnpm --filter @om/web repro
   pnpm --filter @om/core-wasm build:wasm
   cd apps/web && env -i PATH="$PATH" HOME="$HOME" OM_RELEASE_BUILD=1 NODE_ENV=production \
     pnpm exec vite build --outDir /tmp/om-rebuild/site --emptyOutDir
   ```
5. Compare every file the published signed manifest lists with your rebuild (path, size and sha256; also the rebuilt bytes):
   ```sh
   node build/compare-rebuild.mjs /path/to/om-web-X.Y.Z /tmp/om-rebuild/site
   ```
   `MATCH` means each published file was reproduced byte for byte. Your rebuilt `bundle-manifest.json` differs from the published one only in
   `delegation` and `signature`; that is expected.
6. Optional: `pnpm --filter @om/web sbom --sbom-format cyclonedx --prod --lockfile-only` lists the same production dependencies as the published `sbom.cdx.json`
   (the published one has the random serial number and timestamp removed and components sorted).

If step 5 reports a difference, report it with the commit, your OS, Node, pnpm and Rust versions: a toolchain mismatch (most likely the wasm) is
possible and is not by itself evidence of tampering, but it is also not a pass.
