// Loads the real OM Core (crates/om-wasm, compiled to om_wasm.wasm) and starts the runtime over the profile vault.
// The wasm module is a hashed build asset, so it is covered by the signed bundle manifest like every other file.
import { createCoreRuntime, instantiateCore, type CommitAcceptance, type GroupCarriage, type Provider, type WasmCoreClient } from "@om/core-wasm";
import type { DeviceVault, ProfileId } from "@om/platform-api";
import { createEncryptedKv } from "@om/web-platform";
import wasmUrl from "@om/core-wasm/om_wasm.wasm?url";

/**
 * Groups (ADR-0177) are off in every build unless `VITE_OM_GROUPS=1`, and vite.config.ts refuses that flag in a signed production
 * build: ADR-0187 (accepted) covers the engine module, but no product carriage exists yet. With the flag the groups module replaces the shipped one
 * (same ABI plus the engine) and `groups.available()` is true only when the host also supplies a carriage for group wires.
 * No product carriage exists (groups plan Q2), so a real build leaves it out and groups stay unavailable; the e2e build passes a loopback.
 */
export interface StartGroups {
  /** Called once with a getter for the client being built; `null`: this host cannot carry group wires. */
  readonly carriage: (client: () => WasmCoreClient) => GroupCarriage | null;
  readonly acceptance?: CommitAcceptance;
}

export async function startCore(opts: { readonly vault: DeviceVault; readonly indexedDB: IDBFactory; readonly provider: Provider; readonly pollMs: number | (() => number); readonly groups?: StartGroups }): Promise<WasmCoreClient> {
  let url = wasmUrl;
  if (import.meta.env.VITE_OM_GROUPS === "1") url = (await import("@om/core-wasm/om_wasm_groups.wasm?url")).default; // statically dead without the flag: never bundled
  const res = await fetch(url);
  if (!res.ok) throw new Error("core module unavailable");
  const core = await instantiateCore(await res.arrayBuffer());
  const started: { rt?: WasmCoreClient } = {};
  const carriage = import.meta.env.VITE_OM_GROUPS === "1" ? (opts.groups?.carriage(() => { if (started.rt === undefined) throw new Error("core not started"); return started.rt; }) ?? null) : null;
  const rt = await createCoreRuntime({
    core,
    provider: opts.provider,
    storage: (profileId: ProfileId) => createEncryptedKv(opts.vault, { indexedDB: opts.indexedDB }, profileId),
    // No provider adapter exists yet, so no third party can observe anything; the real facts arrive with one.
    home: { tier: "none", observers: ["no_third_party"], migrationAvailable: false, clientOriginChoice: ["default_origin"] },
    pollMs: opts.pollMs,
    ...(import.meta.env.VITE_OM_GROUPS === "1" ? { groupDriver: { carriage, ...(opts.groups?.acceptance === undefined ? {} : { acceptance: opts.groups.acceptance }) } } : {}),
  });
  started.rt = rt;
  return rt;
}
