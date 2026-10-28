// ADR-0182: ML-DSA-65 co-signature on the release-root delegation. Shared-fixture test: the TS verifier (service worker,
// @noble/post-quantum) and verify-release.mjs (operators, node:crypto/OpenSSL) must give the SAME verdict and reason for every
// case, plus the persisted PQ floor (no downgrade), the signed-bytes framing, the pins and the ceremony commands.
// Throwaway keys only; nothing here is a real key.
import { spawnSync } from "node:child_process";
import { createHash, createPrivateKey, sign as nodeSign } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import { ml_dsa65 } from "@noble/post-quantum/ml-dsa.js";
import { createShell, type KV, type ShellCache, type ShellStorage } from "../src/sw/core";
import { frameDelegationPayload, ledgerText, verifyChain, type Delegation } from "../src/update/delegation";
import { commitFloor, evaluateManifest, type FloorStore } from "../src/update/floor";
import { PQ_ALG, PQ_FRAME, PQ_PUBLIC_KEY_BYTES, PQ_SIGNATURE_BYTES, describePq, pqSigningBytes as pqSigningBytesTs, verifyPqSignature } from "../src/update/pq";
import { PQ_REQUIRED_BY_BUILD, PQ_ROOT } from "../src/update/pq-root";
import { verifyBundleSignature } from "../src/update/signature";
import { buildManifest, serializeManifest } from "./manifest";
import { addPqSignature, checkDescriptor, formatPqNote, generatePqRoot, loadPqRootNote, pqDescriptor, pqKeyId } from "./pq-ceremony.mjs";
import { addRootSignature, buildDelegation } from "./root-ceremony.mjs";
import { requireDelegation } from "./sign.mjs";
import { verifyRelease } from "./verify-release.mjs";
import { NOW, delegate, mkOnline, mkRoot, pinOf, signWith } from "./testkit";
import { PINNED_PQ_REQUIRED, PINNED_PQ_ROOT, ledgerText as ledgerTextMjs, pqSigningBytes as pqSigningBytesMjs, verifyManifestSignature, type Delegation as MjsDelegation } from "./verify-release.mjs";

const tmp = mkdtempSync(join(tmpdir(), "om-pq-test-"));
afterAll(() => {
  rmSync(tmp, { recursive: true, force: true });
});
const ceremony = (...a: string[]) => spawnSync("node", [join(import.meta.dirname, "root-ceremony.mjs"), ...a], { encoding: "utf8" });
const ceremonyIn = (input: string, ...a: string[]) => spawnSync("node", [join(import.meta.dirname, "root-ceremony.mjs"), ...a], { encoding: "utf8", input });
const readdirNames = (d: string): string[] => readdirSync(d);
const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;
type Rec = Record<string, unknown>;

const R1 = mkRoot();
const R2 = mkRoot();
const R3 = mkRoot();
const ONLINE = mkOnline();
const PQ = generatePqRoot();
const PQ_OTHER = generatePqRoot();
const PIN = pqDescriptor(PQ.pub);
const ed = pinOf([R1, R2, R3], 2);
type Pin = { keys: typeof ed.keys; threshold: number; pq?: unknown; pqRequired?: boolean };
const pinPq: Pin = { ...ed, pq: PIN };

const pqEpoch = (d: Rec): Rec => ({ ...d, pqRequired: true });
const edOnly = delegate({ roots: [R1, R2], online: ONLINE });
const edOnlyEpoch = delegate({ roots: [R1, R2], online: ONLINE, mutate: pqEpoch });
const withPq = (d: Rec, root = PQ): Rec => addPqSignature(d, root) as unknown as Rec;
const goodPq = withPq(edOnly); // transition: PQ co-signature on an ordinary delegation
const goodEpoch = withPq(edOnlyEpoch); // the PQ epoch, fully signed
const sigsOf = (d: Rec) => clone(d["signatures"]) as Rec[];
const editPq = (d: Rec, f: (e: Rec) => Rec): Rec => ({ ...clone(d), signatures: sigsOf(d).map((s) => (s["keyId"] === PIN.keyId ? f(s) : s)) });
const BASE = buildManifest({ version: "1.0.0", files: [{ path: "index.html", data: new Uint8Array([1]) }, { path: "sw.js", data: new Uint8Array([2]) }] });
/** A manifest signed by the online key under `delegation` (no validation: tests build bad chains on purpose). */
const manifestOf = async (delegation: unknown, version = "1.0.0") => signWith({ ...(await BASE), version }, delegation, ONLINE);
const flip = (b64: string): string => {
  const b = Buffer.from(b64, "base64");
  b[100] = (b[100] ?? 0) ^ 1;
  return b.toString("base64");
};
/** PQ signature of a DIFFERENT key presented under the pinned key's id. */
const forged = (d: Rec): Rec => ({ ...clone(d), signatures: [...sigsOf(d).filter((s) => s["keyId"] !== PIN.keyId), { ...sigsOf(withPq(edOnlyEpoch, PQ_OTHER)).find((s) => s["keyId"] === pqKeyId(PQ_OTHER.pub)), keyId: PIN.keyId }] });
// a delegation that the PQ root signed at v1, then edited (version 2) and re-signed by the Ed25519 roots, keeping the old PQ entry
const edited = (() => {
  const base = { ...clone(withPq(delegate({ roots: [], online: ONLINE, mutate: pqEpoch }))), version: 2 };
  let d = base as unknown;
  d = addRootSignature(d, R1);
  d = addRootSignature(d, R2);
  return d as Rec;
})();

