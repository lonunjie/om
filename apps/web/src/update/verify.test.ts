import { describe, expect, it } from "vitest";
import { buildManifest } from "../../build/manifest";
import { commitFloor, evaluateManifest, type FloorStore } from "./floor";
import { sha256Hex } from "./hash";
import type { BundleManifest } from "./manifest";
import { verifyBundleSignature, type TrustRoot } from "./signature";
import { NOW, delegate, mkOnline, mkRoot, pinOf, signWith } from "../../build/testkit";
import { verifyLoadedBundle, type FetchFile } from "./verify";

const enc = (s: string) => new TextEncoder().encode(s);
const FILES: Record<string, string> = { "index.html": "<html>", "sw.js": "sw", "assets/a.js": "a" };
const mk = (over: Partial<BundleManifest> = {}, version = "1.0.0"): Promise<BundleManifest> =>
  buildManifest({ version, files: Object.entries(FILES).map(([path, d]) => ({ path, data: enc(d) })) }).then((m) => ({ ...m, ...over }));
const source = (files: Record<string, string>): FetchFile => (p) => {
  const d = files[p];
  return Promise.resolve(d === undefined ? undefined : { ok: true, arrayBuffer: () => Promise.resolve(enc(d).buffer) });
};
const fakeFloor = (initial: string | null = null, delegation: number | null = null, pq = false): FloorStore & { v: string | null; d: number | null; pq: boolean } => {
  const s = {
    v: initial,
    d: delegation,
    pq,
    get: () => Promise.resolve(s.v),
    set: (x: string) => {
      s.v = x;
      return Promise.resolve();
    },
    getDelegation: () => Promise.resolve(s.d),
    setDelegation: (x: number) => {
      s.d = x;
      return Promise.resolve();
    },
    getPqRequired: () => Promise.resolve(s.pq),
    setPqRequired: () => {
      s.pq = true;
      return Promise.resolve();
    },
  };
  return s;
};
// A throwaway root -> delegation -> online key chain; `signed` produces a manifest the whole chain verifies.
const ROOT = mkRoot();
const ONLINE = mkOnline();
const DELEGATION = delegate({ roots: [ROOT], online: ONLINE });
const SIGNING: TrustRoot = { roots: pinOf([ROOT]), now: () => NOW };
const signed = (m: BundleManifest, extra: Partial<BundleManifest> = {}): BundleManifest => signWith({ ...m, ...extra }, DELEGATION, ONLINE);

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
    expect(await verifyBundleSignature(m, { roots: pinOf([]) })).toEqual({ ok: false, reason: "unsigned" });
    expect(await verifyBundleSignature(m, { roots: pinOf([]), allowUnsignedDev: true })).toEqual({ ok: false, reason: "unsigned" });
  });
  it("accepts a dev-marked manifest only under an allowing trust root, flagged unsigned_dev", async () => {
    const m = await mk({ dev: true });
    expect(await verifyBundleSignature(m, { roots: pinOf([]), allowUnsignedDev: true })).toEqual({ ok: true, unsignedDev: true, delegationVersion: null });
    expect(await verifyBundleSignature(m, { roots: pinOf([]) })).toEqual({ ok: false, reason: "unsigned" });
  });
  it("accepts a real root -> delegation -> release-key -> manifest chain and returns the delegation version", async () => {
    expect(await verifyBundleSignature(signed(await mk()), SIGNING)).toEqual({ ok: true, unsignedDev: false, delegationVersion: 1 });
  });
  it("fails closed while no root is pinned (the pre-ceremony state), whatever is signed", async () => {
    expect(await verifyBundleSignature(signed(await mk()), { roots: { keys: {}, threshold: 1 }, now: () => NOW })).toEqual({ ok: false, reason: "root_unset" });
  });
  it("rejects a tampered manifest, a missing delegation and a wrong root", async () => {
    const m = signed(await mk());
    expect(await verifyBundleSignature({ ...m, version: "9.9.9" }, SIGNING)).toEqual({ ok: false, reason: "bad_signature" });
    const bare: Record<string, unknown> = { ...m };
    delete bare["delegation"];
    expect(await verifyBundleSignature(bare as unknown as BundleManifest, SIGNING)).toEqual({ ok: false, reason: "no_delegation" });
    expect(await verifyBundleSignature(m, { roots: pinOf([mkRoot()]), now: () => NOW })).toEqual({ ok: false, reason: "root_threshold" });
  });
  it("rejects a manifest signed by a key the delegation does not list, or with a wrong key id / alg", async () => {
    const m = await mk();
    expect(await verifyBundleSignature(signWith(m, DELEGATION, mkOnline()), SIGNING)).toEqual({ ok: false, reason: "bad_signature" });
    expect(await verifyBundleSignature(signWith(m, DELEGATION, ONLINE, "online-evil"), SIGNING)).toEqual({ ok: false, reason: "unlisted_key" });
    const ok = signed(m);
    const sg = ok.signature as NonNullable<BundleManifest["signature"]>;
    expect(await verifyBundleSignature({ ...ok, signature: { ...sg, alg: "rsa" } }, SIGNING)).toEqual({ ok: false, reason: "alg_mismatch" });
    expect(await verifyBundleSignature({ ...ok, signature: { ...sg, sig: "!!" } }, SIGNING)).toEqual({ ok: false, reason: "bad_signature" });
  });
  it("rejects an expired delegation and an expired or not-yet-valid release key", async () => {
    const m = await mk();
    const late = { roots: pinOf([ROOT]), now: () => NOW + 91 * 86_400_000 };
    expect(await verifyBundleSignature(signed(m), late)).toEqual({ ok: false, reason: "delegation_expired" });
    const shortKey = delegate({ roots: [ROOT], online: ONLINE, notAfter: "2026-11-02T00:00:00Z" });
    expect(await verifyBundleSignature(signWith(m, shortKey, ONLINE), { roots: pinOf([ROOT]), now: () => NOW + 5 * 86_400_000 })).toEqual({ ok: false, reason: "key_expired" });
    const future = delegate({ roots: [ROOT], online: ONLINE, notBefore: "2026-12-01T00:00:00Z" });
    expect(await verifyBundleSignature(signWith(m, future, ONLINE), SIGNING)).toEqual({ ok: false, reason: "key_not_yet_valid" });
  });
  it("rejects a revoked key, even one that is still listed", async () => {
    const m = await mk();
    const revoked = delegate({ roots: [ROOT], online: ONLINE, mutate: (d) => ({ ...d, revoked: ["online-1"] }) });
    expect(await verifyBundleSignature(signWith(m, revoked, ONLINE), SIGNING)).toEqual({ ok: false, reason: "revoked_key" });
  });
  it("rejects a delegation edited after the root signed it, and a forged delegation signed by another key", async () => {
    const m = await mk();
    const edited = { ...DELEGATION, keys: [...(DELEGATION["keys"] as unknown[]), { keyId: "x", alg: "ed25519", pub: "00".repeat(32), notBefore: "2026-01-01T00:00:00Z", notAfter: "2030-01-01T00:00:00Z" }] };
    expect(await verifyBundleSignature(signWith(m, edited, ONLINE), SIGNING)).toEqual({ ok: false, reason: "root_threshold" });
    const attacker = mkRoot();
    expect(await verifyBundleSignature(signWith(m, delegate({ roots: [attacker], online: ONLINE }), ONLINE), SIGNING)).toEqual({ ok: false, reason: "root_threshold" });
  });
  it("counts distinct pinned roots toward a 2-of-3 threshold (duplicates and strangers count for nothing)", async () => {
    const [r1, r2, r3] = [mkRoot(), mkRoot(), mkRoot()] as [ReturnType<typeof mkRoot>, ReturnType<typeof mkRoot>, ReturnType<typeof mkRoot>];
    const m = await mk();
    const pin = { roots: pinOf([r1, r2, r3], 2), now: () => NOW };
    expect(await verifyBundleSignature(signWith(m, delegate({ roots: [r1], online: ONLINE }), ONLINE), pin)).toEqual({ ok: false, reason: "root_threshold" });
    const dupe = delegate({ roots: [r1, r1], online: ONLINE });
    expect(await verifyBundleSignature(signWith(m, dupe, ONLINE), pin)).toEqual({ ok: false, reason: "root_threshold" });
    expect(await verifyBundleSignature(signWith(m, delegate({ roots: [r1, mkRoot()], online: ONLINE }), ONLINE), pin)).toEqual({ ok: false, reason: "root_threshold" });
    expect(await verifyBundleSignature(signWith(m, delegate({ roots: [r1, r3], online: ONLINE }), ONLINE), pin)).toMatchObject({ ok: true });
  });
  it("refuses a delegation below the persisted delegation floor (rollback), accepts equal or higher", async () => {
    const m = signed(await mk());
    expect(await verifyBundleSignature(m, SIGNING, 2)).toEqual({ ok: false, reason: "delegation_rollback" });
    expect(await verifyBundleSignature(m, SIGNING, 1)).toMatchObject({ ok: true });
    const v3 = signWith(await mk(), delegate({ roots: [ROOT], online: ONLINE, version: 3 }), ONLINE);
    expect(await verifyBundleSignature(v3, SIGNING, 2)).toEqual({ ok: true, unsignedDev: false, delegationVersion: 3 });
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
    const r = await evaluateManifest(signed(low, { rollback: marker }), floor, SIGNING);
    expect(r).toEqual({ ok: true, unsignedDev: false, rolledBack: true, delegationVersion: 1 });
    if (r.ok) await commitFloor(floor, r, "1.5.0");
    expect(floor.v).toBe("1.5.0");
    expect(await evaluateManifest(signed(await mk({}, "1.4.0"), { rollback: marker }), fakeFloor("2.0.0"), SIGNING)).toMatchObject({ ok: false });
    expect(await evaluateManifest(signed(low, { rollback: { from: "3.0.0", to: "1.5.0" } }), fakeFloor("2.0.0"), SIGNING)).toMatchObject({ ok: false });
    const dev = { ...low, dev: true as const, rollback: marker };
    expect(await evaluateManifest(dev, fakeFloor("2.0.0"), { roots: pinOf([]), allowUnsignedDev: true })).toEqual({ ok: false, reason: "rollback_refused" });
  });
  it("persists and enforces the delegation floor across evaluations, and never lowers it", async () => {
    const floor = fakeFloor();
    const base = await mk();
    const v = (n: number) => signWith(base, delegate({ roots: [ROOT], online: ONLINE, version: n }), ONLINE);
    const accept = async (m: BundleManifest) => {
      const r = await evaluateManifest(m, floor, SIGNING);
      if (r.ok) await commitFloor(floor, r, m.version);
      return r;
    };
    expect(await accept(v(3))).toMatchObject({ ok: true, delegationVersion: 3 });
    expect(floor.d).toBe(3);
    expect(await accept(v(2))).toEqual({ ok: false, reason: "delegation_rollback" });
    expect(floor.d).toBe(3);
    expect(await accept(v(3))).toMatchObject({ ok: true });
    expect(await accept(v(4))).toMatchObject({ ok: true });
    expect(floor.d).toBe(4);
  });
  it("sha256Hex is stable", async () => {
    expect(await sha256Hex(enc(""))).toBe("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
  });
});
