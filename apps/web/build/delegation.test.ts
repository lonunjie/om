// Shared-fixture test (audit B2): the TS verifier (service worker) and verify-release.mjs (operators) must give the
// SAME verdict, with the same reason string, for every case below; plus the ceremony tool's behaviour and safety.
import { spawnSync } from "node:child_process";
import { createPrivateKey, createHash, sign as edSign } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import { frameDelegationPayload, verifyChain, type Delegation } from "../src/update/delegation";
import { verifyBundleSignature } from "../src/update/signature";
import { buildManifest } from "./manifest";
import { DAY, NOW, delegate, mkOnline, mkRoot, pinOf, signWith, type Root } from "./testkit";
import { buildDelegation, generateRoot, loadRoot, parseTime } from "./root-ceremony.mjs";
import { frameDelegationPayload as frameMjs, verifyManifestSignature, type Delegation as MjsDelegation } from "./verify-release.mjs";

const tmp = mkdtempSync(join(tmpdir(), "om-ceremony-test-"));
afterAll(() => {
  rmSync(tmp, { recursive: true, force: true });
});
const ceremony = (...a: string[]) => spawnSync("node", [join(import.meta.dirname, "root-ceremony.mjs"), ...a], { encoding: "utf8" });
const ceremonyIn = (input: string, ...a: string[]) => spawnSync("node", [join(import.meta.dirname, "root-ceremony.mjs"), ...a], { encoding: "utf8", input });

const R1 = mkRoot();
const R2 = mkRoot();
const R3 = mkRoot();
const ONLINE = mkOnline();
const OTHER = mkOnline();
const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;
type Case = { name: string; delegation: unknown; pin: { keys: Record<string, string | { pub: string; frame: string }>; threshold: number }; signer?: typeof ONLINE; keyId?: string; now?: number; floor?: number | null; reason: string | null };
const good = delegate({ roots: [R1], online: ONLINE });
const pin1 = pinOf([R1]);
// Root framing (operator decision 2026-10-28): two Ledger-framed roots (software EMULATION of the Solana off-chain
// envelope, no device) and one raw software root; 2 of 3.
const LA = mkRoot("solana-offchain-v0");
const LB = mkRoot("solana-offchain-v0");
const SW = mkRoot();
const pin3 = pinOf([LA, LB, SW], 2);
type Sig = Record<string, unknown>;
const editSigs = (d: Record<string, unknown>, f: (s: Sig[]) => Sig[]) => ({ ...clone(d), signatures: f(clone(d["signatures"]) as Sig[]) });
const dropFrame = (s: Sig[]) => s.map((x) => Object.fromEntries(Object.entries(x).filter(([k]) => k !== "frame")));
const asRaw = (r: Root): Root => ({ ...r, frame: "raw" }); // same key, signs the unframed payload
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
  { name: "2-of-3: two Ledger-framed roots", delegation: delegate({ roots: [LA, LB], online: ONLINE }), pin: pin3, reason: null },
  { name: "2-of-3 mixed frames: Ledger + software", delegation: delegate({ roots: [LB, SW], online: ONLINE }), pin: pin3, reason: null },
  { name: "2-of-3: all three", delegation: delegate({ roots: [LA, LB, SW], online: ONLINE }), pin: pin3, reason: null },
  { name: "1-of-3 (Ledger only)", delegation: delegate({ roots: [LA], online: ONLINE }), pin: pin3, reason: "root_threshold" },
  { name: "1-of-3 (software only)", delegation: delegate({ roots: [SW], online: ONLINE }), pin: pin3, reason: "root_threshold" },
  { name: "duplicate signature entries from one key count once", delegation: editSigs(delegate({ roots: [LA], online: ONLINE }), (s) => [...s, ...s]), pin: pin3, reason: "root_threshold" },
  { name: "duplicate entries plus a second key still reach 2", delegation: editSigs(delegate({ roots: [LA, SW], online: ONLINE }), (s) => [...s, ...s]), pin: pin3, reason: null },
  { name: "Ledger signature that omits its frame", delegation: editSigs(delegate({ roots: [LA, SW], online: ONLINE }), dropFrame), pin: pin3, reason: "root_threshold" },
  { name: "Ledger key signed the raw payload (wrong frame, honestly declared raw)", delegation: delegate({ roots: [asRaw(LA), SW], online: ONLINE }), pin: pin3, reason: "root_threshold" },
  { name: "Ledger key signed the raw payload but declares the Ledger frame", delegation: editSigs(delegate({ roots: [asRaw(LA), SW], online: ONLINE }), (s) => s.map((x) => (x["keyId"] === LA.id ? { ...x, frame: "solana-offchain-v0" } : x))), pin: pin3, reason: "root_threshold" },
  { name: "software key declares a Ledger frame it is not pinned for", delegation: editSigs(delegate({ roots: [LA, SW], online: ONLINE }), (s) => s.map((x) => (x["keyId"] === SW.id ? { ...x, frame: "solana-offchain-v0" } : x))), pin: pin3, reason: "root_threshold" },
  { name: "Ledger key signed the Ledger frame but the pin says raw", delegation: delegate({ roots: [LA, SW], online: ONLINE }), pin: { keys: { ...pin3.keys, [LA.id]: LA.pub }, threshold: 2 }, reason: "root_threshold" },
  { name: "unknown declared frame is ignored", delegation: editSigs(delegate({ roots: [LA, SW], online: ONLINE }), (s) => s.map((x) => (x["keyId"] === LA.id ? { ...x, frame: "bogus/9" } : x))), pin: pin3, reason: "root_threshold" },
  { name: "unknown pinned frame fails closed", delegation: delegate({ roots: [LA, LB], online: ONLINE }), pin: { keys: { ...pin3.keys, [LA.id]: { pub: LA.pub, frame: "bogus/9" } }, threshold: 2 }, reason: "root_unset" },
  { name: "non-string signature frame is malformed", delegation: editSigs(delegate({ roots: [LA, LB], online: ONLINE }), (s) => s.map((x) => ({ ...x, frame: 7 }))), pin: pin3, reason: "malformed_delegation" },
  { name: "one public key pinned under two ids is unusable", delegation: delegate({ roots: [LA, LB], online: ONLINE }), pin: { keys: { ...pin3.keys, "root-clone": { pub: LA.pub, frame: "solana-offchain-v0" } }, threshold: 2 }, reason: "root_unset" },
  { name: "Ledger-framed roots, delegation edited after signing", delegation: { ...clone(delegate({ roots: [LA, LB], online: ONLINE })), version: 9 }, pin: pin3, reason: "root_threshold" },
  { name: "Ledger-framed roots, expiry edited after signing", delegation: { ...clone(delegate({ roots: [LA, LB], online: ONLINE })), expires: "2027-12-31T00:00:00Z" }, pin: pin3, reason: "root_threshold" },
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

