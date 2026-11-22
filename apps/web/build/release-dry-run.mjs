#!/usr/bin/env node
// SPDX-License-Identifier: MPL-2.0
// Signed-release dry run: builds a release (the web bundle, or with `relay` the per-user Home template) signed with a THROWAWAY key
// under a THROWAWAY root and delegation, generated in memory for this run, and checks the result end to end. Nothing here can touch a
// release key: the vault is never read, the pinned roots are not used for signing, the output goes to a temporary folder, no digest log
// is appended and nothing is uploaded. It proves the mechanics (reproducible build, manifest, signature chain, provenance, SBOM,
// SHA256SUMS) and that a dry-run release is REFUSED by the pinned production roots, so it can never be mistaken for a real one.
// Usage: node build/release-dry-run.mjs [web|relay] [--keep]   (--keep leaves the temporary folder, including the throwaway key, for inspection)
import { spawnSync } from "node:child_process";
import { createPrivateKey, generateKeyPairSync } from "node:crypto";
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { addRootSignature, buildDelegation, describeKey } from "./root-ceremony.mjs";
import { verifyRelease } from "./verify-release.mjs";

const webRoot = fileURLToPath(new URL("..", import.meta.url));
const repoRoot = join(webRoot, "../..");
const args = process.argv.slice(2);
const target = args.find((a) => !a.startsWith("--")) ?? "web";
const keep = args.includes("--keep");
if (!["web", "relay"].includes(target)) { console.error("usage: release-dry-run.mjs [web|relay] [--keep]"); process.exit(2); }
const fail = (m) => { console.error(`dry run: ${m}`); process.exit(1); };

const pair = () => {
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");
  return { pem: privateKey.export({ format: "pem", type: "pkcs8" }), pub: publicKey.export({ format: "der", type: "spki" }).subarray(-32).toString("hex") };
};
const tmp = mkdtempSync(join(tmpdir(), "om-release-dry-run-"));
const ONLINE_ID = "dry-run-online";
const size = (d) => readdirSync(d, { withFileTypes: true }).reduce((n, e) => n + (e.isDirectory() ? size(join(d, e.name)) : statSync(join(d, e.name)).size), 0);
try {
  const rootPair = pair();
  const root = { pem: rootPair.pem, key: createPrivateKey(rootPair.pem), ...describeKey(rootPair.pub), frame: "raw" };
  const online = pair();
  const now = Date.now();
  const delegation = addRootSignature(buildDelegation({
    version: 1, now, expires: new Date(now + 30 * 86_400_000).toISOString().replace(/\.\d{3}Z$/, "Z"),
    addKey: { keyId: ONLINE_ID, pub: online.pub, notBefore: new Date(now - 86_400_000).toISOString().replace(/\.\d{3}Z$/, "Z") },
  }), root);
  const trust = { roots: { [root.id]: root.pub }, threshold: 1 };
  const keyFile = join(tmp, "online.pem");
  const rootsFile = join(tmp, "test-roots.json");
  const delegationFile = join(tmp, "delegation.json");
  writeFileSync(keyFile, online.pem, { mode: 0o600 });
  writeFileSync(rootsFile, JSON.stringify(trust));
  writeFileSync(delegationFile, JSON.stringify(delegation));
  const out = join(tmp, "out");
  const common = ["--sign-with-key-file", keyFile, "--key-id", ONLINE_ID, "--test-roots-file", rootsFile, "--delegation", delegationFile, "--out", out];

  if (target === "web") {
    const r = spawnSync("node", ["build/release.mjs", "--no-zip", ...common], { cwd: webRoot, stdio: "inherit" });
    if (r.status !== 0) fail("the web release build failed");
    const name = readdirSync(out)[0];
    const dir = join(out, name);
    const v = verifyRelease(dir, { requireSigned: true, requireProvenance: true, roots: trust.roots, threshold: 1, pq: null });
    if (!v.ok) fail(`the throwaway-signed release does not verify under the throwaway root:\n  ${v.errors.join("\n  ")}`);
    const refused = verifyRelease(dir, { requireSigned: true });
    if (refused.ok) fail("the pinned production roots ACCEPTED a throwaway-signed release: stop and report this");
    for (const f of ["SHA256SUMS", "sbom.cdx.json", "provenance.intoto.jsonl", "site/bundle-manifest.json"]) if (!existsSync(join(dir, f))) fail(`missing ${f}`);
    const m = JSON.parse(readFileSync(join(dir, "site/bundle-manifest.json"), "utf8"));
    console.log(`dry run OK (web): ${name}, version ${m.version}, ${m.files?.length ?? "?"} files, ${size(join(dir, "site"))} bytes in site/; verifies under the throwaway root; refused by the pinned production roots (${refused.errors.length} error(s), first: ${refused.errors[0] ?? "none"})`);
  } else {
    const r = spawnSync("node", ["infra/cloudflare-home/release/build.mjs", "--allow-dirty", ...common], { cwd: repoRoot, stdio: "inherit" });
    if (r.status !== 0) fail("the relay release build failed");
    const { verifyRelay } = await import(join(repoRoot, "infra/cloudflare-home/release/verify-relay.mjs"));
    const name = readdirSync(out)[0];
    const dir = join(out, name);
    const v = verifyRelay(dir, { roots: trust.roots, threshold: 1, pq: null });
    if (!v.ok) fail(`the throwaway-signed relay release does not verify under the throwaway root:\n  ${v.errors.join("\n  ")}`);
    const refused = verifyRelay(dir, {});
    if (refused.ok) fail("the pinned production roots ACCEPTED a throwaway-signed relay release: stop and report this");
    console.log(`dry run OK (relay): ${name}, worker ${statSync(join(dir, "relay/worker.js")).size} bytes; verifies under the throwaway root; refused by the pinned production roots (first: ${refused.errors[0] ?? "none"})`);
  }
  if (keep) console.log(`kept: ${tmp} (contains a throwaway key; delete it when done)`);
} finally {
  if (!keep) rmSync(tmp, { recursive: true, force: true });
}
