import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { IDBFactory } from "fake-indexeddb";
import { describe, expect, it, vi } from "vitest";
import { createHomeSettings } from "./homeSettings";
import { admissionHash } from "@om/home-wire";
import { createIndexedDbSecretStore, readAdmission } from "@om/provider-http";
import { addBackupHome, createHomesProviderFor, resolveHomeSet, resolveProviderChoice, saveRelayUrl, type ProviderEnv } from "./providerConfig";

const doc = { schema_version: 1, provider_id: "test-home", provider_kinds: ["relay"], durability_class: "durable", features: [], api_versions: ["1"], health: "healthy" };
const urlOf = (i: RequestInfo | URL): string => (typeof i === "string" ? i : i instanceof URL ? i.href : i.url);
const homeFetch = (log: string[]): typeof fetch => (i) => {
  log.push(urlOf(i));
  return Promise.resolve(new Response(JSON.stringify(doc), { status: 200, headers: { "content-type": "application/json" } }));
};

describe("Settings > Your Home over the real configuration", () => {
  it("probes only after the address passes the URL rules, and only the public capabilities document", async () => {
    const urls: string[] = [];
    const env = { indexedDB: new IDBFactory(), fetch: homeFetch(urls) };
    const s = createHomeSettings({ env, choice: null, provider: null, reconnect: () => undefined });
    for (const [bad, reason] of [["http://home.example", "not_https"], ["https://u:p@home.example", "credentials"], ["https://home.example/?c=1", "query_or_fragment"], ["nope", "malformed"]] as const) {
      expect(await s.probe(bad)).toEqual({ kind: "invalid_url", reason });
    }
    expect(urls).toEqual([]); // nothing was requested for an unacceptable address
    expect(await s.probe(" https://home.example/ ")).toEqual({ kind: "ok", baseUrl: "https://home.example", providerId: "test-home", health: "healthy", admission: false });
    expect(urls).toEqual(["https://home.example/.well-known/om-provider-capabilities"]);
  });

  it("saving stores a choice that wins over the build default; clearing returns to the default; a bad address is refused", async () => {
    const env = { indexedDB: new IDBFactory(), buildUrl: "https://default.example" };
    const reconnect = vi.fn();
    const s = createHomeSettings({ env, choice: { config: { baseUrl: "https://default.example" }, source: "default" }, provider: null, reconnect });
    expect(s.current).toEqual({ baseUrl: "https://default.example", source: "default", hasDefault: true });
    expect(await s.save("http://nope.example")).toBe(false);
    expect((await resolveProviderChoice(env))?.source).toBe("default");
    expect(await s.save("https://mine.example")).toBe(true);
    expect(await resolveProviderChoice(env)).toEqual({ config: { baseUrl: "https://mine.example" }, source: "chosen" });
    expect(await s.clear()).toBe(true);
    expect((await resolveProviderChoice(env))?.source).toBe("default");
    s.reconnect();
    expect(reconnect).toHaveBeenCalledTimes(1);
  });

  it("reports no default when the build has none, and no lost mailboxes without a provider", () => {
    const s = createHomeSettings({ env: { indexedDB: new IDBFactory() }, choice: null, provider: null, reconnect: () => undefined });
    expect(s.current).toEqual({ baseUrl: undefined, source: "none", hasDefault: false });
    expect(s.lostMailboxes()).toEqual([]);
  });

  it("passes the provider's lost mailboxes through", () => {
    const provider = { lostMailboxes: () => ["aa", "bb"] } as unknown as Parameters<typeof createHomeSettings>[0]["provider"];
    const s = createHomeSettings({ env: { indexedDB: new IDBFactory() }, choice: null, provider, reconnect: () => undefined });
    expect(s.lostMailboxes()).toEqual(["aa", "bb"]);
  });
});