type Case = { name: string; delegation: unknown; pin: Pin; pqFloor?: boolean; floor?: number | null; now?: number; reason: string | null; pqRequired?: true };
const cases: Case[] = [
  { name: "transition: Ed25519-only chain, PQ pinned, nothing requires PQ (accepted, unchanged behaviour)", delegation: edOnly, pin: pinPq, reason: null },
  { name: "transition: Ed25519-only chain, no PQ pin at all", delegation: edOnly, pin: ed, reason: null },
  { name: "PQ co-signature present and valid, not yet required", delegation: goodPq, pin: pinPq, reason: null },
  { name: "PQ entry present but no PQ pin: ignored like any unknown key id", delegation: goodPq, pin: ed, reason: null },
  { name: "epoch delegation (pqRequired) with valid 2-of-3 Ed25519 and valid PQ", delegation: goodEpoch, pin: pinPq, reason: null, pqRequired: true },
  { name: "epoch delegation, 2-of-3 Ed25519 but NO PQ signature", delegation: edOnlyEpoch, pin: pinPq, reason: "pq_missing" },
  { name: "epoch delegation, PQ signature by the wrong PQ key under the pinned id", delegation: forged(edOnlyEpoch), pin: pinPq, reason: "pq_invalid" },
  { name: "epoch delegation, PQ signature filed under an unpinned key id", delegation: editPq(goodEpoch, (s) => ({ ...s, keyId: "pq-root-other" })), pin: pinPq, reason: "pq_missing" },
  { name: "epoch delegation edited after the PQ root signed it (Ed25519 re-signed, old PQ entry kept)", delegation: edited, pin: pinPq, reason: "pq_invalid" },
  { name: "epoch delegation edited after signing, Ed25519 not re-signed", delegation: { ...clone(goodEpoch), version: 9 }, pin: pinPq, reason: "root_threshold" },
  { name: "PQ valid but only 1-of-3 Ed25519: PQ never substitutes for the Ed25519 threshold", delegation: withPq(delegate({ roots: [R1], online: ONLINE, mutate: pqEpoch })), pin: pinPq, reason: "root_threshold" },
  { name: "PQ signature with one flipped byte", delegation: editPq(goodEpoch, (s) => ({ ...s, sig: flip(String(s["sig"])) })), pin: pinPq, reason: "pq_invalid" },
  { name: "PQ signature truncated", delegation: editPq(goodEpoch, (s) => ({ ...s, sig: String(s["sig"]).slice(0, 4000) })), pin: pinPq, reason: "pq_invalid" },
  { name: "PQ signature not base64", delegation: editPq(goodEpoch, (s) => ({ ...s, sig: "!!" })), pin: pinPq, reason: "pq_invalid" },
  { name: "wrong alg tag on the PQ entry (ml-dsa-87)", delegation: editPq(goodEpoch, (s) => ({ ...s, alg: "ml-dsa-87" })), pin: pinPq, reason: "pq_invalid" },
  { name: "wrong alg tag on the PQ entry (ed25519)", delegation: editPq(goodEpoch, (s) => ({ ...s, alg: "ed25519" })), pin: pinPq, reason: "pq_invalid" },
  { name: "unknown alg tag on the PQ entry (falcon-512)", delegation: editPq(goodEpoch, (s) => ({ ...s, alg: "falcon-512" })), pin: pinPq, reason: "pq_invalid" },
  { name: "PQ entry declares another frame", delegation: editPq(goodEpoch, (s) => ({ ...s, frame: "om-pq-v2" })), pin: pinPq, reason: "pq_invalid" },
  { name: "PQ entry declares no frame", delegation: editPq(goodEpoch, (s) => Object.fromEntries(Object.entries(s).filter(([k]) => k !== "frame"))), pin: pinPq, reason: "pq_invalid" },
  { name: "invalid PQ signature is refused even while PQ is not required", delegation: editPq(goodPq, (s) => ({ ...s, sig: flip(String(s["sig"])) })), pin: pinPq, reason: "pq_invalid" },
  { name: "persisted PQ floor: Ed25519-only chain refused", delegation: edOnly, pin: pinPq, pqFloor: true, reason: "pq_missing" },
  { name: "persisted PQ floor: PQ-signed ordinary delegation accepted", delegation: goodPq, pin: pinPq, pqFloor: true, reason: null, pqRequired: true },
  { name: "persisted PQ floor with no PQ pinned fails closed", delegation: edOnly, pin: ed, pqFloor: true, reason: "pq_root_unset" },
  { name: "build-level PQ requirement: Ed25519-only chain refused", delegation: edOnly, pin: { ...pinPq, pqRequired: true }, reason: "pq_missing" },
  { name: "build-level PQ requirement: PQ-signed chain accepted", delegation: goodPq, pin: { ...pinPq, pqRequired: true }, reason: null, pqRequired: true },
  { name: "epoch delegation but no PQ root pinned (pre-ceremony build) fails closed", delegation: goodEpoch, pin: ed, reason: "pq_root_unset" },
  { name: "pinned PQ descriptor with an unknown alg fails closed", delegation: edOnly, pin: { ...ed, pq: { ...PIN, alg: "ml-dsa-87" } }, reason: "pq_root_unset" },
  { name: "pinned PQ descriptor with an unknown frame fails closed", delegation: edOnly, pin: { ...ed, pq: { ...PIN, frame: "om-pq-v2" } }, reason: "pq_root_unset" },
  { name: "pinned PQ public key of the wrong size fails closed", delegation: edOnly, pin: { ...ed, pq: { ...PIN, pub: PIN.pub.slice(2) } }, reason: "pq_root_unset" },
  { name: "pinned PQ descriptor that is not an object fails closed", delegation: edOnly, pin: { ...ed, pq: "ml-dsa-65" }, reason: "pq_root_unset" },
  { name: "a broken Ed25519 pin is reported first (root_unset)", delegation: edOnly, pin: { keys: {}, threshold: 1, pq: { ...PIN, alg: "x" } }, reason: "root_unset" },
  { name: "pqRequired: false is malformed", delegation: { ...clone(edOnly), pqRequired: false }, pin: pinPq, reason: "malformed_delegation" },
  { name: "pqRequired: 1 is malformed", delegation: { ...clone(edOnly), pqRequired: 1 }, pin: pinPq, reason: "malformed_delegation" },
  { name: "pqRequired: \"true\" is malformed", delegation: { ...clone(edOnly), pqRequired: "true" }, pin: pinPq, reason: "malformed_delegation" },
  { name: "downgrade: the epoch marker stripped from a signed delegation breaks the Ed25519 signatures", delegation: (() => { const d = clone(goodEpoch); delete d["pqRequired"]; return d; })(), pin: pinPq, reason: "root_threshold" },
  { name: "rollback below the delegation floor is still reported as delegation_rollback", delegation: goodEpoch, pin: pinPq, floor: 2, reason: "delegation_rollback" },
  { name: "expiry is still reported before any PQ check", delegation: edOnlyEpoch, pin: pinPq, now: NOW + 91 * 86_400_000, reason: "delegation_expired" },
];

