import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve, sep } from "node:path";
import type { Plugin } from "vite";
import { MANIFEST_PATH } from "../src/update/manifest.ts";
import { CORE_WASM_PATH, GROUPS_WASM_PATH, checkSameRelease, type ModuleTable } from "../src/update/modules.ts";
import { buildManifest, serializeManifest } from "./manifest.ts";

const walk = (dir: string): string[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)]));

const gitCommit = (): string | undefined => {
  if (process.env["OM_GIT_COMMIT"]) return process.env["OM_GIT_COMMIT"];
  try {
    return execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim() || undefined;
  } catch {
    return undefined;
  }
};

/** Emits `bundle-manifest.json` after every other output file (incl. public/) is in place. */
export const bundleManifestPlugin = (opts: { version: string; dev: boolean; modules: ModuleTable | null }): Plugin => {
  let outDir = "";
  return {
    name: "om-bundle-manifest",
    apply: "build",
    enforce: "post",
    configResolved(c) {
      outDir = resolve(c.root, c.build.outDir);
    },
    async closeBundle() {
      const files = walk(outDir)
        .map((abs) => ({ path: relative(outDir, abs).split(sep).join("/"), data: readFileSync(abs) }))
        .filter((f) => f.path !== MANIFEST_PATH);
      const manifest = await buildManifest({ files, version: opts.version, gitCommit: gitCommit(), dev: opts.dev });
      // Build-time proof that the two Core modules in this bundle are the two the bundle's own code carries digests for (D66), and that the groups module is lazy.
      if (opts.modules === null) throw new Error("om-bundle-manifest: om_wasm.wasm and om_wasm_groups.wasm must both be built before the web build (pnpm build does this)");
      for (const [kind, re] of [["core", CORE_WASM_PATH], ["groups", GROUPS_WASM_PATH]] as const) {
        const at = manifest.files.filter((f) => re.test(f.path));
        if (at.length !== 1 || at[0] === undefined) throw new Error(`om-bundle-manifest: expected exactly one ${kind} module in the bundle, found ${String(at.length)}`);
        const v = checkSameRelease(opts.modules, manifest, kind, at[0].path);
        if (!v.ok) throw new Error(`om-bundle-manifest: the ${kind} module in the bundle does not match the digest compiled into it (${v.reason})`);
        if ((kind === "groups") !== (at[0].lazy === true)) throw new Error(`om-bundle-manifest: the ${kind} module has the wrong lazy marking`);
      }
      writeFileSync(join(outDir, MANIFEST_PATH), serializeManifest(manifest));
    },
  };
};