describe("no link-based way to set the Home", () => {
  const here = dirname(fileURLToPath(import.meta.url));
  const files = (dir: string): string[] => readdirSync(dir).flatMap((n) => {
    const p = join(dir, n);
    return statSync(p).isDirectory() ? files(p) : /\.tsx?$/.test(n) && !/\.test\./.test(n) ? [p] : [];
  });

  it("the stored choice is written only by saveRelayUrl, which only the Settings screen's services and the first-run setup page's services call (both from typed text)", () => {
    const callers = files(join(here, "..")).filter((f) => /saveRelayUrl/.test(readFileSync(f, "utf8"))).map((f) => f.slice(join(here, "..").length + 1));
    expect(callers.sort()).toEqual(["core/homeSettings.ts", "core/providerConfig.ts", "core/relaySetup.ts"]);
  });

  it("services are added or left only by the Settings screen's services (typed text, then a tap), never by a link", () => {
    const callers = files(join(here, "..")).filter((f) => /addBackupHome|markHomeLeaving/.test(readFileSync(f, "utf8"))).map((f) => f.slice(join(here, "..").length + 1));
    expect(callers.sort()).toEqual(["core/homeSettings.ts", "core/providerConfig.ts"]);
  });

  it("configuration never reads the page address, so a link cannot point the app at a Home", () => {
    for (const f of ["providerConfig.ts", "homeSettings.ts", "relaySetup.ts", "ConfiguredApp.tsx"]) {
      expect(readFileSync(join(here, f), "utf8"), f).not.toMatch(/location|URLSearchParams|document\.referrer|window\.name/);
    }
  });

  it("saving rejects any address that could carry a query or fragment, so a link-shaped value cannot become the Home", async () => {
    const indexedDB = new IDBFactory();
    expect(await saveRelayUrl({ indexedDB }, "https://home.example/?relay=https://evil.example")).toBe(false);
  });
});

