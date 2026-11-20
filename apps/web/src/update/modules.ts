// The two Core modules a release can serve and the checks that make them one release (ADR-0187; deviations D65 to D68).
// Dependency-free, DOM-free: runs in the page, in Node (build checks) and in tests.
//
// `om_wasm.wasm` is the base Core every build ships. `om_wasm_groups.wasm` is the same Core with the group engine linked in; it is a
// second signed asset that the page fetches only when a person first uses groups. Both are ordinary files of the signed bundle manifest
// (so their digests are signed with everything else), and the build also writes both digests into the bundle's own code (`ModuleTable`),
// which the manifest signature covers too. A module is used only when its bytes match the digest the running code carries, and only
// when the manifest served beside it is the same release and lists both modules with those same digests. Anything else is refused.
import { sha256Hex } from "./hash.ts";
import { GROUPS_WASM_PATH, parseManifest, type BundleManifest } from "./manifest.ts";

export { GROUPS_WASM_PATH };

/** Vite names an emitted asset `assets/<name>-<hash>.<ext>`; these recognise the two Core modules by name. */
export const CORE_WASM_PATH = /^assets\/om_wasm-[A-Za-z0-9_-]+\.wasm$/;
export const EXPRESSIVE_WASM_PATH = /^assets\/om_wasm_expressive-[A-Za-z0-9_-]+\.wasm$/;
/** Manifest files the service worker does not fetch at install: they are loaded on demand and verified by the page. */
export const isLazyPath = (path: string): boolean => GROUPS_WASM_PATH.test(path);

export type ModuleKind = "core" | "groups" | "expressive";
export interface ModuleDigest {
  readonly sha256: string;
  readonly bytes: number;
}
/** What a build knows about its own two modules; compiled into the bundle (a `define`), never read from the network. */
export interface ModuleTable {
  readonly version: string;
  readonly core: ModuleDigest;
  readonly groups: ModuleDigest;
  readonly expressive?: ModuleDigest;
}

const HEX64 = /^[0-9a-f]{64}$/;
const isDigest = (d: unknown): d is ModuleDigest =>
  typeof d === "object" && d !== null && HEX64.test((d as ModuleDigest).sha256) && Number.isSafeInteger((d as ModuleDigest).bytes) && (d as ModuleDigest).bytes > 0;
/** The compiled-in table, or null when it is missing or malformed (callers refuse: fail closed). */
export const parseModuleTable = (v: unknown): ModuleTable | null => {
  if (typeof v !== "object" || v === null) return null;
  const t = v as Partial<ModuleTable>;
  if (typeof t.version !== "string" || t.version === "" || !isDigest(t.core) || !isDigest(t.groups)) return null;
  if (t.expressive !== undefined && !isDigest(t.expressive)) return null;
  return { version: t.version, core: t.core, groups: t.groups, ...(t.expressive === undefined ? {} : { expressive: t.expressive }) };
};

export type ModuleFailure =
  /** No usable table is compiled in. */
  | "table_missing"
  /** The manifest could not be fetched, or is not a manifest. */
  | "manifest_unavailable"
  /** The manifest belongs to a different release than the running code. */
  | "version_skew"
  /** The manifest does not list the module at the address it was asked for, or lists a module with other bytes than the running code expects. */
  | "digest_skew"
  /** The module could not be fetched. */
  | "unavailable"
  /** The bytes are not the bytes the running code expects. */
  | "size_mismatch"
  | "digest_mismatch";
export type ModuleVerdict<T = undefined> = { readonly ok: true; readonly value: T } | { readonly ok: false; readonly reason: ModuleFailure };

const same = (a: ModuleDigest, f: { readonly sha256: string; readonly bytes: number } | undefined): boolean => f !== undefined && f.sha256 === a.sha256 && f.bytes === a.bytes;

