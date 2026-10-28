import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { IDBFactory } from "fake-indexeddb";
import { describe, expect, it, vi } from "vitest";
import { createHomeSettings } from "./homeSettings";
import { resolveProviderChoice, saveRelayUrl } from "./providerConfig";

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
    expect(await s.probe(" https://home.example/ ")).toEqual({ kind: "ok", baseUrl: "https://home.example", providerId: "test-home", health: "healthy" });
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

  it("the stored choice is written only by saveRelayUrl, which only the Settings screen's services call", () => {
    const callers = files(join(here, "..")).filter((f) => /saveRelayUrl/.test(readFileSync(f, "utf8"))).map((f) => f.slice(join(here, "..").length + 1));
    expect(callers.sort()).toEqual(["core/homeSettings.ts", "core/providerConfig.ts"]);
  });

  it("configuration never reads the page address, so a link cannot point the app at a Home", () => {
    for (const f of ["providerConfig.ts", "homeSettings.ts", "ConfiguredApp.tsx"]) {
      expect(readFileSync(join(here, f), "utf8"), f).not.toMatch(/location|URLSearchParams|document\.referrer|window\.name/);
    }
  });

  it("saving rejects any address that could carry a query or fragment, so a link-shaped value cannot become the Home", async () => {
    const indexedDB = new IDBFactory();
    expect(await saveRelayUrl({ indexedDB }, "https://home.example/?relay=https://evil.example")).toBe(false);
  });
});