describe("TS verifier and verify-release.mjs agree on every PQ case (shared fixture)", () => {
  it.each(cases)("$name", async (c) => {
    const m = await manifestOf(c.delegation);
    const now = c.now ?? NOW;
    const floor = c.floor ?? null;
    const ts = await verifyBundleSignature(m, { roots: c.pin as never, now: () => now }, floor, c.pqFloor ?? false);
    const js = verifyManifestSignature(m, c.pin as never, { now, floor, pqFloor: c.pqFloor ?? false });
    if (c.reason !== null) {
      expect(ts).toEqual({ ok: false, reason: c.reason });
      expect(js).toEqual({ ok: false, reason: c.reason });
    } else {
      expect(ts).toEqual({ ok: true, unsignedDev: false, delegationVersion: 1, ...(c.pqRequired ? { pqRequired: true } : {}) });
      expect(js).toEqual({ ok: true, delegationVersion: 1, ...(c.pqRequired ? { pqRequired: true } : {}) });
    }
    const chain = await verifyChain(m.delegation, c.pin as never, { now, floor, pqFloor: c.pqFloor ?? false }, "online-1", "ed25519");
    if (c.reason !== null) expect(chain).toEqual({ ok: false, reason: c.reason });
    else expect(chain).toMatchObject({ ok: true });
  });
});

