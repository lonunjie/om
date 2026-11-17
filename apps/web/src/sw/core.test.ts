import { describe, expect, it } from "vitest";
import { buildManifest, serializeManifest } from "../../build/manifest";
import { createShell, servesBrokenShell, type KV, type ShellCache, type ShellStorage } from "./core";

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
  let lock: Promise<unknown> = Promise.resolve();
  const storage: ShellStorage = {
    exclusive: <T>(job: () => Promise<T>): Promise<T> => {
      const next = lock.then(job); lock = next.catch(() => undefined); return next;
    },
    open: (n) => Promise.resolve(caches.get(n) ?? (caches.set(n, memCache()), caches.get(n) as ReturnType<typeof memCache>)),
    delete: (n) => Promise.resolve(void caches.delete(n)),
    names: () => Promise.resolve([...caches.keys()]),
  };
  const kv: KV = { delete: (k) => Promise.resolve(void kvm.delete(k)), get: (k) => Promise.resolve(kvm.get(k) ?? null), set: (k, v) => Promise.resolve(void kvm.set(k, v)) };
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
  const shell = createShell({ storage, kv, trustRoot: { roots: { keys: {}, threshold: 1 }, allowUnsignedDev: true }, fetchFn });
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
  it("does not fetch the lazy groups module at install: the install stays the size of the base bundle (D65)", async () => {
    const r = rig();
    const groups = "assets/om_wasm_groups-Ab1.wasm";
    await r.publish("1.0.0", { ...V1, "assets/om_wasm-Cd2.wasm": "base", [groups]: "groups engine" });
    await r.shell.stage();
    await r.shell.activate();
    expect(r.fetched).not.toContain(groups);
    expect(r.fetched).toContain("assets/om_wasm-Cd2.wasm");
    expect(await r.shell.respond("/" + groups, false)).toBeUndefined(); // goes to the network; the page verifies it
  });
  it("caches a redirected response as a plain one, so a navigation can be answered with it", async () => {
    const r = rig();
    await r.publish("1.0.0", V1);
    const stored: boolean[] = [];
    const shell = createShell({
      storage: { ...r.storage, open: async (n) => { const c = await r.storage.open(n); return { ...c, put: (p, res) => { stored.push(res.redirected); return c.put(p, res); } }; } },
      kv: r.kv,
      trustRoot: { roots: { keys: {}, threshold: 1 }, allowUnsignedDev: true },
      // The host answers /index.html with a redirect to /.
      fetchFn: async (p) => { const res = await r.fetchFn(p); return p === "index.html" ? Object.defineProperty(res, "redirected", { value: true }) : res; },
    });
    await shell.stage();
    await shell.activate();
    expect(stored.length).toBeGreaterThan(0);
    expect(stored.every((x) => !x)).toBe(true);
    expect(await (await shell.respond("/", true))?.text()).toBe("<html>1");
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
    const strict = createShell({ storage: r.storage, kv: r.kv, trustRoot: { roots: { keys: {}, threshold: 1 } }, fetchFn: r.fetchFn });
    await expect(strict.stage()).rejects.toThrow(/unsigned/);
    expect(r.caches.size).toBe(0);
  });
});

describe("a worker that serves a shell browsers refuse", () => {
  it("is recognised by its recorded version", () => {
    expect(servesBrokenShell(null)).toBe(true); // no version recorded: 0.0.0
    expect(servesBrokenShell("0.0.1")).toBe(true);
    expect(servesBrokenShell("0.0.2")).toBe(false);
    expect(servesBrokenShell("0.0.10")).toBe(false);
    expect(servesBrokenShell("0.1.0")).toBe(false);
    expect(servesBrokenShell("1.0.0")).toBe(false);
    expect(servesBrokenShell("dev-build")).toBe(false); // free-form dev and e2e versions are never treated as broken
  });

  it("is replaced at once, but a first install and a working predecessor are not", async () => {
    const r = rig();
    await r.publish("1.0.0", V1);
    expect(await r.shell.replacesBrokenShell()).toBe(false); // nothing active yet: a first install
    await r.shell.stage();
    await r.shell.activate();
    expect(await r.shell.replacesBrokenShell()).toBe(true); // active, and no worker-version was ever recorded
    r.kvm.set("worker-version", "0.0.1");
    expect(await r.shell.replacesBrokenShell()).toBe(true);
    r.kvm.set("worker-version", "0.0.2");
    expect(await r.shell.replacesBrokenShell()).toBe(false);
  });
});

