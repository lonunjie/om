import { IDBFactory } from "fake-indexeddb";
import { describe, expect, it } from "vitest";
import { MAX_HOMES, addBackupHome, createConfiguredProvider, createHomesProviderFor, decodeHomes, markHomeLeaving, resolveHomeSet, resolveProviderConfig, saveRelayUrl } from "./providerConfig";

/** Writes the store the way an install from before backups existed left it: one string under `provider-url`, nothing else. */
async function seedLegacy(indexedDB: IDBFactory, value: unknown): Promise<void> {
  const db = await new Promise<IDBDatabase>((resolve, reject) => {
    const r = indexedDB.open("om-app-config", 1);
    r.onupgradeneeded = () => { r.result.createObjectStore("config"); };
    r.onsuccess = () => { resolve(r.result); };
    r.onerror = () => { reject(new Error("open")); };
  });
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction("config", "readwrite");
    tx.objectStore("config").put(value, "provider-url");
    tx.oncomplete = () => { resolve(); };
    tx.onerror = () => { reject(new Error("write")); };
  });
  db.close();
}
/** Every raw entry of the store, for checking what is on disk. */
async function dump(indexedDB: IDBFactory): Promise<Record<string, unknown>> {
  const db = await new Promise<IDBDatabase>((resolve, reject) => {
    const r = indexedDB.open("om-app-config", 1);
    r.onsuccess = () => { resolve(r.result); };
    r.onerror = () => { reject(new Error("open")); };
  });
  const out: Record<string, unknown> = {};
  await new Promise<void>((resolve) => {
    const c = db.transaction("config", "readonly").objectStore("config").openCursor();
    c.onsuccess = () => { const cur = c.result; if (cur) { out[typeof cur.key === "string" ? cur.key : "?"] = cur.value; cur.continue(); } else resolve(); };
  });
  db.close();
  return out;
}

describe("provider configuration", () => {
  it("is unconfigured by default: no provider, never a guessed one", async () => {
    const indexedDB = new IDBFactory();
    expect(await resolveProviderConfig({ indexedDB })).toBeNull();
    expect(await createConfiguredProvider({ indexedDB })).toBeNull();
    expect(await resolveProviderConfig({ indexedDB, buildUrl: "" })).toBeNull();
  });

  it("uses the build default, and a stored user choice wins over it", async () => {
    const indexedDB = new IDBFactory();
    expect((await resolveProviderConfig({ indexedDB, buildUrl: "https://default.example" }))?.baseUrl).toBe("https://default.example");
    expect(await saveRelayUrl({ indexedDB }, "https://mine.example/")).toBe(true);
    expect((await resolveProviderConfig({ indexedDB, buildUrl: "https://default.example" }))?.baseUrl).toBe("https://mine.example");
    expect(await saveRelayUrl({ indexedDB }, undefined)).toBe(true); // clearing falls back to the default
    expect((await resolveProviderConfig({ indexedDB, buildUrl: "https://default.example" }))?.baseUrl).toBe("https://default.example");
  });

  it("refuses an unusable URL: http, credentials, a query that could carry a capability; nothing is stored or used", async () => {
    const indexedDB = new IDBFactory();
    for (const bad of ["http://relay.example", "https://u:p@relay.example", "https://relay.example/?c=1", "relay.example", "http://127.0.0.1:8788"]) {
      expect(await saveRelayUrl({ indexedDB }, bad), bad).toBe(false);
    }
    expect(await resolveProviderConfig({ indexedDB })).toBeNull();
    // a bad build default is ignored rather than trusted
    expect(await resolveProviderConfig({ indexedDB, buildUrl: "http://relay.example" })).toBeNull();
  });

  it("plain http works only for a loopback host in a build that opted in", async () => {
    const indexedDB = new IDBFactory();
    expect(await resolveProviderConfig({ indexedDB, buildUrl: "http://127.0.0.1:8788" })).toBeNull();
    expect((await resolveProviderConfig({ indexedDB, buildUrl: "http://127.0.0.1:8788", allowLoopbackHttp: true }))?.baseUrl).toBe("http://127.0.0.1:8788");
    expect(await resolveProviderConfig({ indexedDB, buildUrl: "http://relay.example", allowLoopbackHttp: true })).toBeNull();
  });

  it("builds an HTTP provider for a configured URL", async () => {
    const indexedDB = new IDBFactory();
    const p = await createConfiguredProvider({ indexedDB, buildUrl: "https://default.example" });
    expect(p?.baseUrl).toBe("https://default.example");
  });
});

