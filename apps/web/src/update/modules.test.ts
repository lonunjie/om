import { describe, expect, it } from "vitest";
import { sha256Hex } from "./hash";
import { buildManifest } from "../../build/manifest";
import { checkSameRelease, isLazyPath, loadVerifiedModule, parseModuleTable, type ModuleSource, type ModuleTable } from "./modules";
import { parseManifest, type BundleManifest } from "./manifest";

const enc = (s: string): Uint8Array => new TextEncoder().encode(s);
const CORE = "assets/om_wasm-AbC123_x.wasm";
const GROUPS = "assets/om_wasm_groups-Zy9-8.wasm";
const coreBytes = enc("base core");
const groupsBytes = enc("groups core, bigger");
const ab = (u: Uint8Array): ArrayBuffer => u.buffer.slice(u.byteOffset, u.byteOffset + u.byteLength) as ArrayBuffer;

async function fixture(): Promise<{ table: ModuleTable; manifest: BundleManifest }> {
  const manifest = await buildManifest({ version: "1.2.3", files: [
    { path: "index.html", data: enc("<html>") }, { path: "sw.js", data: enc("sw") }, { path: CORE, data: coreBytes }, { path: GROUPS, data: groupsBytes },
  ] });
  const table: ModuleTable = {
    version: "1.2.3",
    core: { sha256: await sha256Hex(coreBytes), bytes: coreBytes.byteLength },
    groups: { sha256: await sha256Hex(groupsBytes), bytes: groupsBytes.byteLength },
  };
  return { table, manifest };
}
const response = (body: Uint8Array | string | null) => body === null ? { ok: false, text: () => Promise.reject(new Error("x")), arrayBuffer: () => Promise.reject(new Error("x")) }
  : { ok: true, text: () => Promise.resolve(typeof body === "string" ? body : new TextDecoder().decode(body)), arrayBuffer: () => Promise.resolve(ab(typeof body === "string" ? enc(body) : body)) };
const served = (files: Record<string, Uint8Array | string | null>, log: string[] = [], kept?: ModuleSource["kept"]): ModuleSource => ({
  fetch: (p) => { log.push(p); const b = files[p]; return b === undefined ? Promise.reject(new Error("offline")) : Promise.resolve(response(b)); },
  ...(kept === undefined ? {} : { kept }),
});
const manifestText = (m: BundleManifest): string => JSON.stringify(m);

describe("lazy marking and the module table", () => {
  it("marks only the groups module lazy in the generated manifest and the manifest parses", async () => {
    const { manifest } = await fixture();
    expect(manifest.files.filter((f) => f.lazy === true).map((f) => f.path)).toEqual([GROUPS]);
    expect(isLazyPath(CORE)).toBe(false);
    expect(isLazyPath("assets/om_wasm_groups-x.js")).toBe(false);
    expect(() => parseManifest(manifestText(manifest))).not.toThrow();
    expect(() => parseManifest(JSON.stringify({ ...manifest, files: manifest.files.map((f) => ({ ...f, lazy: false })) }))).toThrow();
  });
  it("refuses a lazy flag on any file but the groups module (an unverified skip of the install is never granted)", async () => {
    const { manifest } = await fixture();
    const files = manifest.files.map((f) => (f.path === CORE ? { ...f, lazy: true } : f));
    expect(() => parseManifest(JSON.stringify({ ...manifest, files }))).toThrow(/lazy/);
  });
  it("refuses a missing or malformed table", async () => {
    const { table } = await fixture();
    expect(parseModuleTable(table)).toEqual(table);
    for (const bad of [null, undefined, 1, {}, { ...table, version: "" }, { ...table, core: { sha256: "xx", bytes: 1 } }, { ...table, groups: { sha256: table.groups.sha256, bytes: 0 } }]) expect(parseModuleTable(bad)).toBeNull();
  });
});

describe("version skew between the base Core and the groups module", () => {
  it("accepts one release", async () => {
    const { table, manifest } = await fixture();
    expect(checkSameRelease(table, manifest, "groups", GROUPS)).toEqual({ ok: true, value: undefined });
    expect(checkSameRelease(table, manifest, "core", CORE).ok).toBe(true);
  });
  it("refuses another release's manifest, a different digest for either module, and a module that is not listed", async () => {
    const { table, manifest } = await fixture();
    expect(checkSameRelease(table, { ...manifest, version: "1.2.4" }, "groups", GROUPS)).toEqual({ ok: false, reason: "version_skew" });
    const swap = (path: string): BundleManifest => ({ ...manifest, files: manifest.files.map((f) => (f.path === path ? { ...f, sha256: "0".repeat(64) } : f)) });
    expect(checkSameRelease(table, swap(GROUPS), "groups", GROUPS)).toEqual({ ok: false, reason: "digest_skew" });
    // The base Core of another release: the groups module alone matching is not enough.
    expect(checkSameRelease(table, swap(CORE), "groups", GROUPS)).toEqual({ ok: false, reason: "digest_skew" });
    expect(checkSameRelease(table, { ...manifest, files: manifest.files.filter((f) => f.path !== GROUPS) }, "groups", GROUPS)).toEqual({ ok: false, reason: "digest_skew" });
    expect(checkSameRelease(table, manifest, "groups", "assets/other.wasm")).toEqual({ ok: false, reason: "digest_skew" });
  });
});

