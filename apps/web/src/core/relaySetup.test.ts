import { IDBFactory } from "fake-indexeddb";
import { admissionHash } from "@om/home-wire";
import { createIndexedDbSecretStore, readAdmission } from "@om/provider-http";
import { describe, expect, it, vi } from "vitest";
import { resolveProviderChoice } from "./providerConfig";
import { createRelaySetup, relayJustConnected } from "./relaySetup";

const doc = { schema_version: 1, provider_id: "mine", provider_kinds: ["relay"], durability_class: "durable", features: ["om.admission.v1"], api_versions: ["1"], health: "healthy" };
const urlOf = (i: RequestInfo | URL): string => (typeof i === "string" ? i : i instanceof URL ? i.href : i.url);

describe("first-run delivery-service setup over the real configuration", () => {
  it("shows a stable setup value, probes only a valid address, and stores address and matching secret together", async () => {
    const idb = new IDBFactory();
    const urls: string[] = [];
    const fetcher: typeof fetch = (i) => { urls.push(urlOf(i)); return Promise.resolve(new Response(JSON.stringify(doc), { status: 200, headers: { "content-type": "application/json" } })); };
    const reconnect = vi.fn();
    const env = { indexedDB: idb, fetch: fetcher };
    const s = createRelaySetup({ env, deployUrl: "https://deploy.example/start", reconnect });
    expect(s.deployUrl).toBe("https://deploy.example/start");

    const shown = await s.prepare();
    expect(await s.prepare()).toBe(shown);

    expect(await s.probe("http://mine.example")).toEqual({ kind: "invalid_url", reason: "not_https" });
    expect(urls).toEqual([]); // nothing was requested for an unacceptable address
    expect(await resolveProviderChoice(env)).toBeNull();

    const probed = await s.probe("https://mine.example/");
    expect(probed).toEqual({ kind: "ok", baseUrl: "https://mine.example", providerId: "mine", health: "healthy", admission: true });
    expect(await resolveProviderChoice(env)).toBeNull(); // probing stores nothing

    expect(relayJustConnected()).toBe(false);
    expect(await s.connect("https://mine.example")).toBe(true);
    expect(relayJustConnected()).toBe(true);
    expect(await resolveProviderChoice(env)).toEqual({ config: { baseUrl: "https://mine.example" }, source: "chosen" });
    const secret = await readAdmission(createIndexedDbSecretStore(idb), "https://mine.example");
    expect(await admissionHash(secret ?? new Uint8Array(0))).toBe(shown);
    expect(await readAdmission(createIndexedDbSecretStore(idb), "https://elsewhere.example")).toBeUndefined();
    s.done();
    expect(reconnect).toHaveBeenCalledTimes(1);
  });

  it("refuses to store an unacceptable address or one with no setup value behind it", async () => {
    const env = { indexedDB: new IDBFactory() };
    const s = createRelaySetup({ env, reconnect: () => undefined });
    expect(await s.connect("http://mine.example")).toBe(false);
    expect(await s.connect("https://mine.example")).toBe(false); // prepare() was never called: no secret to bind
    expect(await resolveProviderChoice(env)).toBeNull();
  });

  it("only an https setup link is offered", () => {
    const env = { indexedDB: new IDBFactory() };
    for (const bad of [undefined, "", "http://deploy.example", "javascript:alert(1)", "https://u:p@deploy.example", "not a url"]) {
      expect(createRelaySetup({ env, deployUrl: bad, reconnect: () => undefined }).deployUrl, String(bad)).toBeUndefined();
    }
  });
});