describe("immutable shell staging", () => {
  it("keeps an identical active manifest available after an interrupted restage", async () => {
    const r = rig();
    await r.publish("1.0.0", V1);
    const first = await r.shell.stage(); await r.shell.activate();
    const worker = createShell({ storage: r.storage, kv: r.kv,
      trustRoot: { roots: { keys: {}, threshold: 1 }, allowUnsignedDev: true },
      fetchFn: (p) => p === "index.html" ? Promise.reject(new Error("offline")) : r.fetchFn(p),
    });
    await expect(worker.stage()).rejects.toThrow("offline");
    expect(r.caches.has(first.cacheName)).toBe(true);
    expect(await (await r.shell.respond("/chat/offline", true))?.text()).toBe(V1["index.html"]);
    const second = await r.shell.stage();
    expect(second.cacheName).not.toBe(first.cacheName);
    await r.shell.activate();
    expect(await (await r.shell.respond("/chat/offline", true))?.text()).toBe(V1["index.html"]);
  });

  it.each([0, 1, 3])("preserves the shell when interrupted after %i writes", async (writes) => {
    const r = rig(); await r.publish("1.0.0", V1);
    const active = await r.shell.stage(); await r.shell.activate();
    let count = 0;
    const worker = createShell({
      storage: { ...r.storage, open: async (n) => {
        const c = await r.storage.open(n);
        return { ...c, put: async (p, res) => {
          if (count++ === writes) throw new Error("interrupted");
          await c.put(p, res);
        } };
      } },
      kv: { ...r.kv, set: async (k, v) => {
        if (writes === 3 && k.startsWith("entry:")) throw new Error("interrupted");
        await r.kv.set(k, v);
      } },
      fetchFn: r.fetchFn, trustRoot: { roots: { keys: {}, threshold: 1 }, allowUnsignedDev: true },
    });
    await expect(worker.stage()).rejects.toThrow("interrupted");
    expect(r.kvm.get("active")).toBe(active.cacheName);
    expect(await (await r.shell.respond("/offline", true))?.text()).toBe(V1["index.html"]);
  });

  it("serializes concurrent workers' stage and activation without deleting their live shell", async () => {
    const r = rig(); await r.publish("1.0.0", V1);
    const worker = createShell({ storage: r.storage, kv: r.kv, fetchFn: r.fetchFn,
      trustRoot: { roots: { keys: {}, threshold: 1 }, allowUnsignedDev: true } });
    const [a, b] = await Promise.all([r.shell.stage(), worker.stage()]);
    expect(a.cacheName).not.toBe(b.cacheName);
    await Promise.all([r.shell.activate(), worker.activate()]);
    expect(r.caches.has(r.kvm.get("active") ?? "")).toBe(true);
    expect(await (await r.shell.respond("/offline", true))?.text()).toBe(V1["index.html"]);
    expect(await (await worker.respond("/offline", true))?.text()).toBe(V1["index.html"]);
  });
});

it("retries the active pointer when activation retires a cache during navigation", async () => {
  const r = rig(); await r.publish("1.0.0", V1);
  const old = await r.shell.stage(); await r.shell.activate();
  await r.publish("1.1.0", { ...V1, "index.html": "new shell" });
  await r.shell.stage();
  let switchNow = true;
  const reader = createShell({ storage: { ...r.storage, open: async (name) => {
    if (switchNow && name === old.cacheName) { switchNow = false; await r.shell.activate(); }
    return r.storage.open(name);
  } }, kv: r.kv, fetchFn: r.fetchFn,
    trustRoot: { roots: { keys: {}, threshold: 1 }, allowUnsignedDev: true } });
  expect(await (await reader.respond("/offline", true))?.text()).toBe("new shell");
});