describe("Settings > Message delivery > Advanced over the real configuration", () => {
  const retired: string[] = [];
  /** A person with a main service and one backup, as the app resolves them at start. */
  async function rig(opts: { retire?: (uri: string) => Promise<number>; backups?: readonly string[] } = {}) {
    const env: ProviderEnv = { indexedDB: new IDBFactory(), fetch: homeFetch([]) };
    await saveRelayUrl(env, "https://main.example");
    for (const b of opts.backups ?? ["https://backup.example"]) await addBackupHome(env, b);
    const set = await resolveHomeSet(env);
    if (set === null) throw new Error("no set");
    const provider = await createHomesProviderFor(env, set);
    const host = createHomeSettings({ env, choice: await resolveProviderChoice(env), set, provider, deployUrl: "https://deploy.example/start", reconnect: () => undefined });
    retired.length = 0;
    const bound = host.bindCore?.(opts.retire ?? ((uri) => { retired.push(uri); return Promise.resolve(0); }));
    if (bound?.more === undefined) throw new Error("no more");
    return { env, provider, host, more: bound.more };
  }

  it("lists the main service first, then backups, then the ones being left, and hands the screen the setup pieces", async () => {
    const { more, env } = await rig();
    expect(more.services).toEqual([{ baseUrl: "https://main.example", role: "main" }, { baseUrl: "https://backup.example", role: "backup" }]);
    expect(more.setup.deployUrl).toBe("https://deploy.example/start");
    const hash = await more.setup.prepare();
    expect(hash).toMatch(/^[0-9a-f]{64}$/);
    expect(await more.setup.prepare()).toBe(hash); // the same setup value until a service is added
    expect(await resolveHomeSet(env)).not.toBeNull();
  });

  it("offers more only where there is a service to add to", () => {
    const none = createHomeSettings({ env: { indexedDB: new IDBFactory() }, choice: null, provider: null, reconnect: () => undefined });
    expect(none.more).toBeUndefined();
    expect(none.bindCore?.(() => Promise.resolve(0)).more).toBeUndefined();
  });

  it("adding a backup stores the address and the matching setup secret together, and only for that address", async () => {
    const { more, env } = await rig({ backups: [] });
    const shown = await more.setup.prepare();
    expect(await more.add("http://nope.example")).toBe("failed");
    expect(await more.add("https://new.example/")).toBe("added");
    const secret = await readAdmission(createIndexedDbSecretStore(env.indexedDB), "https://new.example");
    expect(await admissionHash(secret ?? new Uint8Array(0))).toBe(shown);
    expect(await readAdmission(createIndexedDbSecretStore(env.indexedDB), "https://main.example")).toBeUndefined();
    expect((await resolveHomeSet(env))?.homes.map((h) => h.config.baseUrl)).toEqual(["https://main.example", "https://new.example"]);
  });

  it("refuses an address that is already in the list without touching its secret, and refuses when no setup value was ever made", async () => {
    const { more, env } = await rig();
    expect(await more.add("https://backup.example")).toBe("already");
    expect(await more.add("https://main.example/")).toBe("already");
    expect(await readAdmission(createIndexedDbSecretStore(env.indexedDB), "https://backup.example")).toBeUndefined(); // an existing service's secret is never overwritten
    expect(await more.add("https://fresh.example")).toBe("failed"); // prepare() was never called: no secret to bind, so nothing is stored
    expect((await resolveHomeSet(env))?.homes).toHaveLength(2);
  });

  it("a second attempt for an address whose secret was already bound still succeeds", async () => {
    const { more, env } = await rig({ backups: [] });
    await more.setup.prepare();
    // Simulate a first attempt that bound the secret and then failed to store the list.
    const store = createIndexedDbSecretStore(env.indexedDB);
    const { adoptAdmission } = await import("@om/provider-http");
    expect(await adoptAdmission(store, "https://again.example")).toBe(true);
    expect(await more.add("https://again.example")).toBe("added");
  });

  it("moving away asks Core with the service's route name, then records it as leaving; the list shows it as moving away after a restart", async () => {
    const { more, env, provider } = await rig();
    const offers = async (): Promise<string[]> => [...new Set((await provider.allocateRoutes(6)).map((o) => o.relayUri))].sort();
    expect(await offers()).toEqual(["https://backup.example/", "https://main.example/"]);
    expect(await more.move("https://main.example")).toEqual({ kind: "moved", stuck: 0 });
    expect(retired).toEqual(["https://main.example/"]);
    expect(await offers()).toEqual(["https://backup.example/"]); // not offered again before the restart
    const set = await resolveHomeSet(env);
    expect(set?.homes.map((h) => [h.config.baseUrl, h.leaving])).toEqual([["https://backup.example", false], ["https://main.example", true]]);
    // The next connection passes it as draining: still collected from, never offered.
    const next = await createHomesProviderFor(env, set ?? { source: "chosen", homes: [] });
    expect([...new Set((await next.allocateRoutes(6)).map((o) => o.relayUri))]).toEqual(["https://backup.example/"]);
  });

  it("reports chats that have no other service as a plain count, keeps the service in the list, and can be repeated", async () => {
    let stuck = 2;
    const { more, env } = await rig({ retire: () => Promise.resolve(stuck) });
    expect(await more.move("https://main.example")).toEqual({ kind: "moved", stuck: 2 });
    stuck = 0;
    expect(await more.move("https://main.example")).toEqual({ kind: "moved", stuck: 0 }); // a service already being left may be moved again
    expect((await resolveHomeSet(env))?.homes.find((h) => h.config.baseUrl === "https://main.example")?.leaving).toBe(true);
  });

  it("will not move away from the only service that is not being left, and says so", async () => {
    const { more } = await rig({ backups: [] });
    expect(await more.move("https://main.example")).toEqual({ kind: "last" });
    expect(retired).toEqual([]); // Core was not even asked
    const two = await rig();
    expect(await two.more.move("https://main.example")).toEqual({ kind: "moved", stuck: 0 });
    expect(await two.more.move("https://backup.example")).toEqual({ kind: "last" });
  });

  it("when Core fails nothing is recorded and the service is offered again; an unknown address or an unbound host cannot move anything", async () => {
    const { more, env, provider, host } = await rig({ retire: () => Promise.reject(new Error("core")) });
    expect(await more.move("https://main.example")).toEqual({ kind: "failed" });
    expect((await resolveHomeSet(env))?.homes.every((h) => !h.leaving)).toBe(true);
    expect([...new Set((await provider.allocateRoutes(6)).map((o) => o.relayUri))].sort()).toEqual(["https://backup.example/", "https://main.example/"]);
    expect(await more.move("https://unknown.example")).toEqual({ kind: "failed" });
    expect(await host.more?.move("https://main.example")).toEqual({ kind: "failed" }); // before an identity is open there is no Core to ask
  });
});