describe("signed bytes: framing and domain separation", () => {
  const FAKE_PUB = Buffer.from(Array.from({ length: PQ_PUBLIC_KEY_BYTES }, (_, i) => (i * 7 + 3) % 256)).toString("hex");
  const FAKE_PIN = { keyId: "pq-root-fake", alg: PQ_ALG, pub: FAKE_PUB, frame: PQ_FRAME };
  const PAYLOAD = Buffer.from('{"format":"om.release-delegation/1"}');
  // Independent oracle: built by a separate Python implementation (hashlib) from the layout in ADR-0182 section 2.
  const GOLDEN_HEX =
    "6f6d2e72656c656173652d64656c65676174696f6e2e70712f31006d6c2d6473612d36350095b953982194adb7b5c29489011daa76e23eaace5e7134089124808cf97e8b007b22666f726d6174223a226f6d2e72656c656173652d64656c65676174696f6e2f31227d";
  it("both implementations produce the golden message byte for byte", async () => {
    expect(Buffer.from(await pqSigningBytesTs(FAKE_PIN, PAYLOAD)).toString("hex")).toBe(GOLDEN_HEX);
    expect(pqSigningBytesMjs(FAKE_PIN, PAYLOAD).toString("hex")).toBe(GOLDEN_HEX);
  });
  it("the message is: label, 00, alg tag, 00, SHA-256 of the pinned public key, payload", () => {
    const b = Buffer.from(GOLDEN_HEX, "hex");
    expect(b.subarray(0, 26).toString("ascii")).toBe("om.release-delegation.pq/1");
    expect(b[26]).toBe(0);
    expect(b.subarray(27, 36).toString("ascii")).toBe("ml-dsa-65");
    expect(b[36]).toBe(0);
    expect(b.subarray(37, 69).toString("hex")).toBe(createHash("sha256").update(Buffer.from(FAKE_PUB, "hex")).digest("hex"));
    expect(b.subarray(69).equals(PAYLOAD)).toBe(true);
  });
  it("binds the key, the algorithm tag and the payload; the Ed25519 frames cannot collide with it", async () => {
    const base = Buffer.from(await pqSigningBytesTs(FAKE_PIN, PAYLOAD)).toString("hex");
    expect(Buffer.from(await pqSigningBytesTs({ ...FAKE_PIN, pub: "00".repeat(PQ_PUBLIC_KEY_BYTES) }, PAYLOAD)).toString("hex")).not.toBe(base);
    expect(Buffer.from(await pqSigningBytesTs({ ...FAKE_PIN, alg: "ml-dsa-87" }, PAYLOAD)).toString("hex")).not.toBe(base);
    expect(Buffer.from(await pqSigningBytesTs(FAKE_PIN, Buffer.from("{}"))).toString("hex")).not.toBe(base);
    // the Ledger frame starts with 0xff and the raw frame with "{", never with the PQ label
    expect(base.startsWith("6f6d2e72")).toBe(true);
  });
  it("a PQ signature over the bare payload (no frame), or over another key's frame, never verifies", async () => {
    const d = clone(edOnlyEpoch) as unknown as Delegation;
    const payload = Buffer.from(await frameDelegationPayload("raw", d, "00".repeat(32)) as Uint8Array);
    const bare = Buffer.from(ml_dsa65.sign(payload, PQ.secretKey)).toString("base64");
    expect(await verifyPqSignature(PIN, payload, bare)).toBe(false);
    const otherFrame = Buffer.from(ml_dsa65.sign(await pqSigningBytesTs(pqDescriptor(PQ_OTHER.pub), payload), PQ.secretKey)).toString("base64");
    expect(await verifyPqSignature(PIN, payload, otherFrame)).toBe(false);
    const right = Buffer.from(ml_dsa65.sign(await pqSigningBytesTs(PIN, payload), PQ.secretKey)).toString("base64");
    expect(await verifyPqSignature(PIN, payload, right)).toBe(true);
  });
  it("sizes: public key 1952 bytes, signature 3309 bytes (4412 base64 characters)", () => {
    const e = (goodEpoch["signatures"] as Rec[]).find((s) => s["keyId"] === PIN.keyId) as Rec;
    expect(PIN.pub.length).toBe(PQ_PUBLIC_KEY_BYTES * 2);
    expect(Buffer.from(String(e["sig"]), "base64").length).toBe(PQ_SIGNATURE_BYTES);
    expect(String(e["sig"]).length).toBe(4412);
    expect(e).toMatchObject({ alg: "ml-dsa-65", frame: "om-pq-v1", keyId: pqKeyId(PQ.pub) });
  });
  it("a signature made by node:crypto (OpenSSL) from the same seed verifies in the TS verifier, and vice versa", async () => {
    const OID = "0609608648016503040312";
    const pkcs8 = Buffer.concat([Buffer.from(`3034020100300b${OID}04228020`, "hex"), Buffer.from(PQ.seed)]);
    const priv = createPrivateKey({ key: pkcs8, format: "der", type: "pkcs8" });
    const payload = Buffer.from("interop payload");
    const msg = await pqSigningBytesTs(PIN, payload);
    expect(await verifyPqSignature(PIN, payload, nodeSign(null, msg, priv).toString("base64"))).toBe(true);
    const fromNoble = Buffer.from(ml_dsa65.sign(msg, PQ.secretKey)).toString("base64");
    expect(await verifyPqSignature(PIN, payload, fromNoble)).toBe(true);
  });
  it("ledgerText shows the epoch in words on the device, only for a pqRequired delegation (both implementations)", () => {
    const d = clone(edOnly) as unknown as Delegation;
    const e = clone(edOnlyEpoch) as unknown as Delegation;
    const h = "a".repeat(64);
    expect(ledgerText(d, h)).toBe(ledgerTextMjs(d as unknown as MjsDelegation, h));
    expect(ledgerText(d, h)).not.toContain("pq");
    expect(ledgerText(e, h)).toBe(ledgerTextMjs(e as unknown as MjsDelegation, h));
    expect(ledgerText(e, h)).toMatch(/^OM release delegation \| version 1 \| expires \S+ \| pq required \| sha256 a{64}$/);
  });
});

describe("pins", () => {
  it("the client pin and verify-release.mjs pin agree (both unset until the PQ ceremony)", () => {
    expect(PINNED_PQ_ROOT).toEqual(PQ_ROOT);
    expect(PINNED_PQ_REQUIRED).toBe(PQ_REQUIRED_BY_BUILD);
    if (PQ_ROOT !== null) {
      expect(describePq(PQ_ROOT)).toEqual(PQ_ROOT);
      expect(checkDescriptor(PQ_ROOT)).toEqual(PQ_ROOT);
    }
  });
});

