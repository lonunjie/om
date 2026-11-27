// SPDX-License-Identifier: MPL-2.0
import type { Plugin } from "vite";

export function isScopeModule(id: string): boolean {
  const path = id.replaceAll("\\", "/");
  return /(?:^|\/)host\/scope(?:[/?#]|$)/.test(path) ||
    /(?:^|\/)om_wasm_scope(?:[.-][^/?#]*)?\.wasm(?:[?#]|$)/.test(path);
}

/** Run on emitted output, after the engineering entry has been eliminated by the build flags. */
export function scopeReleaseGuard(): Plugin {
  return {
    name: "om-release-scope-guard",
    generateBundle(_options, bundle) {
      for (const output of Object.values(bundle)) {
        const ids = output.type === "chunk" ? [output.fileName, ...Object.keys(output.modules)]
          : [output.fileName, ...output.originalFileNames];
        if (ids.some(isScopeModule)) this.error("Engineering scope code in the release bundle");
      }
    },
  };
}
