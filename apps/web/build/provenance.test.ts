import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import { appendEntry, GENESIS, verifyLog } from "./digest-log.mjs";
import { buildProvenance, dssePae, PROVENANCE_FILE, provenanceLine, signProvenance, verifyProvenance } from "./provenance.mjs";
import { normalizeSbom, writeSbom } from "./sbom.mjs";
import { verifyRelease } from "./verify-release.mjs";
import type { BundleManifest } from "../src/update/manifest";
import { delegate, mkOnline, mkRoot, pinOf, signWith } from "./testkit";

const tmp = mkdtempSync(join(tmpdir(), "om-prov-test-"));
afterAll(() => {
  rmSync(tmp, { recursive: true, force: true });
});
const sha = (b: string | Buffer) => createHash("sha256").update(b).digest("hex");

const root = mkRoot();
const online = mkOnline();
const other = mkOnline();
const KEY = "online-1";
const now = Date.now();
const delegation = delegate({ roots: [root], online, keyId: KEY, now });
const pin = pinOf([root]);
const trust = { roots: pin.keys, threshold: 1 };
const COMMIT = "0123456789abcdef0123456789abcdef01234567";

// A release folder shaped like release.mjs output (signed manifest with a throwaway chain), without running vite.
const repoRoot = join(tmp, "repo");
mkdirSync(repoRoot);
writeFileSync(join(repoRoot, "pnpm-lock.yaml"), "lock: 1\n");
writeFileSync(join(repoRoot, "Cargo.lock"), "# cargo\n");
const base = join(tmp, "om-web-1.2.3");
mkdirSync(join(base, "site"), { recursive: true });
writeFileSync(join(base, "site", "index.html"), "<html></html>");
const unsigned = { version: "1.2.3", dev: false, files: [{ path: "index.html", sha256: sha("<html></html>"), bytes: 13 }], signature: null } as unknown as BundleManifest;
writeFileSync(join(base, "site", "bundle-manifest.json"), JSON.stringify(signWith(unsigned, delegation, online, KEY), null, 2) + "\n");
writeFileSync(join(base, "sbom.cdx.json"), "{}\n");
const sums = ["sbom.cdx.json", "site/bundle-manifest.json", "site/index.html"].map((p) => `${sha(readFileSync(join(base, p)))}  ${p}`);
writeFileSync(join(base, "SHA256SUMS"), sums.join("\n") + "\n");
const st = buildProvenance(base, { commit: COMMIT, finishedOn: "2026-10-29T00:00:00Z", repoRoot });
writeFileSync(join(base, PROVENANCE_FILE), provenanceLine(signProvenance(st, online.pem, KEY, delegation, { ...trust, now })));
const clone = (n: string) => { const d = join(tmp, n); cpSync(base, d, { recursive: true }); return d; };

