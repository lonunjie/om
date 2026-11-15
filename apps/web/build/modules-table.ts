// The module table a build compiles into its own code (see src/update/modules.ts): the digests of the two Core modules this build ships.
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import type { ModuleDigest, ModuleTable } from "../src/update/modules.ts";

const dist = (name: string): URL => new URL(`../../../packages/om-core-wasm/dist/${name}`, import.meta.url);
const digestOf = (name: string): ModuleDigest | null => {
  const url = dist(name);
  if (!existsSync(url)) return null;
  const b = readFileSync(url);
  return { sha256: createHash("sha256").update(b).digest("hex"), bytes: b.byteLength };
};

/** Null when either module has not been built (the loader then refuses: nothing is loaded unverified). `pnpm build` builds both first. */
export const moduleTable = (version: string): ModuleTable | null => {
  const core = digestOf("om_wasm.wasm");
  const groups = digestOf("om_wasm_groups.wasm");
  return core === null || groups === null ? null : { version, core, groups };
};
