// Reproducible bundle-manifest generation: pure function over (path, bytes) pairs; no clocks, no randomness.
import { sha256Hex } from "../src/update/hash.ts";
import { MANIFEST_FORMAT, MANIFEST_PATH, type BundleManifest } from "../src/update/manifest.ts";

/** Files that are host configuration, not served content (Pages consumes `_headers`; it is never fetchable). */
export const UNSERVED = new Set(["_headers"]);

export interface BuildInput {
  readonly files: readonly { readonly path: string; readonly data: Uint8Array }[];
  readonly version: string;
  readonly entry?: string;
  readonly serviceWorker?: string;
  readonly gitCommit?: string | undefined;
  readonly dev?: boolean;
}

const byCodepoint = (a: string, b: string): number => (a < b ? -1 : a > b ? 1 : 0);

export const buildManifest = async (input: BuildInput): Promise<BundleManifest> => {
  const entries = input.files.filter((f) => f.path !== MANIFEST_PATH && !UNSERVED.has(f.path)).toSorted((a, b) => byCodepoint(a.path, b.path));
  const files = await Promise.all(entries.map(async (f) => ({ path: f.path, sha256: await sha256Hex(f.data), bytes: f.data.byteLength })));
  return {
    format: MANIFEST_FORMAT,
    version: input.version,
    files,
    entry: input.entry ?? "index.html",
    serviceWorker: input.serviceWorker ?? "sw.js",
    createdFrom: input.gitCommit ? { gitCommit: input.gitCommit } : {},
    ...(input.dev ? { dev: true as const } : {}),
    signature: null,
  };
};

export const serializeManifest = (m: BundleManifest): string => JSON.stringify(m, null, 2) + "\n";
