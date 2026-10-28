// Bundle manifest model (dependency-free, DOM-free: runs in a service worker, a native web view and Node).
export const MANIFEST_FORMAT = "om.web-bundle/1";
export const MANIFEST_PATH = "bundle-manifest.json";

export interface BundleFile {
  readonly path: string;
  readonly sha256: string;
  readonly bytes: number;
}
export interface BundleSignature {
  readonly alg: string;
  readonly keyId: string;
  /** base64 signature over `signingPayload(manifest)`. */
  readonly sig: string;
}
/** Signed marker authorising exactly one downgrade from `from` to `to` (covered by the manifest signature). */
export interface RollbackMarker {
  readonly from: string;
  readonly to: string;
}
export interface BundleManifest {
  readonly format: typeof MANIFEST_FORMAT;
  readonly version: string;
  readonly files: readonly BundleFile[];
  readonly entry: string;
  readonly serviceWorker: string;
  readonly createdFrom: { readonly gitCommit?: string };
  /** Present only on explicit dev builds; honoured only when the trust root allows unsigned dev bundles. */
  readonly dev?: true;
  readonly rollback?: RollbackMarker;
  /** Root-signed release-key delegation (`delegation.ts`), shipped inside every signed manifest and covered by the release signature. Untrusted until verified. */
  readonly delegation?: unknown;
  readonly signature: BundleSignature | null;
}

const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const HEX64 = /^[0-9a-f]{64}$/;

/** Compare strict `x.y.z` versions; returns null when either is malformed (callers fail closed). */
export const compareVersions = (a: string, b: string): number | null => {
  const ma = SEMVER.exec(a);
  const mb = SEMVER.exec(b);
  if (!ma || !mb) return null;
  for (let i = 1; i <= 3; i++) {
    const d = Number(ma[i]) - Number(mb[i]);
    if (d !== 0) return d < 0 ? -1 : 1;
  }
  return 0;
};

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const safePath = (p: unknown): p is string =>
  typeof p === "string" && p.length > 0 && !p.startsWith("/") && !p.includes("\\") && !p.split("/").some((s) => s === "" || s === "." || s === "..");

/** Parse and structurally validate untrusted manifest JSON. Throws on anything malformed. */
export const parseManifest = (text: string): BundleManifest => {
  const m: unknown = JSON.parse(text);
  if (!isRecord(m) || m["format"] !== MANIFEST_FORMAT) throw new Error("manifest: bad format");
  const { version, files, entry, serviceWorker, createdFrom, signature, dev, rollback, delegation } = m;
  if (typeof version !== "string" || !SEMVER.test(version)) throw new Error("manifest: bad version");
  if (!Array.isArray(files)) throw new Error("manifest: bad files");
  const seen = new Set<string>();
  for (const f of files as unknown[]) {
    if (!isRecord(f) || !safePath(f["path"]) || typeof f["sha256"] !== "string" || !HEX64.test(f["sha256"])) throw new Error("manifest: bad file entry");
    if (typeof f["bytes"] !== "number" || !Number.isSafeInteger(f["bytes"]) || f["bytes"] < 0) throw new Error("manifest: bad file size");
    if (f["path"] === MANIFEST_PATH || seen.has(f["path"])) throw new Error("manifest: duplicate or self-referencing path");
    seen.add(f["path"]);
  }
  if (typeof entry !== "string" || !seen.has(entry) || typeof serviceWorker !== "string" || !seen.has(serviceWorker)) throw new Error("manifest: entry/serviceWorker not listed");
  if (!isRecord(createdFrom)) throw new Error("manifest: bad createdFrom");
  if (dev !== undefined && dev !== true) throw new Error("manifest: bad dev flag");
  if (rollback !== undefined && (!isRecord(rollback) || typeof rollback["from"] !== "string" || typeof rollback["to"] !== "string")) throw new Error("manifest: bad rollback marker");
  if (delegation !== undefined && !isRecord(delegation)) throw new Error("manifest: bad delegation");
  if (signature !== null && (!isRecord(signature) || typeof signature["alg"] !== "string" || typeof signature["keyId"] !== "string" || typeof signature["sig"] !== "string")) throw new Error("manifest: bad signature");
  return m as unknown as BundleManifest;
};

/** Deterministic JSON: sorted object keys, no whitespace; files keep their (already sorted) order. */
export const canonicalJson = (v: unknown): string => {
  if (Array.isArray(v)) return `[${(v as unknown[]).map(canonicalJson).join(",")}]`;
  if (isRecord(v)) {
    return `{${Object.keys(v)
      .filter((k) => v[k] !== undefined)
      .sort()
      .map((k) => `${JSON.stringify(k)}:${canonicalJson(v[k])}`)
      .join(",")}}`;
  }
  return JSON.stringify(v);
};

/** Bytes a release signature covers: the whole manifest with the signature slot nulled. */
export const signingPayload = (m: BundleManifest): Uint8Array => new TextEncoder().encode(canonicalJson({ ...m, signature: null }));