describe("loadVerifiedModule (the digest check before anything is instantiated)", () => {
  it("returns the bytes of a module that matches the table and the manifest, and keeps them", async () => {
    const { table, manifest } = await fixture();
    const kept = new Map<string, ArrayBuffer>();
    const src = served({ "bundle-manifest.json": manifestText(manifest), [GROUPS]: groupsBytes }, [], { get: (k) => Promise.resolve(kept.get(k)), put: (k, b) => { kept.set(k, b); return Promise.resolve(); } });
    const r = await loadVerifiedModule(table, "groups", GROUPS, src, { release: true });
    expect(r.ok && new Uint8Array(r.value)).toEqual(groupsBytes);
    expect(kept.has(table.groups.sha256)).toBe(true);
  });
  it("refuses bytes with another digest or length and keeps nothing", async () => {
    const { table, manifest } = await fixture();
    const kept = new Map<string, ArrayBuffer>();
    const keep = { get: (k: string) => Promise.resolve(kept.get(k)), put: (k: string, b: ArrayBuffer) => { kept.set(k, b); return Promise.resolve(); } };
    const flipped = groupsBytes.slice(); flipped[0] = (flipped[0] ?? 0) ^ 1;
    expect(await loadVerifiedModule(table, "groups", GROUPS, served({ "bundle-manifest.json": manifestText(manifest), [GROUPS]: flipped }, [], keep), { release: true })).toEqual({ ok: false, reason: "digest_mismatch" });
    expect(await loadVerifiedModule(table, "groups", GROUPS, served({ "bundle-manifest.json": manifestText(manifest), [GROUPS]: enc("short") }, [], keep), { release: true })).toEqual({ ok: false, reason: "size_mismatch" });
    expect(kept.size).toBe(0);
  });
  it("refuses a kept copy that no longer matches, and fetches again", async () => {
    const { table, manifest } = await fixture();
    const kept = { get: () => Promise.resolve(ab(enc("tampered copy!"))), put: () => Promise.resolve() };
    const log: string[] = [];
    const r = await loadVerifiedModule(table, "groups", GROUPS, served({ "bundle-manifest.json": manifestText(manifest), [GROUPS]: groupsBytes }, log, kept), { release: true });
    expect(r.ok).toBe(true);
    expect(log).toContain(GROUPS);
  });
  it("uses a verified kept copy with no network (offline start)", async () => {
    const { table } = await fixture();
    const log: string[] = [];
    const r = await loadVerifiedModule(table, "groups", GROUPS, served({}, log, { get: () => Promise.resolve(ab(groupsBytes)), put: () => Promise.resolve() }), { release: true });
    expect(r.ok).toBe(true);
    expect(log).toEqual([]);
  });
  it("fails closed: no table, no manifest, a skewed manifest, an unreachable module", async () => {
    const { table, manifest } = await fixture();
    expect(await loadVerifiedModule(null, "groups", GROUPS, served({}), { release: true })).toEqual({ ok: false, reason: "table_missing" });
    expect(await loadVerifiedModule(table, "groups", GROUPS, served({}), { release: true })).toEqual({ ok: false, reason: "manifest_unavailable" });
    expect(await loadVerifiedModule(table, "groups", GROUPS, served({ "bundle-manifest.json": "not json" }), { release: true })).toEqual({ ok: false, reason: "manifest_unavailable" });
    expect(await loadVerifiedModule(table, "groups", GROUPS, served({ "bundle-manifest.json": manifestText({ ...manifest, version: "9.9.9" }), [GROUPS]: groupsBytes }), { release: true })).toEqual({ ok: false, reason: "version_skew" });
    expect(await loadVerifiedModule(table, "groups", GROUPS, served({ "bundle-manifest.json": manifestText(manifest) }), { release: true })).toEqual({ ok: false, reason: "unavailable" });
    expect(await loadVerifiedModule(table, "groups", GROUPS, served({ "bundle-manifest.json": manifestText(manifest), [GROUPS]: null }), { release: true })).toEqual({ ok: false, reason: "unavailable" });
  });
  it("the base Core is checked against the table alone, with no manifest request (it starts offline)", async () => {
    const { table } = await fixture();
    const log: string[] = [];
    const r = await loadVerifiedModule(table, "core", CORE, served({ [CORE]: coreBytes }, log), { release: false });
    expect(r.ok).toBe(true);
    expect(log).toEqual([CORE]);
    expect(await loadVerifiedModule(table, "core", CORE, served({ [CORE]: enc("other core!") }), { release: false })).toMatchObject({ ok: false });
  });
});