describe("several delivery services in the stored config", () => {
  it("migrates an install with one stored address without losing it, and keeps the old key readable by an older build", async () => {
    const indexedDB = new IDBFactory();
    await seedLegacy(indexedDB, "https://mine.example");
    const before = await resolveHomeSet({ indexedDB });
    expect(before).toEqual({ source: "chosen", homes: [{ config: { baseUrl: "https://mine.example" }, leaving: false }] });
    // Nothing is rewritten by reading; the first change writes the new list AND keeps the old key pointing at the main service.
    expect(await dump(indexedDB)).toEqual({ "provider-url": "https://mine.example" });
    expect(await addBackupHome({ indexedDB }, "https://backup.example")).toBe("added");
    expect(await dump(indexedDB)).toEqual({ "provider-url": "https://mine.example", "homes-v1": [{ url: "https://mine.example" }, { url: "https://backup.example" }] });
    const after = await resolveHomeSet({ indexedDB });
    expect(after?.homes.map((h) => [h.config.baseUrl, h.leaving])).toEqual([["https://mine.example", false], ["https://backup.example", false]]);
    expect((await resolveProviderConfig({ indexedDB }))?.baseUrl).toBe("https://mine.example");
  });

  it("a build default stays the first service when a backup is added to it, and a backup needs a service to back up", async () => {
    const indexedDB = new IDBFactory();
    expect(await addBackupHome({ indexedDB }, "https://backup.example")).toBe("failed");
    const env = { indexedDB, buildUrl: "https://default.example" };
    expect(await addBackupHome(env, "https://backup.example")).toBe("added");
    const set = await resolveHomeSet(env);
    expect(set?.source).toBe("chosen");
    expect(set?.homes.map((h) => h.config.baseUrl)).toEqual(["https://default.example", "https://backup.example"]);
  });

  it("refuses a duplicate (also one being left), an unusable address and a list past its bound, changing nothing", async () => {
    const indexedDB = new IDBFactory();
    expect(await saveRelayUrl({ indexedDB }, "https://a.example")).toBe(true);
    expect(await addBackupHome({ indexedDB }, "https://a.example/")).toBe("already");
    expect(await addBackupHome({ indexedDB }, "http://b.example")).toBe("failed");
    expect(await addBackupHome({ indexedDB }, "https://u:p@b.example")).toBe("failed");
    for (let i = 1; i < MAX_HOMES; i++) expect(await addBackupHome({ indexedDB }, `https://b${String(i)}.example`)).toBe("added");
    expect(await addBackupHome({ indexedDB }, "https://one-too-many.example")).toBe("full");
    expect((await resolveHomeSet({ indexedDB }))?.homes).toHaveLength(MAX_HOMES);
  });

  it("marks a service as leaving: it moves behind the others, stays listed, and the last service can never be left", async () => {
    const indexedDB = new IDBFactory();
    await saveRelayUrl({ indexedDB }, "https://a.example");
    expect(await markHomeLeaving({ indexedDB }, "https://a.example")).toBe(false); // the only service
    await addBackupHome({ indexedDB }, "https://b.example");
    await addBackupHome({ indexedDB }, "https://c.example");
    expect(await markHomeLeaving({ indexedDB }, "https://nope.example")).toBe(false);
    expect(await markHomeLeaving({ indexedDB }, "https://a.example")).toBe(true);
    expect(await markHomeLeaving({ indexedDB }, "https://a.example")).toBe(true); // repeating is harmless
    let set = await resolveHomeSet({ indexedDB });
    expect(set?.homes.map((h) => [h.config.baseUrl, h.leaving])).toEqual([["https://b.example", false], ["https://c.example", false], ["https://a.example", true]]);
    expect((await dump(indexedDB))["provider-url"]).toBe("https://b.example"); // the old key follows the new main service
    expect(await markHomeLeaving({ indexedDB }, "https://b.example")).toBe(true);
    expect(await markHomeLeaving({ indexedDB }, "https://c.example")).toBe(false); // would leave nothing to deliver through
    set = await resolveHomeSet({ indexedDB });
    expect(set?.homes.filter((h) => !h.leaving).map((h) => h.config.baseUrl)).toEqual(["https://c.example"]);
  });

  it("choosing one address replaces the whole list; clearing removes both keys", async () => {
    const indexedDB = new IDBFactory();
    await saveRelayUrl({ indexedDB }, "https://a.example");
    await addBackupHome({ indexedDB }, "https://b.example");
    expect(await saveRelayUrl({ indexedDB }, "https://c.example")).toBe(true);
    expect((await resolveHomeSet({ indexedDB }))?.homes.map((h) => h.config.baseUrl)).toEqual(["https://c.example"]);
    expect(await saveRelayUrl({ indexedDB }, undefined)).toBe(true);
    expect(await dump(indexedDB)).toEqual({});
  });

  it("decoding survives a damaged store: bad entries are dropped, duplicates collapse, a list that is all leaving keeps its first as main", () => {
    const env = { indexedDB: new IDBFactory() };
    expect(decodeHomes([null, 7, { url: 3 }, { url: "http://bad.example" }, { url: "https://a.example", draining: true }, { url: "https://a.example/" }, { url: "https://b.example", draining: true }], undefined, env))
      .toEqual([{ url: "https://a.example" }, { url: "https://b.example", draining: true }]);
    expect(decodeHomes("not a list", undefined, env)).toEqual([]);
    expect(decodeHomes(undefined, 42, env)).toEqual([]);
    expect(decodeHomes(undefined, "https://old.example/", env)).toEqual([{ url: "https://old.example" }]);
  });

  it("the provider offers routes at every service not being left, collects from all, and stops offering a service the moment it is left", async () => {
    const indexedDB = new IDBFactory();
    await saveRelayUrl({ indexedDB }, "https://a.example");
    await addBackupHome({ indexedDB }, "https://b.example");
    await addBackupHome({ indexedDB }, "https://c.example");
    await markHomeLeaving({ indexedDB }, "https://c.example");
    const set = await resolveHomeSet({ indexedDB });
    if (set === null) throw new Error("no set");
    const provider = await createHomesProviderFor({ indexedDB }, set);
    const hosts = async (n: number): Promise<string[]> => [...new Set((await provider.allocateRoutes(n)).map((o) => o.relayUri))].sort();
    expect(await hosts(6)).toEqual(["https://a.example/", "https://b.example/"]); // never the one being left
    const undo = provider.leave("https://a.example");
    expect(await hosts(4)).toEqual(["https://b.example/"]); // gone at once, before any reconnect
    undo();
    expect(await hosts(6)).toEqual(["https://a.example/", "https://b.example/"]);
    provider.leave("https://a.example");
    provider.leave("https://b.example");
    expect(await provider.allocateRoutes(2)).toEqual([]); // nothing left to offer: Core refuses what needs a mailbox rather than being given a dead one
    provider.dispose();
  });
});
