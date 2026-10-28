import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import { PINNED_ROOTS, PINNED_ROOT_THRESHOLD, verifyRelease } from "./verify-release.mjs";
import { extractPem, requireDelegation, signManifest } from "./sign.mjs";
import { cleanBuildEnv, pollutedKeys } from "./clean-env.mjs";
import { siteProblems } from "./release-guards.mjs";
import { ROOT_KEYS, ROOT_KEY_COUNT, ROOT_THRESHOLD, isRootPinned, makeTrustRoot } from "../src/update/release-key";
import { verifyBundleSignature } from "../src/update/signature";
import { signingPayload, type BundleManifest } from "../src/update/manifest";
import { delegate, mkOnline, mkRoot, pinOf } from "./testkit";

const webRoot = new URL("..", import.meta.url).pathname;
const tmp = mkdtempSync(join(tmpdir(), "om-release-test-"));
afterAll(() => {
  rmSync(tmp, { recursive: true, force: true });
});
const run = (...a: string[]) => spawnSync("node", ["build/release.mjs", ...a], { cwd: webRoot, encoding: "utf8" });
const repoRoot = new URL("../../..", import.meta.url).pathname;

describe("production build hygiene (audit H1)", () => {
  const polluters = ["VITE_OM_DEV_HARNESS", "VITE_OM_E2E_CORE", "OM_BUNDLE_DEV", "VITE_OM_ANYTHING"];

  it("allowlists the build environment and never forwards bundle-selecting variables", () => {
    const env = { PATH: "/bin", HOME: "/h", LANG: "C", VITE_OM_E2E_CORE: "1", OM_BUNDLE_DEV: "1", VITE_X: "1", SECRET_TOKEN: "s", NODE_ENV: "development" };
    expect(pollutedKeys(env)).toEqual(["OM_BUNDLE_DEV", "VITE_OM_E2E_CORE", "VITE_X"]);
    expect(cleanBuildEnv(env)).toEqual({ PATH: "/bin", HOME: "/h", LANG: "C", NODE_ENV: "production" });
    expect(cleanBuildEnv(env, { dev: true }).OM_BUNDLE_DEV).toBe("1");
    expect(Object.keys(cleanBuildEnv(env))).not.toContain("SECRET_TOKEN");
  });

  it.each(polluters)("release.mjs fails when %s is set in the environment (before building anything)", (name) => {
    const r = spawnSync("node", ["build/release.mjs", "--dev", "--no-zip", "--out", tmp], {
      cwd: webRoot, encoding: "utf8", env: { ...process.env, [name]: "1" },
    });
    expect(r.status).toBe(1);
    expect(r.stderr).toMatch(new RegExp(`refusing to build.*${name}`, "s"));
    expect(existsSync(join(tmp, "om-web-0.0.0-dev"))).toBe(false);
  });

  it("vite itself refuses a production-mode build with a test flag unless the dev-bundle switch is set", { timeout: 60_000 }, () => {
    for (const flag of ["VITE_OM_DEV_HARNESS", "VITE_OM_E2E_CORE"]) {
      const env: Record<string, string | undefined> = { ...process.env, [flag]: "1" };
      delete env["OM_BUNDLE_DEV"];
      const r = spawnSync("pnpm", ["exec", "vite", "build", "--outDir", join(tmp, `refused-${flag}`)], { cwd: webRoot, encoding: "utf8", env });
      expect(r.status, r.stdout).not.toBe(0);
      expect(r.stdout + r.stderr).toMatch(/R-072/);
      expect(existsSync(join(tmp, `refused-${flag}`))).toBe(false);
    }
  });

  describe("pre-signing site checks", () => {
    const site = (files: Record<string, string>) => {
      const d = mkdtempSync(join(tmp, "site-"));
      for (const [n, body] of Object.entries(files)) { mkdirSync(join(d, n, ".."), { recursive: true }); writeFileSync(join(d, n), body); }
      return d;
    };
    const env = { ...process.env } as Record<string, string | undefined>;
    it("passes a site with a clean Core wasm", () => {
      expect(siteProblems(site({ "index.html": "<html></html>", "assets/om_wasm-1.wasm": "\0asm om_host random_fill" }), repoRoot, env)).toEqual([]);
    });
    it.each(["InsecureTestRng", "OM-TEST-LOOPBACK-NOT-FOR-PRODUCTION", "debug_inject", "LoopbackNetwork"])("flags a Core wasm carrying %s", (marker) => {
      const p = siteProblems(site({ "assets/om_wasm-1.wasm": `\0asm ${marker}` }), repoRoot, env);
      expect(p.join("\n")).toContain(marker);
    });
    it("flags a harness bundle and a site without the Core", () => {
      expect(siteProblems(site({ "assets/a.js": "OM-DEV-HARNESS-NOT-FOR-PRODUCTION", "assets/om_wasm-1.wasm": "\0asm" }), repoRoot, env).join()).toMatch(/harness/);
      expect(siteProblems(site({ "index.html": "x" }), repoRoot, env).join()).toMatch(/Core is missing/);
    });
  });
});