/**
 * One release or nothing. The manifest must be this build's release, must list the module at `path` with the digest this build
 * carries, and must also list the other module with the digest this build carries: the base Core and the groups module are only ever
 * used together when both belong to the release that is running.
 */
export const checkSameRelease = (table: ModuleTable, manifest: BundleManifest, kind: ModuleKind, path: string): ModuleVerdict => {
  if (manifest.version !== table.version) return { ok: false, reason: "version_skew" };
  const at = manifest.files.find((f) => f.path === path);
  const expected = table[kind];
  if (expected === undefined || !same(expected, at)) return { ok: false, reason: "digest_skew" };
  const other = kind === "core" ? "groups" : "core";
  const otherMatch = manifest.files.filter((f) => same(table[other], f) && (other === "core" ? CORE_WASM_PATH : GROUPS_WASM_PATH).test(f.path));
  if (otherMatch.length !== 1) return { ok: false, reason: "digest_skew" };
  return { ok: true, value: undefined };
};

/** The bytes against the digest. Refuses a different length or a different SHA-256. */
export const checkBytes = async (expected: ModuleDigest, bytes: ArrayBuffer): Promise<ModuleVerdict> => {
  if (bytes.byteLength !== expected.bytes) return { ok: false, reason: "size_mismatch" };
  return (await sha256Hex(bytes)) === expected.sha256 ? { ok: true, value: undefined } : { ok: false, reason: "digest_mismatch" };
};

export interface ModuleSource {
  /** Fetch a path of this origin (the manifest, or the module). */
  readonly fetch: (path: string) => Promise<{ readonly ok: boolean; text(): Promise<string>; arrayBuffer(): Promise<ArrayBuffer> }>;
  /** A copy kept from an earlier verified load, if any (read back and verified again before use). */
  readonly kept?: { get(sha256: string): Promise<ArrayBuffer | undefined>; put(sha256: string, bytes: ArrayBuffer): Promise<void> };
}

/**
 * The verified bytes of one module of this release, or the reason it was refused. Nothing is returned that was not checked against
 * the digest the running code carries (a kept copy included), so a caller can instantiate the result as is.
 * A kept copy needs no manifest: its digest is the one compiled into the running code, so it is this release's module by construction (and
 * a person who used groups can start offline). Anything that has to be fetched is first checked against the served manifest
 * (`checkSameRelease`) unless `release` is false, which the base Core uses at start-up so that it still starts offline.
 */
export async function loadVerifiedModule(table: ModuleTable | null, kind: ModuleKind, path: string, src: ModuleSource, opts: { readonly release: boolean }): Promise<ModuleVerdict<ArrayBuffer>> {
  if (table === null) return { ok: false, reason: "table_missing" };
  const want = table[kind];
  if (want === undefined) return { ok: false, reason: "table_missing" };
  try {
    const kept = await src.kept?.get(want.sha256);
    if (kept !== undefined && (await checkBytes(want, kept)).ok) return { ok: true, value: kept };
  } catch { /* a kept copy that cannot be read is simply not used */ }
  if (opts.release) {
    let manifest: BundleManifest;
    try {
      const r = await src.fetch("bundle-manifest.json");
      if (!r.ok) return { ok: false, reason: "manifest_unavailable" };
      manifest = parseManifest(await r.text());
    } catch {
      return { ok: false, reason: "manifest_unavailable" };
    }
    const release = checkSameRelease(table, manifest, kind, path);
    if (!release.ok) return release;
  }
  let bytes: ArrayBuffer;
  try {
    const r = await src.fetch(path);
    if (!r.ok) return { ok: false, reason: "unavailable" };
    bytes = await r.arrayBuffer();
  } catch {
    return { ok: false, reason: "unavailable" };
  }
  const verdict = await checkBytes(want, bytes);
  if (!verdict.ok) return verdict;
  try { await src.kept?.put(want.sha256, bytes); } catch { /* not kept: fetched again next time */ }
  return { ok: true, value: bytes };
}
