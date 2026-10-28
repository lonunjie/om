// Shared-fixture test (audit B2): the TS verifier (service worker) and verify-release.mjs (operators) must give the
// SAME verdict, with the same reason string, for every case below; plus the ceremony tool's behaviour and safety.
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import { verifyChain } from "../src/update/delegation";
import { verifyBundleSignature } from "../src/update/signature";
import { buildManifest } from "./manifest";
import { DAY, NOW, delegate, mkOnline, mkRoot, pinOf, signWith, type Root } from "./testkit";
import { buildDelegation, generateRoot, loadRoot, parseTime } from "./root-ceremony.mjs";
import { verifyManifestSignature } from "./verify-release.mjs";

const tmp = mkdtempSync(join(tmpdir(), "om-ceremony-test-"));
afterAll(() => {
  rmSync(tmp, { recursive: true, force: true });
});
const ceremony = (...a: string[]) => spawnSync("node", [join(import.meta.dirname, "root-ceremony.mjs"), ...a], { encoding: "utf8" });

const R1 = mkRoot();
const R2 = mkRoot();
const R3 = mkRoot();
const ONLINE = mkOnline();
const OTHER = mkOnline();
const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;
type Case = { name: string; delegation: unknown; pin: { keys: Record<string, string>; threshold: number }; signer?: typeof ONLINE; keyId?: string; now?: number; floor?: number | null; reason: string | null };
const good = delegate({ roots: [R1], online: ONLINE });
const pin1 = pinOf([R1]);
const cases: Case[] = [
  { name: "valid chain", delegation: good, pin: pin1, reason: null },
  { name: "valid chain, floor equal", delegation: good, pin: pin1, floor: 1, reason: null },
  { name: "no root pinned", delegation: good, pin: { keys: {}, threshold: 1 }, reason: "root_unset" },
  { name: "threshold 0", delegation: good, pin: { keys: pin1.keys, threshold: 0 }, reason: "root_unset" },
  { name: "threshold above N", delegation: good, pin: { keys: pin1.keys, threshold: 2 }, reason: "root_unset" },
  { name: "malformed root pin hex", delegation: good, pin: { keys: { [R1.id]: "zz" }, threshold: 1 }, reason: "root_unset" },
  { name: "not an object", delegation: "x", pin: pin1, reason: "malformed_delegation" },
  { name: "bad format", delegation: { ...clone(good), format: "other/1" }, pin: pin1, reason: "malformed_delegation" },
  { name: "version 0", delegation: { ...clone(good), version: 0 }, pin: pin1, reason: "malformed_delegation" },
  { name: "version 1.5", delegation: { ...clone(good), version: 1.5 }, pin: pin1, reason: "malformed_delegation" },
  { name: "bad expiry text", delegation: { ...clone(good), expires: "tomorrow" }, pin: pin1, reason: "malformed_delegation" },
  { name: "duplicate key ids", delegation: { ...clone(good), keys: [...(good["keys"] as unknown[]), ...(good["keys"] as unknown[])] }, pin: pin1, reason: "malformed_delegation" },
  { name: "bad key pub", delegation: { ...clone(good), keys: [{ ...(clone(good["keys"]) as Record<string, unknown>[])[0], pub: "AB" }] }, pin: pin1, reason: "malformed_delegation" },
  { name: "edited after signing", delegation: { ...clone(good), version: 7 }, pin: pin1, reason: "root_threshold" },
  { name: "signed by a stranger", delegation: delegate({ roots: [R2], online: ONLINE }), pin: pin1, reason: "root_threshold" },
  { name: "unsigned", delegation: delegate({ roots: [], online: ONLINE }), pin: pin1, reason: "root_threshold" },
  { name: "wrong alg on root signature", delegation: { ...clone(good), signatures: (good["signatures"] as Record<string, string>[]).map((s) => ({ ...s, alg: "rsa" })) }, pin: pin1, reason: "root_threshold" },
  { name: "garbage root signature", delegation: { ...clone(good), signatures: (good["signatures"] as Record<string, string>[]).map((s) => ({ ...s, sig: "!!" })) }, pin: pin1, reason: "root_threshold" },
  { name: "2-of-3 with two roots", delegation: delegate({ roots: [R1, R3], online: ONLINE }), pin: pinOf([R1, R2, R3], 2), reason: null },
  { name: "2-of-3 with one root", delegation: delegate({ roots: [R1], online: ONLINE }), pin: pinOf([R1, R2, R3], 2), reason: "root_threshold" },
  { name: "2-of-3 with one root twice", delegation: delegate({ roots: [R1, R1], online: ONLINE }), pin: pinOf([R1, R2, R3], 2), reason: "root_threshold" },
  { name: "2-of-3 with one root plus a stranger", delegation: delegate({ roots: [R1, mkRoot()], online: ONLINE }), pin: pinOf([R1, R2, R3], 2), reason: "root_threshold" },
  { name: "expired delegation", delegation: good, pin: pin1, now: NOW + 91 * DAY, reason: "delegation_expired" },
  { name: "expires exactly now", delegation: good, pin: pin1, now: NOW + 90 * DAY, reason: "delegation_expired" },
  { name: "rolled back below floor", delegation: good, pin: pin1, floor: 2, reason: "delegation_rollback" },
  { name: "revoked (still listed)", delegation: delegate({ roots: [R1], online: ONLINE, mutate: (d) => ({ ...d, revoked: ["online-1"] }) }), pin: pin1, reason: "revoked_key" },
  { name: "unlisted key id", delegation: good, pin: pin1, keyId: "online-2", reason: "unlisted_key" },
  { name: "key listed but signed by another key", delegation: good, pin: pin1, signer: OTHER, reason: "bad_signature" },
  { name: "key not yet valid", delegation: delegate({ roots: [R1], online: ONLINE, notBefore: "2026-12-01T00:00:00Z" }), pin: pin1, reason: "key_not_yet_valid" },
  { name: "key expired (delegation still fine)", delegation: delegate({ roots: [R1], online: ONLINE, notAfter: "2026-11-02T00:00:00Z" }), pin: pin1, now: NOW + 3 * DAY, reason: "key_expired" },
  { name: "key window includes its end", delegation: delegate({ roots: [R1], online: ONLINE, notAfter: "2026-11-02T00:00:00Z" }), pin: pin1, now: Date.parse("2026-11-02T00:00:00Z"), reason: null },
  { name: "alg other than ed25519 listed", delegation: delegate({ roots: [R1], online: ONLINE, mutate: (d) => ({ ...d, keys: (d["keys"] as Record<string, unknown>[]).map((k) => ({ ...k, alg: "rsa" })) }) }), pin: pin1, reason: "alg_mismatch" },
];

