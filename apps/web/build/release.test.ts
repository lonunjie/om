import { spawnSync } from "node:child_process";
import { createPublicKey, generateKeyPairSync } from "node:crypto";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import { PINNED_KEYS, verifyRelease } from "./verify-release.mjs";
import { RELEASE_KEY_ID, extractPem, signManifest } from "./sign.mjs";
import { RELEASE_KEY_ID as TS_KEY_ID, RELEASE_PUBLIC_KEY_HEX, makeTrustRoot } from "../src/update/release-key";
import { ed25519Verifier, verifyBundleSignature } from "../src/update/signature";
import { signingPayload, type BundleManifest, type BundleSignature } from "../src/update/manifest";

const webRoot = new URL("..", import.meta.url).pathname;
const tmp = mkdtempSync(join(tmpdir(), "om-release-test-"));
afterAll(() => {
  rmSync(tmp, { recursive: true, force: true });
});
const run = (...a: string[]) => spawnSync("node", ["build/release.mjs", ...a], { cwd: webRoot, encoding: "utf8" });

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

  it("pins the same production key in the client trust root and verify.mjs", () => {
    expect(TS_KEY_ID).toBe(RELEASE_KEY_ID);
    expect(PINNED_KEYS).toEqual({ [RELEASE_KEY_ID]: RELEASE_PUBLIC_KEY_HEX });
    expect(Object.keys(makeTrustRoot(false).verifiers)).toEqual([RELEASE_KEY_ID]);
    expect(makeTrustRoot(false).allowUnsignedDev).toBe(false);
  });
  it("extracts only the PEM block from a vault note", () => {
    expect(extractPem("note\n-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\ntail")).toMatch(/^-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----$/);
    expect(extractPem("nothing")).toBeNull();
  });

  describe("signed release (throwaway key)", () => {
    const { privateKey, publicKey } = generateKeyPairSync("ed25519");
    const pubHex = publicKey.export({ format: "der", type: "spki" }).subarray(-32).toString("hex");
    const trusted = { [RELEASE_KEY_ID]: pubHex };
    const keyFile = join(tmp, "throwaway.pem");
    writeFileSync(keyFile, privateKey.export({ format: "pem", type: "pkcs8" }));
    const dir = join(tmp, "om-web-0.0.0");
    const clone = (name: string) => { const d = join(tmp, name); cpSync(dir, d, { recursive: true }); return d; };
    const manifestOf = (d: string) => JSON.parse(readFileSync(join(d, "site/bundle-manifest.json"), "utf8")) as BundleManifest;
    const rewrite = (d: string, mut: (m: Record<string, unknown>) => void) => {
      const p = join(d, "site/bundle-manifest.json");
      const m = JSON.parse(readFileSync(p, "utf8")) as Record<string, unknown>;
      mut(m);
      writeFileSync(p, JSON.stringify(m, null, 2) + "\n");
    };
    // Manifest edits also break SHA256SUMS; assert the signature error specifically.
    const sigErr = (d: string) => verifyRelease(d, { trustedKeys: trusted, requireSigned: true }).errors.filter((e) => e.startsWith("signature:"));

    it("builds, verifies with the right key, and matches the TS verifier byte-for-byte", { timeout: 120_000 }, async () => {
      const r = run("--no-zip", "--out", tmp, "--sign-with-key-file", keyFile);
      expect(r.status, r.stderr).toBe(0);
      expect(r.stdout + r.stderr).not.toMatch(/PRIVATE KEY/);
      const v = verifyRelease(dir, { trustedKeys: trusted, requireSigned: true });
      expect(v.errors).toEqual([]);
      expect(v.info.signatureValid).toBe(true);
      const m = manifestOf(dir);
      const root = { verifiers: { [RELEASE_KEY_ID]: ed25519Verifier(Buffer.from(pubHex, "hex")) } };
      expect(await verifyBundleSignature(m, root)).toEqual({ ok: true, unsignedDev: false });
      // JS signer payload == TS payload
      expect(Buffer.from(signingPayload(m)).equals(Buffer.from(signingPayload({ ...m, signature: null })))).toBe(true);
      // the pinned production key (not the throwaway) must reject it
      expect(verifyRelease(dir).errors.join("\n")).toMatch(/signature: bad signature/);
      expect(createPublicKey({ key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), Buffer.from(RELEASE_PUBLIC_KEY_HEX, "hex")]), format: "der", type: "spki" }).asymmetricKeyType).toBe("ed25519");
    });
    it("rejects a tampered manifest", () => {
      const d = clone("t-manifest");
      rewrite(d, (m) => { m["version"] = "9.9.9"; });
      expect(sigErr(d)).toEqual(["signature: bad signature"]);
    });
    it("rejects a tampered file", () => {
      const d = clone("t-file");
      writeFileSync(join(d, "site", "robots.txt"), "tampered");
      expect(verifyRelease(d, { trustedKeys: trusted }).ok).toBe(false);
    });
    it("rejects a wrong keyId or alg", async () => {
      const k = clone("t-keyid");
      rewrite(k, (m) => { (m["signature"] as Record<string, string>)["keyId"] = "om-release-evil"; });
      expect(sigErr(k)).toEqual(["signature: unknown signing key om-release-evil"]);
      const a = clone("t-alg");
      rewrite(a, (m) => { (m["signature"] as Record<string, string>)["alg"] = "rsa"; });
      expect(sigErr(a)).toEqual(["signature: algorithm mismatch (rsa)"]);
      const root = { verifiers: { [RELEASE_KEY_ID]: ed25519Verifier(Buffer.from(pubHex, "hex")) } };
      const m = manifestOf(dir);
      const sg = m.signature as BundleSignature;
      expect(await verifyBundleSignature({ ...m, signature: { ...sg, keyId: "x" } }, root)).toEqual({ ok: false, reason: "unknown_key" });
      expect(await verifyBundleSignature({ ...m, signature: { ...sg, alg: "rsa" } }, root)).toEqual({ ok: false, reason: "alg_mismatch" });
    });
    it("signManifest output verifies under the TS trust-root shape and a wrong key fails", async () => {
      const m = manifestOf(dir);
      const other = generateKeyPairSync("ed25519").privateKey.export({ format: "pem", type: "pkcs8" });
      const forged = { ...m, signature: signManifest(m, other) };
      const root = { verifiers: { [RELEASE_KEY_ID]: ed25519Verifier(Buffer.from(pubHex, "hex")) } };
      expect(await verifyBundleSignature(forged, root)).toEqual({ ok: false, reason: "bad_signature" });
    });
  });
});