describe("persisted PQ floor (no downgrade after the epoch)", () => {
  const fake = (): FloorStore & { v: string | null; d: number | null; pq: boolean } => {
    const s = {
      v: null as string | null, d: null as number | null, pq: false,
      get: () => Promise.resolve(s.v), set: (x: string) => { s.v = x; return Promise.resolve(); },
      getDelegation: () => Promise.resolve(s.d), setDelegation: (x: number) => { s.d = x; return Promise.resolve(); },
      getPqRequired: () => Promise.resolve(s.pq), setPqRequired: () => { s.pq = true; return Promise.resolve(); },
    };
    return s;
  };
  const trust = { roots: pinPq as never, now: () => NOW };
  it("is raised only by a fully verified epoch delegation, and after that an Ed25519-only chain is refused forever", async () => {
    const floor = fake();
    // transition: an Ed25519-only chain is accepted and does not raise the floor
    const a = await evaluateManifest(await manifestOf(edOnly), floor, trust);
    expect(a).toEqual({ ok: true, unsignedDev: false, rolledBack: false, delegationVersion: 1 });
    if (a.ok) await commitFloor(floor, a, "1.0.0");
    expect(floor.pq).toBe(false);
    // evaluating alone never persists
    const e = await evaluateManifest(await manifestOf(goodEpoch, "1.0.1"), floor, trust);
    expect(e).toMatchObject({ ok: true, pqRequired: true });
    expect(floor.pq).toBe(false);
    if (e.ok) await commitFloor(floor, e, "1.0.1");
    expect(floor.pq).toBe(true);
    // a root-signed but Ed25519-only delegation, even at a higher version, is now refused; PQ-signed ones are accepted
    const m = (d: Rec) => manifestOf(d, "1.0.2");
    const noPq = delegate({ roots: [R1, R2], online: ONLINE, version: 5 });
    expect(await evaluateManifest(await m(noPq), floor, trust)).toEqual({ ok: false, reason: "pq_missing" });
    expect(await evaluateManifest(await m(withPq(noPq)), floor, trust)).toMatchObject({ ok: true, pqRequired: true });
    // the floor has no way down: committing an unrelated accepted manifest leaves it set
    expect(floor.pq).toBe(true);
  });
  it("a PQ-required delegation is refused by a build with no PQ root pinned, and the floor is not raised", async () => {
    const floor = fake();
    const r = await evaluateManifest(await manifestOf(goodEpoch), floor, { roots: ed, now: () => NOW });
    expect(r).toEqual({ ok: false, reason: "pq_root_unset" });
    expect(floor.pq).toBe(false);
  });
});

describe("service worker: the PQ floor survives in storage and refuses a later Ed25519-only update", () => {
  const memCache = (): ShellCache & { m: Map<string, string> } => {
    const m = new Map<string, string>();
    return { m, put: async (p, r) => void m.set(p, await r.text()), get: (p) => Promise.resolve(m.has(p) ? new Response(m.get(p)) : undefined), paths: () => Promise.resolve([...m.keys()]) };
  };
  it("stage() with an epoch delegation writes pq-required-floor; a second shell over the same storage refuses an Ed25519-only chain", async () => {
    const caches = new Map<string, ReturnType<typeof memCache>>();
    const kvm = new Map<string, string>();
    const net = new Map<string, string>();
    const storage: ShellStorage = {
      open: (n) => Promise.resolve(caches.get(n) ?? (caches.set(n, memCache()), caches.get(n) as ReturnType<typeof memCache>)),
      delete: (n) => Promise.resolve(void caches.delete(n)),
      names: () => Promise.resolve([...caches.keys()]),
    };
    const kv: KV = { get: (k) => Promise.resolve(kvm.get(k) ?? null), set: (k, v) => Promise.resolve(void kvm.set(k, v)) };
    const fetchFn = (p: string): Promise<Response> => Promise.resolve(net.has(p) ? new Response(net.get(p)) : new Response("", { status: 404 }));
    const publish = async (version: string, delegation: unknown) => {
      net.clear();
      const files = { "index.html": "<html>", "sw.js": "sw" };
      const m = await buildManifest({ version, files: Object.entries(files).map(([path, d]) => ({ path, data: new TextEncoder().encode(d) })) });
      net.set("bundle-manifest.json", serializeManifest(signWith(m, delegation, ONLINE)));
      for (const [p, d] of Object.entries(files)) net.set(p, d);
    };
    const shell = () => createShell({ storage, kv, fetchFn, trustRoot: { roots: pinPq as never, now: () => NOW } });
    await publish("1.0.0", edOnly);
    await shell().stage();
    expect(kvm.has("pq-required-floor")).toBe(false);
    await publish("1.1.0", goodEpoch);
    await shell().stage();
    expect(kvm.get("pq-required-floor")).toBe("1");
    await publish("1.2.0", delegate({ roots: [R1, R2], online: ONLINE, version: 4 }));
    await expect(shell().stage()).rejects.toThrow(/pq_missing/);
    await publish("1.2.0", withPq(delegate({ roots: [R1, R2], online: ONLINE, version: 4 })));
    await expect(shell().stage()).resolves.toMatchObject({ version: "1.2.0" });
    kvm.set("pq-required-floor", "garbage"); // a corrupt value still counts as required
    await publish("1.3.0", delegate({ roots: [R1, R2], online: ONLINE, version: 4 }));
    await expect(shell().stage()).rejects.toThrow(/pq_missing/);
  });
});

