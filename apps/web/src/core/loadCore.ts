// Loads the real OM Core (crates/om-wasm, compiled to om_wasm.wasm) and starts the runtime over the profile vault.
// Both Core modules are hashed build assets covered by the signed bundle manifest, and the build also compiles their digests into this
// code (src/update/modules.ts): a module is instantiated only when its bytes match.
//
// Groups (ADR-0177, ADR-0187): the groups module (om_wasm_groups.wasm, the same Core plus the group engine) is a second signed asset. It is NOT
// fetched at first load: the base module starts the app, and the groups module is fetched, verified and cached the first time a person
// uses groups (`prepareGroups`), after which the app restarts onto it (a Core instance cannot be swapped under open profiles: every context
// they hold is bound to the instance, D67). A device that has used groups starts on the groups module from then on.
import { createCoreRuntime, instantiateCore, type Provider, type WasmCoreClient } from "@om/core-wasm";
import type { DeviceVault, ProfileId } from "@om/platform-api";
import type { ExpressiveHost } from "@om/ui";
import { createEncryptedKv } from "@om/web-platform";
import baseUrl from "@om/core-wasm/om_wasm.wasm?url";
import { loadVerifiedModule, parseModuleTable, type ModuleFailure, type ModuleKind, type ModuleSource, type ModuleTable } from "../update/modules";

const MARK = "om.groups.v1";
const OPEN_NEW = "om.groups.open-new";
const KEPT = "om-lazy-modules-v1";

const store = (): Storage | null => { try { return globalThis.localStorage; } catch { return null; } };
/** True once this device has loaded the groups module (a per-device capability marker; it names nobody and holds no key). */
export const groupsInUse = (): boolean => { try { return store()?.getItem(MARK) === "1"; } catch { return false; } };
const markGroups = (): void => { try { store()?.setItem(MARK, "1"); } catch { /* not remembered: the module is fetched again next time */ } };
/** After the restart, take the person straight back to what they asked for. */
export const rememberOpenNewGroup = (): void => { try { globalThis.sessionStorage.setItem(OPEN_NEW, "1"); } catch { /* the person taps again */ } };
export const takeOpenNewGroup = (): boolean => { try { const v = globalThis.sessionStorage.getItem(OPEN_NEW) === "1"; globalThis.sessionStorage.removeItem(OPEN_NEW); return v; } catch { return false; } };

const table = (): ModuleTable | null => parseModuleTable(typeof __OM_WASM_MODULES__ === "undefined" ? null : __OM_WASM_MODULES__);
const pathOf = (url: string): string => new URL(url, globalThis.location.href).pathname.replace(/^\//, "");

const source = (): ModuleSource => ({
  fetch: (path) => fetch(new URL(path, globalThis.location.origin), { cache: "no-store" }),
  kept: {
    async get(sha256) {
      const c = await globalThis.caches.open(KEPT);
      return (await c.match("/__om-module/" + sha256))?.arrayBuffer();
    },
    async put(sha256, bytes) {
      const c = await globalThis.caches.open(KEPT);
      await c.put("/__om-module/" + sha256, new Response(bytes.slice(0)));
    },
  },
});

const moduleUrl = async (kind: ModuleKind): Promise<string> => (kind === "core" ? baseUrl : (await import("@om/core-wasm/om_wasm_groups.wasm?url")).default);

/** Fetch, verify against this release and keep the groups module. Refuses (never half-loads) on any mismatch. After `ok` the app restarts onto it. */
export async function prepareGroups(): Promise<{ readonly ok: true } | { readonly ok: false; readonly reason: ModuleFailure }> {
  let url: string;
  try { url = await moduleUrl("groups"); } catch { return { ok: false, reason: "unavailable" }; }
  const v = await loadVerifiedModule(table(), "groups", pathOf(url), source(), { release: true });
  if (!v.ok) return v;
  markGroups();
  return { ok: true };
}

export type WebCoreClient = WasmCoreClient & { readonly describeExpressive?: ExpressiveHost["describe"] };

export async function startCore(opts: { readonly vault: DeviceVault; readonly indexedDB: IDBFactory; readonly provider: Provider; readonly pollMs: number | (() => number) }): Promise<WebCoreClient> {
  let bytes: ArrayBuffer | undefined;
  let groups = false;
  if (groupsInUse()) {
    // A device that has used groups starts on the groups module. If it cannot be verified now (offline and not kept, or a changed release), the base module
    // starts instead and groups are offered again on demand: chats keep working, nothing unverified is loaded.
    const g = await loadVerifiedModule(table(), "groups", pathOf(await moduleUrl("groups")), source(), { release: true });
    if (g.ok) { bytes = g.value; groups = true; }
  }
  if (bytes === undefined) {
    // Offline-safe: the base module is checked against the digest compiled into this code, without asking the network for the manifest.
    const b = await loadVerifiedModule(table(), "core", pathOf(baseUrl), source(), { release: false });
    if (!b.ok) throw new Error(`core module refused (${b.reason})`);
    bytes = b.value;
  }
  const core = await instantiateCore(bytes);
  const runtime = await createCoreRuntime({
    core,
    provider: opts.provider,
    storage: (profileId: ProfileId) => createEncryptedKv(opts.vault, { indexedDB: opts.indexedDB }, profileId),
    // No provider adapter exists yet, so no third party can observe anything; the real facts arrive with one.
    home: { tier: "none", observers: ["no_third_party"], migrationAvailable: false, clientOriginChoice: ["default_origin"] },
    pollMs: opts.pollMs,
    calls: import.meta.env.VITE_OM_CALLS === "1",
    expressiveMedia: import.meta.env.VITE_OM_EXPRESSIVE_MEDIA === "1",
    ...(groups ? { groupDriver: {} } : {}),
  });
  if (!runtime.expressive.available()) return runtime;
  // The existing bridge exposes a local plan query. Keep the plan inside the host and
  // do not perform its provider call here. The visible Open control uses CoreClient.decode.
  const describeExpressive: ExpressiveHost["describe"] = (ctx, messageId, source) => Promise.resolve().then(() => {
    const result = core.call({ cmd: "expressive_query", ctx, query: { kind: "fetch", messageId, source } }) as {
      readonly altText: string; readonly call?: unknown;
    } | null;
    return result === null ? null : { altText: result.altText, canOpen: result.call !== undefined };
  });
  return Object.assign(runtime, { describeExpressive });
}
