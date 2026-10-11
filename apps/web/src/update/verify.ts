import { sha256Hex } from "./hash";
import type { BundleManifest } from "./manifest";

/** Minimal response shape so the verifier runs with fetch, a Cache lookup, or a test double. */
export interface FileResponse {
  readonly ok: boolean;
  arrayBuffer(): Promise<ArrayBuffer>;
}
export type FetchFile = (path: string) => Promise<FileResponse | undefined>;

export type BundleVerdict =
  | { readonly ok: true }
  | { readonly ok: false; readonly reason: "missing" | "hash_mismatch" | "size_mismatch" | "unlisted_file"; readonly path: string };

/**
 * Re-hash every listed file exactly as the shell loads it and reject on any mismatch.
 * `listPaths` (optional) enumerates what the shell actually holds (e.g. cache keys);
 * anything present but not in the manifest rejects the bundle.
 */
export const verifyLoadedBundle = async (
  manifest: BundleManifest,
  fetchFn: FetchFile,
  listPaths?: () => Promise<readonly string[]>,
): Promise<BundleVerdict> => {
  for (const f of manifest.files) {
    let res: FileResponse | undefined;
    try {
      res = await fetchFn(f.path);
    } catch {
      return { ok: false, reason: "missing", path: f.path };
    }
    if (!res?.ok) return { ok: false, reason: "missing", path: f.path };
    const buf = await res.arrayBuffer();
    if (buf.byteLength !== f.bytes) return { ok: false, reason: "size_mismatch", path: f.path };
    if ((await sha256Hex(buf)) !== f.sha256) return { ok: false, reason: "hash_mismatch", path: f.path };
  }
  if (listPaths) {
    const listed = new Set(manifest.files.map((f) => f.path));
    for (const p of await listPaths()) if (!listed.has(p)) return { ok: false, reason: "unlisted_file", path: p };
  }
  return { ok: true };
};
