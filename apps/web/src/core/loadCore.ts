// Loads the real OM Core (crates/om-wasm, compiled to om_wasm.wasm) and starts the runtime over the profile vault.
// The wasm module is a hashed build asset, so it is covered by the signed bundle manifest like every other file.
import { createCoreRuntime, instantiateCore, type Provider, type WasmCoreClient } from "@om/core-wasm";
import type { DeviceVault, ProfileId } from "@om/platform-api";
import { createEncryptedKv } from "@om/web-platform";
import wasmUrl from "@om/core-wasm/om_wasm.wasm?url";

/**
 * Groups (ADR-0177) are off in every build unless `VITE_OM_GROUPS=1`, and vite.config.ts refuses that flag in a signed production
 * build until the groups module ships as its own signed asset (the manifest path is listed in the groups carriage report, package 3a).
 * With the flag the groups module replaces the shipped one (same ABI plus the engine) and the runtime gets the group driver, which
 * carries group frames over the Home the person already uses (ADR-0189): `groups.available()` is true exactly when the module is present.
 */
export async function startCore(opts: { readonly vault: DeviceVault; readonly indexedDB: IDBFactory; readonly provider: Provider; readonly pollMs: number | (() => number) }): Promise<WasmCoreClient> {
  let url = wasmUrl;
  if (import.meta.env.VITE_OM_GROUPS === "1") url = (await import("@om/core-wasm/om_wasm_groups.wasm?url")).default; // statically dead without the flag: never bundled
  const res = await fetch(url);
  if (!res.ok) throw new Error("core module unavailable");
  const core = await instantiateCore(await res.arrayBuffer());
  const rt = await createCoreRuntime({
    core,
    provider: opts.provider,
    storage: (profileId: ProfileId) => createEncryptedKv(opts.vault, { indexedDB: opts.indexedDB }, profileId),
    // No provider adapter exists yet, so no third party can observe anything; the real facts arrive with one.
    home: { tier: "none", observers: ["no_third_party"], migrationAvailable: false, clientOriginChoice: ["default_origin"] },
    pollMs: opts.pollMs,
    ...(import.meta.env.VITE_OM_GROUPS === "1" ? { groupDriver: {} } : {}),
  });
  return rt;
}
