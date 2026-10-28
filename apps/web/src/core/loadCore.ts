// Loads the real OM Core (crates/om-wasm, compiled to om_wasm.wasm) and starts the runtime over the profile vault.
// The wasm module is a hashed build asset, so it is covered by the signed bundle manifest like every other file.
import { createCoreRuntime, instantiateCore, type Provider, type WasmCoreClient } from "@om/core-wasm";
import type { DeviceVault, ProfileId } from "@om/platform-api";
import { createEncryptedKv } from "@om/web-platform";
import wasmUrl from "@om/core-wasm/om_wasm.wasm?url";

export async function startCore(opts: { readonly vault: DeviceVault; readonly indexedDB: IDBFactory; readonly provider: Provider; readonly pollMs: number }): Promise<WasmCoreClient> {
  const res = await fetch(wasmUrl);
  if (!res.ok) throw new Error("core module unavailable");
  const core = await instantiateCore(await res.arrayBuffer());
  return createCoreRuntime({
    core,
    provider: opts.provider,
    storage: (profileId: ProfileId) => createEncryptedKv(opts.vault, { indexedDB: opts.indexedDB }, profileId),
    // No provider adapter exists yet, so no third party can observe anything; the real facts arrive with one.
    home: { tier: "none", observers: ["no_third_party"], migrationAvailable: false, clientOriginChoice: ["default_origin"] },
    pollMs: opts.pollMs,
  });
}