describe("signing tool: requireDelegation is stricter than a client once a PQ root is pinned", () => {
  const trust = { roots: ed.keys, threshold: 2, now: NOW };
  it("refuses an Ed25519-only delegation when a PQ root is pinned, accepts the PQ-signed one; unchanged without a PQ pin", () => {
    expect(() => requireDelegation(edOnly, ONLINE.pem, "online-1", trust)).not.toThrow();
    expect(() => requireDelegation(edOnly, ONLINE.pem, "online-1", { ...trust, pq: PIN })).toThrow(/delegation refused: pq_missing/);
    expect(() => requireDelegation(goodPq, ONLINE.pem, "online-1", { ...trust, pq: PIN })).not.toThrow();
    expect(() => requireDelegation(editPq(goodPq, (s) => ({ ...s, sig: flip(String(s["sig"])) })), ONLINE.pem, "online-1", { ...trust, pq: PIN })).toThrow(/pq_invalid/);
  });
});

describe("ceremony: PQ commands", () => {
  const vaultItem = () => {
    const r = ceremony("gen-pq-root", "--to-stdin-for-vault", "--name", "OM test PQ root", "--pub-out", join(tmp, "pq-pub.json"));
    expect(r.status, r.stderr).toBe(0);
    return { r, item: JSON.parse(r.stdout) as { type: number; name: string; notes: string; secureNote: { type: number } } };
  };
  it("gen-pq-root puts the seed only in a Bitwarden item on stdout (no file), prints only public data, writes the public descriptor", () => {
    const { r, item } = vaultItem();
    expect(item).toMatchObject({ type: 2, name: "OM test PQ root", secureNote: { type: 0 } });
    expect(item.notes).toMatch(/^-----BEGIN OM PQ ROOT SEED-----\nalg: ml-dsa-65\nframe: om-pq-v1\nkey-id: pq-root-[0-9a-f]{16}\nfingerprint: sha256 [0-9a-f]{64}\nseed: [0-9a-f]{64}\n-----END OM PQ ROOT SEED-----\n$/);
    const seed = /seed: ([0-9a-f]{64})/.exec(item.notes)?.[1] ?? "";
    expect(r.stderr).not.toContain(seed);
    expect(r.stderr).toMatch(/pq root key id: pq-root-[0-9a-f]{16}/);
    expect(r.stderr).toMatch(/fingerprint:\s+sha256 [0-9a-f ]+/);
    const pub = JSON.parse(readFileSync(join(tmp, "pq-pub.json"), "utf8")) as Rec;
    expect(checkDescriptor(pub)).toEqual(pub);
    expect(readFileSync(join(tmp, "pq-pub.json"), "utf8")).not.toContain(seed);
    expect(ceremony("gen-pq-root", "--out", join(tmp, "x")).stderr).toMatch(/only runs with --to-stdin-for-vault/);
    expect(ceremony("gen-pq-root", "--to-stdin-for-vault", "--out", join(tmp, "x")).stderr).toMatch(/never writes a key file/);
    expect(existsSync(join(tmp, "x"))).toBe(false);
  });
  it("show-pq-root prints the same descriptor and the paste-ready pins; garbage and damaged copies are refused without echo", () => {
    const { item } = vaultItem();
    const shown = ceremonyIn(item.notes, "show-pq-root", "--pq-root-stdin");
    expect(shown.status, shown.stderr).toBe(0);
    const desc = JSON.parse(readFileSync(join(tmp, "pq-pub.json"), "utf8")) as Rec;
    expect(shown.stdout).toContain(String(desc["pub"]));
    expect(shown.stdout).toMatch(/export const PQ_ROOT: PqRootPin \| null = \{/);
    expect(shown.stdout).toMatch(/export const PINNED_PQ_ROOT = \{/);
    expect(shown.stdout + shown.stderr).not.toMatch(/seed:/);
    const bad = ceremonyIn("SECRET-LOOKING-TEXT", "show-pq-root", "--pq-root-stdin");
    expect(bad.status).toBe(1);
    expect(bad.stderr).toMatch(/not an OM PQ root seed note/);
    expect(bad.stderr + bad.stdout).not.toContain("SECRET-LOOKING-TEXT");
    const flipped = item.notes.replace(/seed: ([0-9a-f])/, (_m, c: string) => `seed: ${c === "0" ? "1" : "0"}`);
    const damaged = ceremonyIn(flipped, "show-pq-root", "--pq-root-stdin");
    expect(damaged.status).toBe(1);
    expect(damaged.stderr).toMatch(/fingerprint does not match its seed/);
    expect(() => loadPqRootNote(item.notes.replace("ml-dsa-65", "ml-dsa-87"))).toThrow(/another algorithm/);
  });
  it("full flow: Ed25519 2-of-3, pq-sign-delegation from stdin, verify; then the epoch with --pq-required; strict verify fails without PQ", () => {
    const { item } = vaultItem();
    const pub = JSON.parse(readFileSync(join(tmp, "pq-pub.json"), "utf8")) as { keyId: string; pub: string };
    const roots = { roots: ed.keys, threshold: 2, pq: pub };
    const rootsFile = join(tmp, "roots-pq.json");
    writeFileSync(rootsFile, JSON.stringify(roots));
    const rootsNoPq = join(tmp, "roots-nopq.json");
    writeFileSync(rootsNoPq, JSON.stringify({ roots: ed.keys, threshold: 2 }));
    const d = join(tmp, "pq-flow.json");
    expect(ceremony("draft", "--online-pub", ONLINE.pub, "--key-id", "om-release-test", "--expires", "+90d", "--out", d).status).toBe(0);
    let doc = JSON.parse(readFileSync(d, "utf8")) as Rec;
    expect(doc).not.toHaveProperty("pqRequired"); // a plain draft is byte-for-byte the old shape
    writeFileSync(d, JSON.stringify(addRootSignature(addRootSignature(doc, R1), R2)));
    const verify = (file: string, rf: string, ...x: string[]) => ceremony("verify", "--in", file, "--roots-file", rf, ...x);
    expect(verify(d, rootsNoPq).stdout).toMatch(/OK: 2-of-3/);
    expect(verify(d, rootsFile).stdout).toMatch(/NO PQ co-signature \(Ed25519-only chain, allowed until a pqRequired delegation\)/);
    expect(verify(d, rootsFile, "--require-pq").stdout).toMatch(/FAILED: pq_missing/);
    // attach the PQ co-signature after the Ed25519 signatures exist; they stay valid (the PQ entry is not part of the signed bytes)
    const edSigsBefore = JSON.stringify((JSON.parse(readFileSync(d, "utf8")) as { signatures: Rec[] }).signatures.map((s) => s["sig"]));
    const signed = ceremonyIn(item.notes, "pq-sign-delegation", "--pq-root-stdin", "--in", d, "--roots-file", rootsFile);
    expect(signed.status, signed.stderr).toBe(0);
    expect(signed.stdout + signed.stderr).not.toMatch(/seed:/);
    doc = JSON.parse(readFileSync(d, "utf8")) as Rec;
    expect(sigsOf(doc).map((s) => s["keyId"])).toContain(pub.keyId);
    expect(sigsOf(doc).filter((s) => s["alg"] === "ed25519").map((s) => s["sig"])).toEqual(JSON.parse(edSigsBefore) as string[]);
    expect(verify(d, rootsFile, "--require-pq").stdout).toMatch(/OK: 2-of-3.*PQ co-signature pq-root-[0-9a-f]{16} verifies/);
    expect(verify(d, rootsNoPq).stdout).toMatch(/OK: 2-of-3/); // a verifier without the PQ pin ignores the entry
    // the epoch: sticky across renewals
    const e = join(tmp, "pq-epoch.json");
    expect(ceremony("draft", "--prev", d, "--pq-required", "--expires", "+90d", "--out", e).status).toBe(0);
    expect(JSON.parse(readFileSync(e, "utf8"))).toMatchObject({ version: 2, pqRequired: true, signatures: [] });
    const renew = join(tmp, "pq-renew.json");
    expect(ceremony("draft", "--prev", e, "--expires", "+90d", "--out", renew).status).toBe(0);
    expect(JSON.parse(readFileSync(renew, "utf8"))).toMatchObject({ version: 3, pqRequired: true });
    expect(ceremony("inspect", "--in", renew).stdout).toMatch(/pq required: yes/);
    writeFileSync(renew, JSON.stringify(addRootSignature(addRootSignature(JSON.parse(readFileSync(renew, "utf8")), R1), R2)));
    expect(verify(renew, rootsFile).stdout).toMatch(/FAILED: pq_missing/); // the epoch needs the PQ signature even without --require-pq
    expect(ceremonyIn(item.notes, "pq-sign-delegation", "--pq-root-stdin", "--in", renew).status).toBe(0);
    expect(verify(renew, rootsFile).stdout).toMatch(/OK: 2-of-3.*pqRequired.*PQ co-signature .* verifies/);
    // a different PQ key cannot satisfy the pin
    const other = ceremony("gen-pq-root", "--to-stdin-for-vault");
    const wrong = join(tmp, "pq-wrong.json");
    writeFileSync(wrong, JSON.stringify(addRootSignature(addRootSignature(JSON.parse(readFileSync(e, "utf8")), R1), R2)));
    expect(ceremonyIn((JSON.parse(other.stdout) as { notes: string }).notes, "pq-sign-delegation", "--pq-root-stdin", "--in", wrong).status).toBe(0);
    expect(verify(wrong, rootsFile).stdout).toMatch(/FAILED: pq_missing/);
  });
  it("external PQ signer path: pq-payload writes the exact bytes, pq-attach verifies before writing", () => {
    const { item } = vaultItem();
    const note = loadPqRootNote(item.notes);
    const descFile = join(tmp, "pq-pub.json");
    const d = join(tmp, "pq-external.json");
    expect(ceremony("draft", "--online-pub", ONLINE.pub, "--key-id", "om-release-test", "--expires", "+90d", "--pq-required", "--out", d).status).toBe(0);
    const payloadFile = join(tmp, "pq-payload.bin");
    const p = ceremony("pq-payload", "--in", d, "--pq-root-file", descFile, "--out", payloadFile);
    expect(p.status, p.stderr).toBe(0);
    const bytes = readFileSync(payloadFile);
    expect(p.stdout).toContain(`sha256 ${createHash("sha256").update(bytes).digest("hex")}`);
    expect(p.stdout).toMatch(/pq required: yes/);
    const sig = Buffer.from(ml_dsa65.sign(bytes, note.secretKey));
    const before = readFileSync(d, "utf8");
    const wrongKey = join(tmp, "pq-wrong-desc.json");
    writeFileSync(wrongKey, JSON.stringify(pqDescriptor(PQ_OTHER.pub)));
    expect(ceremony("pq-attach", "--in", d, "--pq-root-file", wrongKey, "--sig", sig.toString("hex")).status).toBe(1);
    expect(ceremony("pq-attach", "--in", d, "--pq-root-file", descFile, "--sig", flip(sig.toString("base64"))).stderr).toMatch(/does not verify/);
    expect(ceremony("pq-attach", "--in", d, "--pq-root-file", descFile, "--sig", "abcd").stderr).toMatch(/3309-byte/);
    expect(readFileSync(d, "utf8")).toBe(before);
    const ok = ceremony("pq-attach", "--in", d, "--pq-root-file", descFile, "--sig", sig.toString("hex"));
    expect(ok.status, ok.stderr).toBe(0);
    expect(JSON.parse(readFileSync(d, "utf8")) as Rec).toMatchObject({ pqRequired: true, signatures: [{ keyId: pqKeyId(note.pub), alg: "ml-dsa-65", frame: "om-pq-v1" }] });
  });
  it("the vault note round-trips, and buildDelegation without PQ options keeps the exact old shape", () => {
    const r = generatePqRoot();
    expect(loadPqRootNote(formatPqNote(r)).pub).toBe(r.pub);
    const d = buildDelegation({ expires: "+10d", addKey: { keyId: "k", pub: ONLINE.pub }, now: NOW });
    expect(Object.keys(d)).toEqual(["format", "version", "expires", "keys", "revoked", "signatures"]);
  });
});

describe("release flow with a PQ root (throwaway roots; a full vite build)", () => {
  const webRoot = join(import.meta.dirname, "..");
  it("release.mjs signs under a PQ-signed delegation, and the whole chain verifies only with the right PQ pin", { timeout: 180_000 }, () => {
    const real = Date.now();
    const keyFile = join(tmp, "release-online.pem");
    writeFileSync(keyFile, ONLINE.pem);
    const deleg = withPq(delegate({ roots: [R1, R2], online: ONLINE, keyId: "online-1", now: real }));
    const dfile = join(tmp, "release-delegation.json");
    writeFileSync(dfile, JSON.stringify(deleg));
    const rootsFile = join(tmp, "release-roots.json");
    writeFileSync(rootsFile, JSON.stringify({ roots: ed.keys, threshold: 2, pq: PIN }));
    const out = join(tmp, "release-out");
    const r = spawnSync("node", ["build/release.mjs", "--no-zip", "--out", out, "--sign-with-key-file", keyFile, "--key-id", "online-1", "--delegation", dfile, "--test-roots-file", rootsFile], { cwd: webRoot, encoding: "utf8" });
    expect(r.status, r.stderr).toBe(0);
    const dir = join(out, readdirNames(out)[0] ?? "");
    const opts = { requireSigned: true, roots: ed.keys, threshold: 2, now: real };
    expect(verifyRelease(dir, { ...opts, pq: PIN, pqRequired: true }).errors).toEqual([]);
    expect(verifyRelease(dir, { ...opts, pq: pqDescriptor(PQ_OTHER.pub), pqRequired: true }).errors).toEqual(["signature: pq_missing"]);
    expect(verifyRelease(dir, opts).errors).toEqual([]); // a verifier without a PQ pin ignores the entry (transition)
    expect(verifyRelease(dir, { ...opts, pqRequired: true }).errors).toEqual(["signature: pq_root_unset"]);
  });
  it("release.mjs refuses an Ed25519-only delegation once the PQ root is pinned in the roots file", { timeout: 180_000 }, () => {
    const real = Date.now();
    const keyFile = join(tmp, "release-online2.pem");
    writeFileSync(keyFile, ONLINE.pem);
    const dfile = join(tmp, "release-delegation-ed.json");
    writeFileSync(dfile, JSON.stringify(delegate({ roots: [R1, R2], online: ONLINE, keyId: "online-1", now: real })));
    const rootsFile = join(tmp, "release-roots2.json");
    writeFileSync(rootsFile, JSON.stringify({ roots: ed.keys, threshold: 2, pq: PIN }));
    const r = spawnSync("node", ["build/release.mjs", "--no-zip", "--out", join(tmp, "release-out2"), "--sign-with-key-file", keyFile, "--key-id", "online-1", "--delegation", dfile, "--test-roots-file", rootsFile], { cwd: webRoot, encoding: "utf8" });
    expect(r.status).toBe(1);
    expect(r.stderr).toMatch(/signing failed: delegation refused: pq_missing/);
    expect(existsSync(join(tmp, "release-out2"))).toBe(false);
  });
});