describe("release provenance", () => {
  it("builds an in-toto v1 / SLSA v1 statement with the specified shape", () => {
    expect(st._type).toBe("https://in-toto.io/Statement/v1");
    expect(st.predicateType).toBe("https://slsa.dev/provenance/v1");
    expect(st.subject.map((s) => s.name)).toEqual(["site/bundle-manifest.json", "SHA256SUMS"]);
    expect(st.subject[1]?.digest.sha256).toBe(sha(readFileSync(join(base, "SHA256SUMS"))));
    const bd = st.predicate.buildDefinition;
    expect(bd.buildType).toBe("https://lonunjie.org/om/release/v1");
    expect(bd.externalParameters).toEqual({ repo: "https://github.com/lonunjie/om", commit: COMMIT, version: "1.2.3" });
    expect(bd.resolvedDependencies.find((d) => d.name === "pnpm-lock.yaml")?.digest.sha256).toBe(sha("lock: 1\n"));
    expect(bd.resolvedDependencies.find((d) => d.name === "Cargo.lock")?.digest.sha256).toBe(sha("# cargo\n"));
    expect(st.predicate.runDetails.builder.id).toMatch(/^https:\/\//);
  });
  it("round-trips: DSSE payloadType, standard PAE signature, chain to the pinned root, subjects match", () => {
    const env = JSON.parse(readFileSync(join(base, PROVENANCE_FILE), "utf8")) as { payloadType: string };
    expect(env.payloadType).toBe("application/vnd.in-toto+json");
    expect(dssePae("a", Buffer.from("bc")).toString()).toBe("DSSEv1 1 a 2 bc");
    const r = verifyProvenance(base, { ...trust, now });
    expect(r.errors).toEqual([]);
    expect(r.info.commit).toBe(COMMIT);
    expect(verifyRelease(base, { ...trust, now, requireSigned: true, requireProvenance: true }).errors).toEqual([]);
  });
  it("is optional for releases that predate it, mandatory with requireProvenance", () => {
    const d = clone("no-prov");
    rmSync(join(d, PROVENANCE_FILE));
    expect(verifyRelease(d, { ...trust, now, requireSigned: true }).errors).toEqual([]);
    expect(verifyRelease(d, { ...trust, now, requireProvenance: true }).errors).toEqual(["provenance: missing"]);
  });
  it("fails on a tampered subject digest (SHA256SUMS or manifest changed after signing)", () => {
    const a = clone("t-sums");
    writeFileSync(join(a, "SHA256SUMS"), readFileSync(join(a, "SHA256SUMS"), "utf8") + `${"a".repeat(64)}  site/index.html\n`);
    expect(verifyProvenance(a, { ...trust, now }).errors).toEqual(["subject digest mismatch SHA256SUMS"]);
    const b = clone("t-manifest");
    writeFileSync(join(b, "site/bundle-manifest.json"), readFileSync(join(b, "site/bundle-manifest.json"), "utf8") + " ");
    expect(verifyProvenance(b, { ...trust, now }).errors.join()).toMatch(/subject digest mismatch site\/bundle-manifest.json/);
  });
  it("fails on a tampered payload, a swapped signature, a wrong key and a wrong root", () => {
    const env = JSON.parse(readFileSync(join(base, PROVENANCE_FILE), "utf8")) as Record<string, unknown>;
    const put = (n: string, e: unknown) => { const d = clone(n); writeFileSync(join(d, PROVENANCE_FILE), JSON.stringify(e) + "\n"); return d; };
    const forged = { ...st, subject: [{ name: "SHA256SUMS", digest: { sha256: "0".repeat(64) } }, ...st.subject.slice(0, 1)] };
    const p1 = put("t-payload", { ...env, payload: Buffer.from(JSON.stringify(forged)).toString("base64") });
    expect(verifyProvenance(p1, { ...trust, now }).errors).toEqual(["bad_signature"]);
    const wrong = signProvenance(st, other.pem, KEY, delegate({ roots: [root], online: other, keyId: KEY, now }), { ...trust, now });
    const p2 = put("t-wrongkey", { ...wrong, delegation: env.delegation }); // other key's signature under the manifest's delegation
    expect(verifyProvenance(p2, { ...trust, now }).errors).toEqual(["bad_signature"]);
    expect(verifyProvenance(put("t-deleg", wrong), { ...trust, now }).errors).toEqual(["delegation differs from the one embedded in the manifest"]);
    expect(verifyProvenance(base, { roots: pinOf([mkRoot()]).keys, threshold: 1, now }).errors).toEqual(["delegation: root_threshold"]);
    expect(verifyProvenance(base, { ...trust, now: now + 200 * 86_400_000 }).errors).toEqual(["delegation: delegation_expired"]);
  });
  it("signProvenance refuses without a valid delegation or with a key the delegation does not list", () => {
    expect(() => signProvenance(st, online.pem, KEY, undefined, trust)).toThrow(/delegation missing/);
    expect(() => signProvenance(st, other.pem, KEY, delegation, trust)).toThrow(/does not match the public key/);
    expect(() => signProvenance(st, online.pem, KEY, delegate({ roots: [mkRoot()], online, keyId: KEY, now }), trust)).toThrow(/root_threshold/);
  });
});

describe("release digest log (single-operator)", () => {
  const log = join(tmp, "log", "digests.jsonl");
  const entry = (v: string) => ({ version: v, commit: COMMIT, manifestSha256: sha(`m${v}`), sha256sumsSha256: sha(`s${v}`), time: new Date(now).toISOString() });
  it("chains entries from the genesis hash and verifies", () => {
    const e1 = appendEntry(log, entry("1.0.0"), online.pem, KEY, delegation, { ...trust, now });
    const e2 = appendEntry(log, entry("1.0.1"), online.pem, KEY, delegation, { ...trust, now });
    expect(e1.prev).toBe(GENESIS);
    const lines = readFileSync(log, "utf8").trim().split("\n");
    expect(e2.prev).toBe(sha(lines[0] ?? ""));
    const v = verifyLog(log, { ...trust, now });
    expect(v.errors).toEqual([]);
    expect(v.entries.map((e) => e.version)).toEqual(["1.0.0", "1.0.1"]);
  });
  it("refuses a duplicate version and a signer the delegation does not authorise", () => {
    expect(() => appendEntry(log, entry("1.0.0"), online.pem, KEY, delegation, { ...trust, now })).toThrow(/already logged/);
    expect(() => appendEntry(log, entry("9.9.9"), other.pem, KEY, delegation, { ...trust, now })).toThrow(/does not match the public key/);
    expect(readFileSync(log, "utf8").trim().split("\n")).toHaveLength(2);
  });
  it("fails when a line is modified, removed from the middle, or the wrong root is pinned", () => {
    const lines = readFileSync(log, "utf8").trim().split("\n");
    appendEntry(log, entry("1.0.2"), online.pem, KEY, delegation, { ...trust, now });
    const all = readFileSync(log, "utf8").trim().split("\n");
    const mk = (n: string, ls: string[]) => { const f = join(tmp, "log", n); writeFileSync(f, ls.join("\n") + "\n"); return f; };
    const edited = { ...(JSON.parse(lines[0] ?? "") as Record<string, unknown>), commit: "f".repeat(40) };
    expect(verifyLog(mk("e1.jsonl", [JSON.stringify(edited), ...all.slice(1)]), { ...trust, now }).errors.join()).toMatch(/line 1: bad_signature.*line 2: prev does not match/);
    expect(verifyLog(mk("e2.jsonl", [all[0] ?? "", all[2] ?? ""]), { ...trust, now }).errors.join()).toMatch(/line 2: prev does not match/);
    expect(verifyLog(log, { roots: pinOf([mkRoot()]).keys, threshold: 1, now }).errors.join()).toMatch(/root_threshold/);
    expect(existsSync(join(tmp, "log", "delegations"))).toBe(true);
  });
});

describe("SBOM", () => {
  it("normalizeSbom drops the per-run serialNumber and timestamp", () => {
    expect(normalizeSbom({ bomFormat: "CycloneDX", serialNumber: "urn:uuid:x", metadata: { timestamp: "t", component: { name: "a" } } })).toEqual({ bomFormat: "CycloneDX", metadata: { component: { name: "a" } } });
  });
  it("writes a deterministic CycloneDX document for the web workspace", { timeout: 60_000 }, () => {
    const a = join(tmp, "a.cdx.json"), b = join(tmp, "b.cdx.json");
    expect(writeSbom(a)).toBeGreaterThan(0);
    writeSbom(b);
    expect(readFileSync(a, "utf8")).toBe(readFileSync(b, "utf8"));
    expect((JSON.parse(readFileSync(a, "utf8")) as { bomFormat: string }).bomFormat).toBe("CycloneDX");
  });
});

describe("rebuild compare", () => {
  it("matches an identical site and reports a differing, missing or extra file", async () => {
    const { compareRebuild } = await import("./compare-rebuild.mjs");
    expect(compareRebuild(base, join(base, "site"))).toEqual({ ok: true, errors: [] });
    const d = clone("rb-changed");
    writeFileSync(join(d, "site", "index.html"), "<html>x</html>");
    writeFileSync(join(d, "site", "bundle-manifest.json"), JSON.stringify({ version: "1.2.3", files: [{ path: "index.html", sha256: sha("<html>x</html>"), bytes: 14 }, { path: "extra.js", sha256: sha("e"), bytes: 1 }] }));
    const r = compareRebuild(base, d);
    expect(r.ok).toBe(false);
    expect(r.errors).toEqual(expect.arrayContaining(["differs: index.html", "extra in rebuild: extra.js"]));
  });
});