describe("Ledger frame (Solana off-chain message, v0, format 0)", () => {
  // Independent oracle: built by a separate Python implementation (not this code) from the layout in
  // deploy/KEY-CEREMONY.md "Ledger framing", which follows LedgerHQ/app-solana (libsol/parser.c parse_offchain_message_header).
  const GOLDEN_DELEGATION = JSON.parse(
    '{"format":"om.release-delegation/1","version":3,"expires":"2027-01-26T00:00:00Z","keys":[{"keyId":"om-release-2026-10-28","alg":"ed25519","pub":"fad8cae9155ea0b7943c0b3f141a20f85b35c88ca5eacfe576a9bf578f28ebeb","notBefore":"2026-10-28T00:00:00Z","notAfter":"2027-01-26T00:00:00Z"}],"revoked":[],"signatures":[]}',
  ) as Delegation;
  const GOLDEN_JS = GOLDEN_DELEGATION as unknown as MjsDelegation;
  const must = <T>(v: T | null): T => {
    if (v === null) throw new Error("unexpected null");
    return v;
  };
  const SIGNER = "11".repeat(32);
  const GOLDEN_HEX =
    "ff736f6c616e61206f6666636861696e006f6d2e72656c656173652d64656c65676174696f6e2f31000000000000000000000111111111111111111111111111111111111111111111111111111111111111118a004f4d2072656c656173652064656c65676174696f6e207c2076657273696f6e2033207c206578706972657320323032372d30312d32365430303a30303a30305a207c207368613235362033643361363334663635646339383036373364336533623362633739386465323830656166353932653937336134373138623265633661386431366339306539";
  const hex = (b: Uint8Array) => Buffer.from(b).toString("hex");
  it("both implementations produce the golden envelope byte for byte", async () => {
    expect(hex(must(await frameDelegationPayload("solana-offchain-v0", GOLDEN_DELEGATION, SIGNER)))).toBe(GOLDEN_HEX);
    expect(hex(must(frameMjs("solana-offchain-v0", GOLDEN_JS, SIGNER)))).toBe(GOLDEN_HEX);
  });
  it("the envelope is the Solana v0 layout: domain, version, app domain, format 0, one signer, u16 LE length, ASCII text", () => {
    const b = Buffer.from(GOLDEN_HEX, "hex");
    expect(b.subarray(0, 16).toString("latin1")).toBe("\xffsolana offchain");
    expect(b[16]).toBe(0);
    expect(b.subarray(17, 49).toString("latin1").replace(/\0+$/, "")).toBe("om.release-delegation/1");
    expect([b[49], b[50]]).toEqual([0, 1]);
    expect(b.subarray(51, 83).toString("hex")).toBe(SIGNER);
    const len = b.readUInt16LE(83);
    expect(b.length).toBe(85 + len); // OFFCHAIN_MESSAGE_HEADER_LENGTH = 85 for one signer (app globals.h)
    const text = b.subarray(85).toString("latin1");
    expect(text).toMatch(/^[\x20-\x7e]+$/); // restricted ASCII: the device shows it in clear
    expect(len).toBeLessThanOrEqual(1212);
  });
  it("the frame binds the signer key, version, expiry and payload hash; raw returns the payload itself; unknown is null", async () => {
    const f = async (d: object, pub = SIGNER) => hex(must(await frameDelegationPayload("solana-offchain-v0", d as Delegation, pub)));
    const base = await f(GOLDEN_DELEGATION);
    expect(await f(GOLDEN_DELEGATION, "22".repeat(32))).not.toBe(base);
    expect(await f({ ...GOLDEN_DELEGATION, version: 4 })).not.toBe(base);
    expect(await f({ ...GOLDEN_DELEGATION, revoked: ["x"] })).not.toBe(base);
    expect(await f({ ...GOLDEN_DELEGATION, signatures: [{ keyId: "a", alg: "ed25519", sig: "AA==" }] })).toBe(base); // signatures are not signed
    expect(hex(must(await frameDelegationPayload("raw", GOLDEN_DELEGATION, SIGNER)))).toBe(hex(must(frameMjs("raw", GOLDEN_JS, SIGNER))));
    expect(await frameDelegationPayload("nope", GOLDEN_DELEGATION, SIGNER)).toBeNull();
    expect(frameMjs("nope", GOLDEN_JS, SIGNER)).toBeNull();
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

describe("root-ceremony: vault generation and external (hardware) signers", () => {
  const ONLINE_PUB = "fad8cae9155ea0b7943c0b3f141a20f85b35c88ca5eacfe576a9bf578f28ebeb";
  /** Stand-in for a Ledger: signs exactly the bytes in the payload file with a software key. NOT a device. */
  const emulatedLedgerSign = (payloadFile: string, r: Root) => edSign(null, readFileSync(payloadFile), createPrivateKey(r.pem)).toString("base64");

  it("gen-root --to-stdin-for-vault puts the PEM only in a Bitwarden item on stdout; no file; public data on stderr", () => {
    const r = ceremony("gen-root", "--to-stdin-for-vault", "--name", "OM test root");
    expect(r.status, r.stderr).toBe(0);
    const item = JSON.parse(r.stdout) as { type: number; name: string; notes: string; secureNote: { type: number }; fields: { name: string; value: string; type: number }[] };
    expect(item).toMatchObject({ type: 2, name: "OM test root", secureNote: { type: 0 } });
    // the key is a single-line HIDDEN custom field (type 1), never the note body (always visible in the UI)
    expect(item.fields).toHaveLength(1);
    expect(item.fields[0]).toMatchObject({ type: 1 });
    expect(item.fields[0]?.name).toMatch(/^ed25519-pkcs8-\d{4}-\d{2}-\d{2}$/);
    const body = item.fields[0]?.value ?? "";
    expect(body).toMatch(/^[A-Za-z0-9+/]{64}$/);
    expect(item.notes).not.toContain(body);
    expect(item.notes).not.toMatch(/PRIVATE KEY-----\n?[A-Za-z0-9+/]/);
    expect(r.stderr).not.toContain(body);
    expect(r.stderr).not.toMatch(/PRIVATE KEY/);
    const pub = /public key:\s+([0-9a-f]{64})/.exec(r.stderr)?.[1] ?? "";
    expect(r.stderr).toMatch(/root key id:\s+root-[0-9a-f]{16}/);
    expect(r.stderr).toMatch(/fingerprint:\s+sha256 [0-9a-f ]+/);
    // the vault copy is the key: show-root --root-key-stdin on `bw get item` JSON (or the bare field value) yields the same public key
    expect(ceremonyIn(JSON.stringify(item), "show-root", "--root-key-stdin").stdout).toContain(pub);
    expect(ceremonyIn(body, "show-root", "--root-key-stdin").stdout).toContain(pub);
    expect(ceremony("gen-root", "--to-stdin-for-vault", "--out", join(tmp, "x.pem")).stderr).toMatch(/never writes a file/);
    expect(existsSync(join(tmp, "x.pem"))).toBe(false);
  });

  it("show-root and cosign reject garbage on stdin without echoing it", () => {
    const r = ceremonyIn("SECRET-LOOKING-TEXT", "show-root", "--root-key-stdin");
    expect(r.status).toBe(1);
    expect(r.stderr).toMatch(/not a PKCS8/);
    expect(r.stderr + r.stdout).not.toContain("SECRET-LOOKING-TEXT");
  });

  it("full 2-of-3 flow: draft, two Ledger-framed signatures via external-payload/attach, software cosign over stdin, verify", () => {
    const LA2 = mkRoot("solana-offchain-v0");
    const LB2 = mkRoot("solana-offchain-v0");
    const vaultKey = ceremony("gen-root", "--to-stdin-for-vault");
    const vaultPem = vaultKey.stdout; // the whole item JSON, as `bw get item` would print it
    const swPub = /public key:\s+([0-9a-f]{64})/.exec(vaultKey.stderr)?.[1] ?? "";
    const d = join(tmp, "flow-delegation.json");
    expect(ceremony("draft", "--online-pub", ONLINE_PUB, "--key-id", "om-release-2026-10-28", "--expires", "+90d", "--out", d).status).toBe(0);
    expect(JSON.parse(readFileSync(d, "utf8"))).toMatchObject({ version: 1, signatures: [] });
    const pinSnippet = ceremony("pin-snippet", "--root", `${LA2.pub}=solana-offchain-v0`, "--root", `${LB2.pub}=solana-offchain-v0`, "--root", `${swPub}=raw`);
    expect(pinSnippet.status, pinSnippet.stderr).toBe(0);
    const rootsFile = join(tmp, "roots.json");
    writeFileSync(rootsFile, pinSnippet.stdout.trim().split("\n").pop() ?? "");
    const verifyNow = () => ceremony("verify", "--in", d, "--roots-file", rootsFile);
    expect(verifyNow().stdout).toMatch(/FAILED: root_threshold/);

    // Ledger A
    const payloadA = join(tmp, "payload-a.bin");
    const rep = ceremony("external-payload", "--in", d, "--signer-pub", LA2.pub, "--out", payloadA);
    expect(rep.status, rep.stderr).toBe(0);
    const parsed = JSON.parse(readFileSync(d, "utf8")) as Delegation;
    const framed = Buffer.from(readFileSync(payloadA));
    expect(framed.toString("hex")).toBe(Buffer.from(frameMjs("solana-offchain-v0", parsed as never, LA2.pub) ?? []).toString("hex"));
    const text = framed.subarray(85).toString("ascii");
    expect(rep.stdout).toContain(text); // the operator can compare this line with the device screen
    expect(rep.stdout).toContain(`sha256 ${createHash("sha256").update(framed).digest("hex")}`);
    expect(text).toMatch(/^OM release delegation \| version 1 \| expires \d{4}-\d\d-\d\dT[\d:]+Z \| sha256 [0-9a-f]{64}$/);
    expect(rep.stdout).toContain(`delegation payload sha256:  ${text.slice(-64)}`);
    const sigA = emulatedLedgerSign(payloadA, LA2);
    const helperJson = join(tmp, "sig-a.json");
    writeFileSync(helperJson, JSON.stringify({ path: "44'/501'/0'", pub: LA2.pub, sigBase64: sigA, sigHex: Buffer.from(sigA, "base64").toString("hex") }));
    expect(ceremony("external-attach", "--in", d, "--from-helper", helperJson, "--pub", LB2.pub).stderr).toMatch(/does not match/);
    const att = ceremony("external-attach", "--in", d, "--from-helper", helperJson);
    expect(att.status, att.stderr).toBe(0);
    expect(verifyNow().stdout).toMatch(/FAILED: root_threshold/); // 1 of 3
    // hex signatures are accepted too; a wrong-device or wrong-frame signature is refused and nothing changes
    const before = readFileSync(d, "utf8");
    const rawSig = edSign(null, Buffer.from(JSON.stringify(parsed)), createPrivateKey(LB2.pem)).toString("hex");
    const bad = ceremony("external-attach", "--in", d, "--pub", LB2.pub, "--sig", rawSig);
    expect(bad.status).toBe(1);
    expect(bad.stderr).toMatch(/does not verify/);
    const otherDevice = ceremony("external-attach", "--in", d, "--pub", LB2.pub, "--sig", sigA);
    expect(otherDevice.status).toBe(1);
    expect(readFileSync(d, "utf8")).toBe(before);

    // Ledger B, then 2-of-3 holds with two Ledgers
    const payloadB = join(tmp, "payload-b.bin");
    expect(ceremony("external-payload", "--in", d, "--signer-pub", LB2.pub, "--out", payloadB).status).toBe(0);
    expect(ceremony("external-attach", "--in", d, "--pub", LB2.pub, "--sig", Buffer.from(emulatedLedgerSign(payloadB, LB2), "base64").toString("hex")).status).toBe(0);
    expect(verifyNow().stdout).toMatch(/OK: 2-of-3/);

    // mixed frames: Ledger A + the vault software key (stdin), with Ledger B left out
    const d2 = join(tmp, "flow-delegation-2.json");
    expect(ceremony("draft", "--online-pub", ONLINE_PUB, "--key-id", "om-release-2026-10-28", "--expires", "+90d", "--out", d2).status).toBe(0);
    const payloadA2 = join(tmp, "payload-a2.bin");
    expect(ceremony("external-payload", "--in", d2, "--signer-pub", LA2.pub, "--out", payloadA2).status).toBe(0);
    expect(ceremony("external-attach", "--in", d2, "--pub", LA2.pub, "--sig", emulatedLedgerSign(payloadA2, LA2)).status).toBe(0);
    const co = ceremonyIn(vaultPem, "cosign", "--root-key-stdin", "--in", d2);
    expect(co.status, co.stderr).toBe(0);
    expect(co.stdout + co.stderr).not.toMatch(/PRIVATE KEY/);
    expect(ceremony("verify", "--in", d2, "--roots-file", rootsFile).stdout).toMatch(/OK: 2-of-3/);
    const signed = JSON.parse(readFileSync(d2, "utf8")) as Delegation;
    expect(signed.signatures.map((x) => x.frame)).toEqual(expect.arrayContaining(["solana-offchain-v0", undefined]));
  });

  it("pin-snippet insists on three distinct roots and known frames; external-payload refuses unknown frames", () => {
    const k = (n: string) => n.repeat(64);
    expect(ceremony("pin-snippet", "--root", `${k("a")}=raw`, "--root", `${k("b")}=raw`).stderr).toMatch(/exactly 3/);
    expect(ceremony("pin-snippet", "--root", `${k("a")}=raw`, "--root", `${k("a")}=raw`, "--root", `${k("b")}=raw`).stderr).toMatch(/twice/);
    expect(ceremony("pin-snippet", "--root", `${k("a")}=bogus`, "--root", `${k("b")}=raw`, "--root", `${k("c")}=raw`).stderr).toMatch(/unknown frame/);
    const d = join(tmp, "draft-for-frame.json");
    expect(ceremony("draft", "--online-pub", ONLINE_PUB, "--key-id", "k", "--expires", "+30d", "--out", d).status).toBe(0);
    expect(ceremony("external-payload", "--in", d, "--signer-pub", k("a"), "--frame", "bogus", "--out", join(tmp, "p.bin")).stderr).toMatch(/unknown frame/);
  });
});
