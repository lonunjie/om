import { IDBFactory } from "fake-indexeddb";
import { describe, expect, it } from "vitest";
import { createConfiguredProvider, resolveProviderConfig, saveRelayUrl } from "./providerConfig";

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