describe("TS verifier and verify-release.mjs agree on every case (shared fixture)", () => {
  const base = buildManifest({ version: "1.0.0", files: [{ path: "index.html", data: new TextEncoder().encode("x") }, { path: "sw.js", data: new TextEncoder().encode("y") }] });
  it.each(cases)("$name", async (c) => {
    const m = signWith(await base, c.delegation, c.signer ?? ONLINE, c.keyId ?? "online-1");
    const now = c.now ?? NOW;
    const floor = c.floor ?? null;
    const ts = await verifyBundleSignature(m, { roots: c.pin, now: () => now }, floor);
    const js = verifyManifestSignature(m, c.pin, { now, floor });
    expect(ts.ok).toBe(c.reason === null);
    expect(js.ok).toBe(c.reason === null);
    if (c.reason !== null) {
      expect(ts).toEqual({ ok: false, reason: c.reason });
      expect(js).toEqual({ ok: false, reason: c.reason });
    } else {
      expect(ts).toMatchObject({ ok: true });
      expect(js).toMatchObject({ ok: true });
    }
    // the chain function alone (what the SW uses) gives the same answer
    const chain = await verifyChain(m.delegation, c.pin, { now, floor }, c.keyId ?? "online-1", "ed25519");
    if (c.reason !== null && c.reason !== "bad_signature") expect(chain).toEqual({ ok: false, reason: c.reason });
  });
  it("a manifest edit after signing fails identically in both", async () => {
    const m = { ...signWith(await base, good, ONLINE), version: "2.0.0" };
    expect(await verifyBundleSignature(m, { roots: pin1, now: () => NOW })).toEqual({ ok: false, reason: "bad_signature" });
    expect(verifyManifestSignature(m, pin1, { now: NOW })).toEqual({ ok: false, reason: "bad_signature" });
  });
});

