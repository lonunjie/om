import { describe, expect, it } from "vitest";
import { buildManifest } from "../../build/manifest";
import { commitFloor, evaluateManifest, type FloorStore } from "./floor";
import { sha256Hex } from "./hash";
import type { BundleManifest } from "./manifest";
import { ed25519Verifier, verifyBundleSignature, type TrustRoot } from "./signature";
import { verifyLoadedBundle, type FetchFile } from "./verify";

const enc = (s: string) => new TextEncoder().encode(s);
const FILES: Record<string, string> = { "index.html": "<html>", "sw.js": "sw", "assets/a.js": "a" };
const mk = (over: Partial<BundleManifest> = {}, version = "1.0.0"): Promise<BundleManifest> =>
  buildManifest({ version, files: Object.entries(FILES).map(([path, d]) => ({ path, data: enc(d) })) }).then((m) => ({ ...m, ...over }));
const source = (files: Record<string, string>): FetchFile => (p) => {
  const d = files[p];
  return Promise.resolve(d === undefined ? undefined : { ok: true, arrayBuffer: () => Promise.resolve(enc(d).buffer) });
};
const fakeFloor = (initial: string | null = null): FloorStore & { v: string | null } => {
  const s = {
    v: initial,
    get: () => Promise.resolve(s.v),
    set: (x: string) => {
      s.v = x;
      return Promise.resolve();
    },
  };
  return s;
};
const SIGNING: TrustRoot = { verifiers: { k1: { alg: "test", verify: (_p, sig) => Promise.resolve(new TextDecoder().decode(sig) === "ok") } } };
const signed = (m: BundleManifest, sig = "ok", extra: Partial<BundleManifest> = {}): BundleManifest => ({
  ...m,
  ...extra,
  signature: { alg: "test", keyId: "k1", sig: btoa(sig) },
});

describe("verifyLoadedBundle", () => {
  it("accepts an exact match", async () => {
    expect(await verifyLoadedBundle(await mk(), source(FILES), () => Promise.resolve(Object.keys(FILES)))).toEqual({ ok: true });
  });
  it("rejects a tampered file (same size and different size)", async () => {
    expect(await verifyLoadedBundle(await mk(), source({ ...FILES, "assets/a.js": "b" }))).toMatchObject({ ok: false, reason: "hash_mismatch", path: "assets/a.js" });
    expect(await verifyLoadedBundle(await mk(), source({ ...FILES, "assets/a.js": "bb" }))).toMatchObject({ ok: false, reason: "size_mismatch" });
  });
  it("rejects a missing file", async () => {
    const rest = Object.fromEntries(Object.entries(FILES).filter(([p]) => p !== "sw.js"));
    expect(await verifyLoadedBundle(await mk(), source(rest))).toMatchObject({ ok: false, reason: "missing", path: "sw.js" });
  });
  it("rejects an extra unlisted file in the shell cache", async () => {
    const r = await verifyLoadedBundle(await mk(), source(FILES), () => Promise.resolve([...Object.keys(FILES), "evil.js"]));
    expect(r).toEqual({ ok: false, reason: "unlisted_file", path: "evil.js" });
  });
  it("treats a throwing fetch as missing", async () => {
    expect(await verifyLoadedBundle(await mk(), () => Promise.reject(new Error("net")))).toMatchObject({ ok: false, reason: "missing" });
  });
});

