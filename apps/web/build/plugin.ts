import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve, sep } from "node:path";
import type { Plugin } from "vite";
import { MANIFEST_PATH } from "../src/update/manifest.ts";
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
export const bundleManifestPlugin = (opts: { version: string; dev: boolean }): Plugin => {
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
      writeFileSync(join(outDir, MANIFEST_PATH), serializeManifest(manifest));
    },
  };
};