describe("release script", () => {
  it("refuses an unsigned production release and says why", () => {
    const r = run("--out", tmp);
    expect(r.status).toBe(1);
    expect(r.stderr).toMatch(/UNSIGNED/);
    expect(r.stderr).toMatch(/--dev/);
  });
  it("builds a verifiable dev release; tampering and --require-signed fail", { timeout: 120_000 }, () => {
    const r = run("--dev", "--no-zip", "--out", tmp);
    expect(r.status, r.stderr).toBe(0);
    const dir = join(tmp, "om-web-0.0.0-dev");
    expect(existsSync(join(dir, "SHA256SUMS"))).toBe(true);
    expect(verifyRelease(dir).ok).toBe(true);
    expect(verifyRelease(dir, { requireSigned: true }).ok).toBe(false);
    const cli = spawnSync("node", [join(dir, "verify.mjs"), dir], { encoding: "utf8" });
    expect(cli.status).toBe(0);
    expect(cli.stdout).toMatch(/DEV build/);
    const bad = join(tmp, "tampered");
    cpSync(dir, bad, { recursive: true });
    writeFileSync(join(bad, "site", "robots.txt"), "tampered");
    const v = verifyRelease(bad);
    expect(v.ok).toBe(false);
    expect(v.errors.join("\n")).toMatch(/mismatch/);
  });

  it("the root policy is 2-of-3 and, once pinned, exactly three distinct root keys with distinct public keys", () => {
    expect(ROOT_THRESHOLD).toBe(2);
    expect(ROOT_KEY_COUNT).toBe(3);
    if (isRootPinned()) {
      const ids = Object.keys(ROOT_KEYS);
      expect(ids).toHaveLength(ROOT_KEY_COUNT);
      expect(new Set(Object.values(ROOT_KEYS).map((v) => (typeof v === "string" ? v : v.pub))).size).toBe(ROOT_KEY_COUNT);
    }
  });

  it("pins the same offline root in the client trust root and verify.mjs (currently UNSET: ceremony pending)", () => {
    expect(PINNED_ROOTS).toEqual(ROOT_KEYS);
    expect(PINNED_ROOT_THRESHOLD).toBe(ROOT_THRESHOLD);
    expect(makeTrustRoot(false).roots).toMatchObject({ keys: ROOT_KEYS, threshold: ROOT_THRESHOLD }); // plus the PQ pin (ADR-0182), checked in pq-cosign.test.ts
    expect(makeTrustRoot(false).allowUnsignedDev).toBe(false);
    // No online release key is pinned anywhere in the client any more: only roots.
    expect(Object.keys(makeTrustRoot(false))).not.toContain("verifiers");
  });
  it.skipIf(isRootPinned())("while the root is unset, a production release is refused up front (dev builds are unaffected)", () => {
    for (const a of [["--sign-from-vault"], ["--signature", join(tmp, "x.json")]]) {
      const r = run("--no-zip", "--out", join(tmp, "unset-root"), ...a);
      expect(r.status).toBe(1);
      expect(r.stderr).toMatch(/no offline ROOT key is pinned/);
      expect(existsSync(join(tmp, "unset-root"))).toBe(false);
    }
  });
  it.skipIf(isRootPinned())("while the root is unset, a vite release build (OM_RELEASE_BUILD) fails but a plain production build config loads", { timeout: 60_000 }, () => {
    const env: Record<string, string | undefined> = { ...process.env, OM_RELEASE_BUILD: "1" };
    delete env["OM_BUNDLE_DEV"];
    const r = spawnSync("pnpm", ["exec", "vite", "build", "--outDir", join(tmp, "refused-release")], { cwd: webRoot, encoding: "utf8", env });
    expect(r.status, r.stdout).not.toBe(0);
    expect(r.stdout + r.stderr).toMatch(/offline ROOT public key is not pinned/);
    expect(existsSync(join(tmp, "refused-release"))).toBe(false);
  });
  it("extracts only the PEM block from a vault note", () => {
    expect(extractPem("note\n-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\ntail")).toMatch(/^-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----$/);
    expect(extractPem("nothing")).toBeNull();
  });

  describe("signed release (throwaway root + online key)", () => {
    const root = mkRoot();
    const online = mkOnline();
    const ONLINE_ID = "online-1";
    const real = Date.now(); // release.mjs and verify.mjs read the real clock
    const delegation = delegate({ roots: [root], online, keyId: ONLINE_ID, now: real });
    const trust = { roots: pinOf([root]).keys, threshold: 1 };
    const keyFile = join(tmp, "online.pem");
    writeFileSync(keyFile, online.pem);
    const rootsFile = join(tmp, "test-roots.json");
    writeFileSync(rootsFile, JSON.stringify({ roots: trust.roots, threshold: 1 }));
    const delegationFile = join(tmp, "delegation.json");
    writeFileSync(delegationFile, JSON.stringify(delegation));
    const dir = join(tmp, "om-web-0.0.0");
    const clone = (name: string) => { const d = join(tmp, name); cpSync(dir, d, { recursive: true }); return d; };
    const manifestOf = (d: string) => JSON.parse(readFileSync(join(d, "site/bundle-manifest.json"), "utf8")) as BundleManifest;
    const rewrite = (d: string, mut: (m: Record<string, unknown>) => void) => {
      const p = join(d, "site/bundle-manifest.json");
      const m = JSON.parse(readFileSync(p, "utf8")) as Record<string, unknown>;
      mut(m);
      writeFileSync(p, JSON.stringify(m, null, 2) + "\n");
    };
    const sigErr = (d: string, extra: Record<string, unknown> = {}) =>
      verifyRelease(d, { ...trust, requireSigned: true, ...extra }).errors.filter((e) => e.startsWith("signature:"));
    const signArgs = ["--no-zip", "--out", tmp, "--sign-with-key-file", keyFile, "--key-id", ONLINE_ID, "--test-roots-file", rootsFile];

    it("refuses to sign without a delegation, and says where to get one", { timeout: 120_000 }, () => {
      const r = run(...signArgs, "--delegation", join(tmp, "nope.json"));
      expect(r.status).toBe(1);
      expect(r.stderr).toMatch(/delegation missing or unreadable/);
      expect(existsSync(join(tmp, "om-web-0.0.0"))).toBe(false);
    });
    it("sign.mjs refuses a missing, expired, wrongly-listed, revoked or mismatched-key delegation (no output)", () => {
      const m = { version: "1.0.0", delegation: undefined as unknown };
      const t = (d: unknown, id = ONLINE_ID, pem = online.pem, now = real) => { m.delegation = d; return () => signManifest(m, pem, id, { ...trust, now }); };
      expect(t(undefined)).toThrow(/delegation missing/);
      expect(t(delegation)).not.toThrow();
      expect(t(delegation, ONLINE_ID, online.pem, real + 200 * 86_400_000)).toThrow(/delegation refused: delegation_expired/);
      expect(t(delegation, "other-id")).toThrow(/unlisted_key/);
      expect(t(delegation, ONLINE_ID, mkOnline().pem)).toThrow(/does not match the public key/);
      expect(t(delegate({ roots: [root], online, keyId: ONLINE_ID, now: real, mutate: (d) => ({ ...d, revoked: [ONLINE_ID] }) }))).toThrow(/revoked_key/);
      expect(t(delegate({ roots: [mkRoot()], online, keyId: ONLINE_ID, now: real }))).toThrow(/root_threshold/);
      expect(() => requireDelegation(delegation, online.pem, ONLINE_ID, { roots: {}, threshold: 1, now: real })).toThrow(/root_unset/);
      expect(() => signManifest({ delegation }, online.pem, ONLINE_ID, { roots: {}, threshold: 1 })).toThrow(/root_unset/);
    });
    it("builds, verifies with the right root, and matches the TS verifier byte-for-byte", { timeout: 120_000 }, async () => {
      const r = run(...signArgs, "--delegation", delegationFile);
      expect(r.status, r.stderr).toBe(0);
      expect(r.stdout + r.stderr).not.toMatch(/PRIVATE KEY/);
      const v = verifyRelease(dir, { ...trust, requireSigned: true });
      expect(v.errors).toEqual([]);
      expect(v.info.signatureValid).toBe(true);
      expect(v.info.delegationVersion).toBe(1);
      const m = manifestOf(dir);
      expect(m.delegation).toEqual(delegation);
      expect(await verifyBundleSignature(m, { roots: pinOf([root]), now: () => real })).toEqual({ ok: true, unsignedDev: false, delegationVersion: 1 });
      expect(Buffer.from(signingPayload(m)).equals(Buffer.from(signingPayload({ ...m, signature: null })))).toBe(true);
      // the real (currently unset) pin, or any other root, must reject it
      expect(verifyRelease(dir).errors.join("\n")).toMatch(/signature: (root_unset|root_threshold)/);
      expect(sigErr(dir, { roots: pinOf([mkRoot()]).keys })).toEqual(["signature: root_threshold"]);
      const cli = spawnSync("node", [join(dir, "verify.mjs"), dir, "--require-signed"], { encoding: "utf8" });
      expect(cli.status).toBe(1); // verify.mjs carries the real pin, not the test root
    });
    it("rejects a tampered manifest", () => {
      const d = clone("t-manifest");
      rewrite(d, (m) => { m["version"] = "9.9.9"; });
      expect(sigErr(d)).toEqual(["signature: bad_signature"]);
    });
    it("rejects a tampered file", () => {
      const d = clone("t-file");
      writeFileSync(join(d, "site", "robots.txt"), "tampered");
      expect(verifyRelease(d, trust).ok).toBe(false);
    });
    it("rejects a tampered or stripped delegation", () => {
      const e = clone("t-deleg");
      rewrite(e, (m) => { (m["delegation"] as Record<string, unknown>)["expires"] = "2099-01-01T00:00:00Z"; });
      expect(sigErr(e)).toEqual(["signature: root_threshold"]);
      const s = clone("t-deleg-strip");
      rewrite(s, (m) => { delete m["delegation"]; });
      expect(sigErr(s)).toEqual(["signature: no_delegation"]);
    });
    it("rejects an expired delegation, a rolled-back delegation and a wrong keyId or alg", () => {
      expect(sigErr(dir, { now: Date.now() + 200 * 86_400_000 })).toEqual(["signature: delegation_expired"]);
      expect(sigErr(dir, { delegationFloor: 2 })).toEqual(["signature: delegation_rollback"]);
      const k = clone("t-keyid");
      rewrite(k, (m) => { (m["signature"] as Record<string, string>)["keyId"] = "om-release-evil"; });
      expect(sigErr(k)).toEqual(["signature: unlisted_key"]);
      const a = clone("t-alg");
      rewrite(a, (m) => { (m["signature"] as Record<string, string>)["alg"] = "rsa"; });
      expect(sigErr(a)).toEqual(["signature: alg_mismatch"]);
    });
    it("does not report an externally supplied signature as signed unless the whole chain verifies", { timeout: 120_000 }, () => {
      const sigFile = join(tmp, "bad-sig.json");
      writeFileSync(sigFile, JSON.stringify({ alg: "ed25519", keyId: ONLINE_ID, sig: Buffer.alloc(64, 7).toString("base64") }));
      const out = join(tmp, "bad-sig-out");
      const r = run("--no-zip", "--out", out, "--signature", sigFile, "--delegation", delegationFile, "--test-roots-file", rootsFile);
      expect(r.status).toBe(1);
      expect(r.stderr).toMatch(/does not verify/);
      expect(r.stderr).toMatch(/signature: bad_signature/);
      expect(r.stdout).not.toMatch(/signed om-web/);
    });
    it("signManifest output verifies under the TS trust root and a wrong key fails", async () => {
      const m = manifestOf(dir);
      const forged = { ...m, signature: { alg: "ed25519", keyId: ONLINE_ID, sig: Buffer.alloc(64, 1).toString("base64") } };
      expect(await verifyBundleSignature(forged, { roots: pinOf([root]), now: () => real })).toEqual({ ok: false, reason: "bad_signature" });
    });
  });
});
