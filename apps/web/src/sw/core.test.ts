import { describe, expect, it } from "vitest";
import { buildManifest, serializeManifest } from "../../build/manifest";
import { createShell, type KV, type ShellCache, type ShellStorage } from "./core";

const enc = (s: string) => new TextEncoder().encode(s);
const memCache = (): ShellCache & { m: Map<string, string> } => {
  const m = new Map<string, string>();
  return {
    m,
    put: async (p, r) => void m.set(p, await r.text()),
    get: (p) => Promise.resolve(m.has(p) ? new Response(m.get(p)) : undefined),
    paths: () => Promise.resolve([...m.keys()]),
  };
};
const rig = () => {
  const caches = new Map<string, ReturnType<typeof memCache>>();
  const kvm = new Map<string, string>();
  const net = new Map<string, string>();
  const fetched: string[] = [];
  const storage: ShellStorage = {
    open: (n) => Promise.resolve(caches.get(n) ?? (caches.set(n, memCache()), caches.get(n) as ReturnType<typeof memCache>)),
    delete: (n) => Promise.resolve(void caches.delete(n)),
    names: () => Promise.resolve([...caches.keys()]),
  };
  const kv: KV = { get: (k) => Promise.resolve(kvm.get(k) ?? null), set: (k, v) => Promise.resolve(void kvm.set(k, v)) };
  const publish = async (version: string, files: Record<string, string>, tamper?: Record<string, string>) => {
    net.clear();
    const m = await buildManifest({ version, dev: true, files: Object.entries(files).map(([path, d]) => ({ path, data: enc(d) })) });
    net.set("bundle-manifest.json", serializeManifest(m));
    for (const [p, d] of Object.entries({ ...files, ...tamper })) net.set(p, d);
  };
  const fetchFn = (p: string): Promise<Response> => {
    fetched.push(p);
    const d = net.get(p);
    return Promise.resolve(d === undefined ? new Response("", { status: 404 }) : new Response(d));
  };
  const shell = createShell({ storage, kv, trustRoot: { verifiers: {}, allowUnsignedDev: true }, fetchFn });
  return { shell, caches, kvm, publish, fetched, storage, kv, fetchFn };
};
const V1 = { "index.html": "<html>1", "sw.js": "sw1", "assets/a.js": "a1" };

describe("service worker shell", () => {
  it("stages, activates, then serves precached files and the shell for navigations; passes everything else through", async () => {
    const r = rig();
    await r.publish("1.0.0", V1);
    expect(await r.shell.respond("/assets/a.js", false)).toBeUndefined(); // nothing active yet
    expect(await r.shell.stage()).toMatchObject({ version: "1.0.0", unsignedDev: true });
    await r.shell.activate();
    expect(await (await r.shell.respond("/assets/a.js", false))?.text()).toBe("a1");
    expect(await (await r.shell.respond("/chat/42", true))?.text()).toBe("<html>1");
    expect(await r.shell.respond("/api/v1/messages", false)).toBeUndefined();
    expect(await r.shell.respond("/bundle-manifest.json", false)).toBeUndefined();
    expect([...r.caches.values()].every((c) => !c.m.has("api/v1/messages"))).toBe(true);
  });
  it("keeps the old shell serving until activate, then drops it", async () => {
    const r = rig();
    await r.publish("1.0.0", V1);
    await r.shell.stage();
    await r.shell.activate();
    await r.publish("1.1.0", { ...V1, "assets/a.js": "a2" });
    await r.shell.stage();
    expect(await (await r.shell.respond("/assets/a.js", false))?.text()).toBe("a1"); // not swapped mid-session
    expect(r.caches.size).toBe(2);
    await r.shell.activate();
    expect(await (await r.shell.respond("/assets/a.js", false))?.text()).toBe("a2");
    expect(r.caches.size).toBe(1);
  });
  it("refuses a rolled-back bundle and a tampered download without touching the live cache", async () => {
    const r = rig();
    await r.publish("1.1.0", V1);
    await r.shell.stage();
    await r.shell.activate();
    await r.publish("1.0.0", V1);
    await expect(r.shell.stage()).rejects.toThrow(/rollback_refused/);
    expect(r.caches.size).toBe(1);
    await r.publish("1.2.0", V1, { "assets/a.js": "EVIL" });
    await expect(r.shell.stage()).rejects.toThrow(/hash_mismatch|size_mismatch/);
    expect(r.caches.size).toBe(1);
    expect(r.kvm.get("floor")).toBe("1.1.0");
  });
  it("fails closed on an unsigned manifest when the trust root does not allow dev", async () => {
    const r = rig();
    await r.publish("1.0.0", V1);
    const strict = createShell({ storage: r.storage, kv: r.kv, trustRoot: { verifiers: {} }, fetchFn: r.fetchFn });
    await expect(strict.stage()).rejects.toThrow(/unsigned/);
    expect(r.caches.size).toBe(0);
  });
});