describe("root-ceremony library", () => {
  const writeKey = (name: string, r: Root, mode = 0o600) => {
    const f = join(tmp, name);
    writeFileSync(f, r.pem, { mode });
    return f;
  };
  it("builds monotonic delegations: renewal bumps the version, rotation adds a key, revocation moves it to revoked for good", () => {
    const v1 = buildDelegation({ expires: "+90d", addKey: { keyId: "k1", pub: ONLINE.pub }, now: NOW });
    expect(v1).toMatchObject({ version: 1, revoked: [] });
    const v2 = buildDelegation({ prev: v1, expires: "+90d", addKey: { keyId: "k2", pub: OTHER.pub }, now: NOW });
    expect(v2.version).toBe(2);
    expect(v2.keys.map((k) => k.keyId)).toEqual(["k1", "k2"]);
    const v3 = buildDelegation({ prev: v2, expires: "+90d", revokeId: "k1", now: NOW });
    expect(v3).toMatchObject({ version: 3, revoked: ["k1"] });
    expect(v3.keys.map((k) => k.keyId)).toEqual(["k2"]);
    expect(() => buildDelegation({ prev: v3, expires: "+90d", addKey: { keyId: "k1", pub: ONLINE.pub }, now: NOW })).toThrow(/never reused/);
    expect(buildDelegation({ prev: v3, expires: "+30d", now: NOW }).version).toBe(4);
    expect(() => buildDelegation({ prev: v3, expires: "+900d", now: NOW })).toThrow(/more than 400 days/);
    expect(() => buildDelegation({ expires: "2020-01-01", now: NOW })).toThrow(/in the past/);
    expect(() => buildDelegation({ prev: v3, expires: "+10d", revokeId: "ghost", now: NOW })).toThrow(/not in the delegation/);
    expect(() => buildDelegation({ expires: "+10d", addKey: { keyId: "bad id!", pub: ONLINE.pub }, now: NOW })).toThrow(/key-id/);
    expect(() => buildDelegation({ expires: "+10d", addKey: { keyId: "k", pub: "abc" }, now: NOW })).toThrow(/online-pub/);
  });
  it("parses times", () => {
    expect(parseTime("2027-01-02")).toBe("2027-01-02T00:00:00Z");
    expect(parseTime("+1d", NOW)).toBe("2026-11-02T00:00:00Z");
    expect(() => parseTime("soon")).toThrow(/bad time/);
  });
  it("loadRoot refuses loose permissions, wrong files and wrong key types without echoing content", () => {
    const r = generateRoot();
    const ok = writeKey("root-ok.pem", { ...mkRoot(), pem: r.pem });
    expect(loadRoot(ok).id).toBe(r.id);
    const loose = writeKey("root-loose.pem", mkRoot(), 0o644);
    expect(() => loadRoot(loose)).toThrow(/must be 600/);
    const junk = join(tmp, "junk.pem");
    writeFileSync(junk, "SECRET-LOOKING-TEXT", { mode: 0o600 });
    expect(() => loadRoot(junk)).toThrow(/not a PKCS8/);
    try { loadRoot(junk); } catch (e) { expect(String(e)).not.toContain("SECRET-LOOKING-TEXT"); }
    expect(() => loadRoot(join(tmp, "missing.pem"))).toThrow(/cannot read/);
  });
});