describe("verifyBundleSignature (fail closed)", () => {
  it("rejects an unsigned production manifest, even under a dev-permissive trust root when not marked dev", async () => {
    const m = await mk();
    expect(await verifyBundleSignature(m, { verifiers: {} })).toEqual({ ok: false, reason: "unsigned" });
    expect(await verifyBundleSignature(m, { verifiers: {}, allowUnsignedDev: true })).toEqual({ ok: false, reason: "unsigned" });
  });
  it("accepts a dev-marked manifest only under an allowing trust root, flagged unsigned_dev", async () => {
    const m = await mk({ dev: true });
    expect(await verifyBundleSignature(m, { verifiers: {}, allowUnsignedDev: true })).toEqual({ ok: true, unsignedDev: true });
    expect(await verifyBundleSignature(m, { verifiers: {} })).toEqual({ ok: false, reason: "unsigned" });
  });
  it("checks key pin, algorithm and signature", async () => {
    const m = await mk();
    expect(await verifyBundleSignature(signed(m), SIGNING)).toEqual({ ok: true, unsignedDev: false });
    expect(await verifyBundleSignature(signed(m, "bad"), SIGNING)).toMatchObject({ reason: "bad_signature" });
    expect(await verifyBundleSignature({ ...signed(m), signature: { alg: "test", keyId: "other", sig: "b2s=" } }, SIGNING)).toMatchObject({ reason: "unknown_key" });
    expect(await verifyBundleSignature({ ...signed(m), signature: { alg: "rsa", keyId: "k1", sig: "b2s=" } }, SIGNING)).toMatchObject({ reason: "alg_mismatch" });
    expect(await verifyBundleSignature({ ...signed(m), signature: { alg: "test", keyId: "k1", sig: "!!" } }, SIGNING)).toMatchObject({ reason: "bad_signature" });
  });
  it("verifies a real Ed25519 signature over the signing payload and rejects tampering", async () => {
    const { signingPayload } = await import("./manifest");
    const kp = await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"]);
    const raw = new Uint8Array(await crypto.subtle.exportKey("raw", kp.publicKey));
    const trust: TrustRoot = { verifiers: { k1: ed25519Verifier(raw) } };
    const m = await mk();
    const sig = new Uint8Array(await crypto.subtle.sign({ name: "Ed25519" }, kp.privateKey, signingPayload(m) as BufferSource));
    const ok = { ...m, signature: { alg: "ed25519", keyId: "k1", sig: btoa(String.fromCharCode(...sig)) } };
    expect(await verifyBundleSignature(ok, trust)).toEqual({ ok: true, unsignedDev: false });
    expect(await verifyBundleSignature({ ...ok, version: "9.9.9" }, trust)).toMatchObject({ ok: false, reason: "bad_signature" });
  });
});

describe("rollback / freeze floor", () => {
  it("refuses a lower version, accepts equal and higher, and persists the floor", async () => {
    const floor = fakeFloor();
    const v = async (version: string) => signed(await mk({}, version));
    const first = await evaluateManifest(await v("1.2.0"), floor, SIGNING);
    expect(first).toMatchObject({ ok: true });
    if (first.ok) await commitFloor(floor, first, "1.2.0");
    expect(floor.v).toBe("1.2.0");
    expect(await evaluateManifest(await v("1.1.9"), floor, SIGNING)).toEqual({ ok: false, reason: "rollback_refused" });
    expect(await evaluateManifest(await v("1.2.0"), floor, SIGNING)).toMatchObject({ ok: true });
    const up = await evaluateManifest(await v("1.10.0"), floor, SIGNING);
    expect(up).toMatchObject({ ok: true });
    if (up.ok) await commitFloor(floor, up, "1.10.0");
    expect(floor.v).toBe("1.10.0");
    // a fresh evaluator over the same store (worker restart) still refuses
    expect(await evaluateManifest(await v("1.9.0"), floor, SIGNING)).toMatchObject({ ok: false });
  });
  it("allows a downgrade only with a matching signed rollback marker; unsigned dev can never roll back", async () => {
    const floor = fakeFloor("2.0.0");
    const low = await mk({}, "1.5.0");
    const marker = { from: "2.0.0", to: "1.5.0" };
    const r = await evaluateManifest(signed(low, "ok", { rollback: marker }), floor, SIGNING);
    expect(r).toEqual({ ok: true, unsignedDev: false, rolledBack: true });
    if (r.ok) await commitFloor(floor, r, "1.5.0");
    expect(floor.v).toBe("1.5.0");
    expect(await evaluateManifest(signed(await mk({}, "1.4.0"), "ok", { rollback: marker }), fakeFloor("2.0.0"), SIGNING)).toMatchObject({ ok: false });
    expect(await evaluateManifest(signed(low, "ok", { rollback: { from: "3.0.0", to: "1.5.0" } }), fakeFloor("2.0.0"), SIGNING)).toMatchObject({ ok: false });
    const dev = { ...low, dev: true as const, rollback: marker };
    expect(await evaluateManifest(dev, fakeFloor("2.0.0"), { verifiers: {}, allowUnsignedDev: true })).toEqual({ ok: false, reason: "rollback_refused" });
  });
  it("sha256Hex is stable", async () => {
    expect(await sha256Hex(enc(""))).toBe("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
  });
});