describe("root-ceremony CLI", () => {
  it("gen-root writes a 0600 key only where told, prints only public data, and never overwrites", () => {
    const keyPath = join(tmp, "media-root.pem");
    const r = ceremony("gen-root", "--out", keyPath);
    // the temp dir is outside the repo; it is not removable media, which only warns
    expect(r.status, r.stderr).toBe(0);
    expect(statSync(keyPath).mode & 0o777).toBe(0o600);
    const pem = readFileSync(keyPath, "utf8");
    expect(pem).toMatch(/BEGIN PRIVATE KEY/);
    const body = pem.replace(/-----[A-Z ]+-----|\s/g, "");
    expect(r.stdout + r.stderr).not.toContain(body);
    expect(r.stdout + r.stderr).not.toMatch(/PRIVATE KEY/);
    expect(r.stdout).toMatch(/root key id:\s+root-[0-9a-f]{16}/);
    expect(r.stdout).toMatch(/public key:\s+[0-9a-f]{64}/);
    expect(r.stdout).toMatch(/fingerprint:\s+sha256 [0-9a-f ]+/);
    expect(r.stderr).toMatch(/does not look like removable media/);
    const loaded = loadRoot(keyPath);
    expect(readFileSync(`${keyPath}.pub`, "utf8")).toContain(loaded.pub);
    const again = ceremony("gen-root", "--out", keyPath);
    expect(again.status).toBe(1);
    expect(again.stderr).toMatch(/already exists/);
    expect(readFileSync(keyPath, "utf8")).toBe(pem);
    expect(ceremony("show-root", "--root-key", keyPath).stdout).toContain(loaded.pub);
  });
  it("gen-root refuses a relative path, a missing directory and anywhere inside this git work tree", () => {
    expect(ceremony("gen-root", "--out", "root.pem").stderr).toMatch(/absolute/);
    expect(ceremony("gen-root", "--out", join(tmp, "nodir", "root.pem")).stderr).toMatch(/does not exist/);
    const inRepo = join(import.meta.dirname, "..", "should-not-exist.pem");
    const r = ceremony("gen-root", "--out", inRepo);
    expect(r.status).toBe(1);
    expect(r.stderr).toMatch(/git work tree/);
    expect(existsSync(inRepo)).toBe(false);
  });
  it("sign-delegation, cosign, revoke and inspect produce delegations the verifiers accept", async () => {
    const k1 = join(tmp, "k1.pem");
    const k2 = join(tmp, "k2.pem");
    expect(ceremony("gen-root", "--out", k1).status).toBe(0);
    expect(ceremony("gen-root", "--out", k2).status).toBe(0);
    const r1 = loadRoot(k1);
    const r2 = loadRoot(k2);
    const pin = { keys: { [r1.id]: r1.pub, [r2.id]: r2.pub }, threshold: 2 };
    const d = join(tmp, "delegation.json");
    const online = mkOnline();
    const s = ceremony("sign-delegation", "--root-key", k1, "--online-pub", online.pub, "--key-id", "online-1", "--expires", "+60d", "--out", d);
    expect(s.status, s.stderr).toBe(0);
    expect(s.stdout + s.stderr).not.toMatch(/PRIVATE KEY/);
    const base = await buildManifest({ version: "1.0.0", files: [{ path: "index.html", data: new Uint8Array([1]) }, { path: "sw.js", data: new Uint8Array([2]) }] });
    const check = (file: string) => verifyManifestSignature(signWith(base, JSON.parse(readFileSync(file, "utf8")), online), pin, { now: Date.now() });
    expect(check(d)).toEqual({ ok: false, reason: "root_threshold" }); // 1 of 2
    expect(ceremony("cosign", "--root-key", k2, "--in", d).status).toBe(0);
    expect(check(d)).toMatchObject({ ok: true, delegationVersion: 1 });
    const rev = join(tmp, "delegation-2.json");
    const v = ceremony("revoke", "--root-key", k1, "--root-key", k2, "--prev", d, "--key-id", "online-1", "--out", rev);
    expect(v.status, v.stderr).toBe(0);
    expect(check(rev)).toEqual({ ok: false, reason: "revoked_key" });
    expect(JSON.parse(readFileSync(rev, "utf8"))).toMatchObject({ version: 2, revoked: ["online-1"], keys: [] });
    expect(ceremony("inspect", "--in", rev).stdout).toMatch(/revoked: online-1/);
    expect(ceremony("sign-delegation", "--root-key", k1, "--expires", "+10d").stderr).toMatch(/--online-pub is required/);
  });
});
