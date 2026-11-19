// The opt-in for alerts while OM is closed (ADR-0210): what the page does before it registers anything, and every way it can decline.
import { describe, expect, it, vi } from "vitest";
import { toBase64Url } from "@om/home-wire";
import { createPushClient, type PushClient } from "@om/provider-http";
import { createAlertsServices, type AlertsEnv, type SubscriptionLike } from "./pushAlerts";
import { retireBrowserPush } from "./alertsHost";

const KEY = new Uint8Array(65).fill(4);
function rig(over: Partial<{ permission: string; ask: string; ios: boolean; standalone: boolean; profiles: number; supported: boolean; hasHome: boolean; register: { registered: number; failed: number; reason: string | undefined }; key: Uint8Array | undefined; existing: boolean; existingKey: Uint8Array<ArrayBuffer>; browser: boolean; subscribeThrows: boolean; push: PushClient }> = {}) {
  const o = { permission: "default", ask: "granted", ios: false, standalone: true, profiles: 1, supported: true, hasHome: true, register: { registered: 2, failed: 0, reason: undefined as string | undefined }, key: KEY as Uint8Array | undefined, existing: false, browser: true, subscribeThrows: false, ...over };
  const log: string[] = [];
  const store = new Map<string, string>();
  const notice: string[] = [];
  let sub: SubscriptionLike | null = o.existing ? { endpoint: "https://web.push.apple.com/old", options: { applicationServerKey: (over.existingKey ?? KEY).buffer }, unsubscribe: () => { log.push("browser-unsub"); sub = null; return Promise.resolve(true); } } : null;
  let permission = o.permission;
  const env: AlertsEnv = {
    provider: o.hasHome ? { push: over.push ?? {
      supported: () => Promise.resolve(o.supported), publicKey: () => Promise.resolve(o.key),
      register: (e: string) => { log.push(`register ${e}`); return Promise.resolve(o.register as never); },
      unregister: (e: string) => { log.push(`unregister ${e}`); return Promise.resolve(); }, opened: () => undefined, pause: () => undefined,
    } } : null,
    serviceWorker: o.browser ? { ready: Promise.resolve({ pushManager: {
      getSubscription: () => Promise.resolve(sub),
      subscribe: (opts) => { log.push(`subscribe userVisibleOnly=${String(opts.userVisibleOnly)} key=${String(opts.applicationServerKey.length)}`); if (o.subscribeThrows) return Promise.reject(new Error("x")); sub = { endpoint: "https://web.push.apple.com/new", options: { applicationServerKey: new Uint8Array(opts.applicationServerKey).buffer }, unsubscribe: () => { log.push("browser-unsub"); sub = null; return Promise.resolve(true); } }; return Promise.resolve(sub); },
    } }) } : undefined,
    notification: o.browser ? { get permission() { return permission; }, requestPermission: () => { log.push("ask"); permission = o.ask; return Promise.resolve(o.ask); } } : undefined,
    store: { get: (k) => store.get(k) ?? null, set: (k, v) => { store.set(k, v); }, remove: (k) => { store.delete(k); } },
    isIos: o.ios, standalone: o.standalone, profileCount: () => o.profiles,
    saveNotice: (t) => { notice.push(t); return Promise.resolve(); },
    noticeText: { title: "OM", body: "New message" },
  };
  return { svc: createAlertsServices(env), log, store, notice, profiles: (n: number) => { o.profiles = n; } };
}

describe("availability", () => {
  it("is ready when the browser, the Home and a single identity allow it", async () => { expect(await rig().svc.availability()).toBe("ready"); });
  it("says why not, in this order: no browser support, iOS outside the Home Screen app, no Home, a Home without the feature, several identities", async () => {
    expect(await rig({ browser: false }).svc.availability()).toBe("no_browser");
    expect(await rig({ ios: true, standalone: false }).svc.availability()).toBe("needs_install");
    expect(await rig({ hasHome: false }).svc.availability()).toBe("no_home");
    expect(await rig({ supported: false }).svc.availability()).toBe("no_home");
    expect(await rig({ profiles: 2 }).svc.availability()).toBe("many_profiles");
  });
});

describe("enable", () => {
  it("asks permission first, subscribes with the Home's key, registers the endpoint, and stores the choice and the notice", async () => {
    const r = rig();
    expect(await r.svc.enable({ confirmedMaximum: false })).toBe("enabled");
    expect(r.log).toEqual(["ask", "subscribe userVisibleOnly=true key=65", "register https://web.push.apple.com/new"]);
    expect(JSON.parse(r.store.get("om.alerts.v1") as string)).toEqual({ on: true, maxOk: false });
    expect(r.notice).toEqual([JSON.stringify({ title: "OM", body: "New message" })]);
    expect(await r.svc.status()).toEqual({ on: true, permission: "granted", maxOk: false });
  });
  it("registers nothing when permission is refused, and never asks again once it is blocked", async () => {
    const r = rig({ ask: "denied" });
    expect(await r.svc.enable({ confirmedMaximum: false })).toBe("denied");
    expect(r.log).toEqual(["ask"]);
    expect(r.store.size).toBe(0);
    const blocked = rig({ permission: "denied" });
    expect(await blocked.svc.enable({ confirmedMaximum: false })).toBe("denied");
    expect(blocked.log).toEqual([]);
  });
  it("undoes the browser subscription when the Home does not take it", async () => {
    for (const [reason, want] of [["refused", "refused"], ["full", "full"], ["unreachable", "unreachable"], ["unsupported", "failed"]] as const) {
      const r = rig({ register: { registered: 0, failed: 2, reason } });
      expect(await r.svc.enable({ confirmedMaximum: false }), reason).toBe(want);
      expect(r.log.at(-1)).toBe("browser-unsub");
      expect(r.store.size).toBe(0);
    }
  });
  it("is on even when no mailbox is open yet: the provider registers each as it opens", async () => {
    const r = rig({ register: { registered: 0, failed: 0, reason: undefined } });
    expect(await r.svc.enable({ confirmedMaximum: false })).toBe("enabled");
    expect(r.store.size).toBe(1);
  });
  it("fails cleanly without a usable key or when the browser will not subscribe", async () => {
    expect(await rig({ key: undefined }).svc.enable({ confirmedMaximum: false })).toBe("failed");
    expect(await rig({ subscribeThrows: true }).svc.enable({ confirmedMaximum: false })).toBe("failed");
  });
  it("reuses a subscription the browser already has", async () => {
    const r = rig({ existing: true });
    expect(await r.svc.enable({ confirmedMaximum: false })).toBe("enabled");
    expect(r.log).toEqual(["ask", "register https://web.push.apple.com/old"]);
  });
  it("subscribes against the selected Home's key when an old subscription has a different key", async () => {
    const r = rig({ existing: true, existingKey: new Uint8Array(65).fill(6) });
    expect(await r.svc.enable({ confirmedMaximum: false })).toBe("enabled");
    expect(r.log).toEqual(["ask", "browser-unsub", "subscribe userVisibleOnly=true key=65", "register https://web.push.apple.com/new"]);
  });
  it("is not attempted where it is not available", async () => {
    const r = rig({ profiles: 2 });
    expect(await r.svc.enable({ confirmedMaximum: false })).toBe("unavailable");
    expect(r.log).toEqual([]);
  });
});

describe("disable and refresh", () => {
  it("withdraws only the subscription bound to the previous Home's key", async () => {
    const unregister = vi.fn<PushClient["unregister"]>(() => Promise.resolve());
    const push: PushClient = {
      unregister, publicKey: () => Promise.resolve(KEY), supported: () => Promise.resolve(true),
      pause: () => undefined, opened: () => undefined,
      register: () => Promise.resolve({ registered: 0, failed: 0, reason: undefined }),
    };
    let bound = new Uint8Array(65).fill(6);
    vi.stubGlobal("navigator", { serviceWorker: { ready: Promise.resolve({ pushManager: {
      getSubscription: () => Promise.resolve({ endpoint: "https://web.push.apple.com/old", options: { applicationServerKey: bound.buffer } }),
    } }) } });
    try {
      await retireBrowserPush(push);
      expect(unregister).not.toHaveBeenCalled();
      bound = KEY;
      await retireBrowserPush(push);
      expect(unregister).toHaveBeenCalledExactlyOnceWith("https://web.push.apple.com/old");
    } finally { vi.unstubAllGlobals(); }
  });

  it("withdraws a subscription when another profile is added", async () => {
    const r = rig();
    await r.svc.enable({ confirmedMaximum: false });
    r.log.length = 0;
    r.profiles(2);
    await r.svc.refresh({ maximum: false });
    expect(r.log).toEqual(["unregister https://web.push.apple.com/new", "browser-unsub"]);
    expect((await r.svc.status()).on).toBe(false);
  });
  it("pauses registration for newly opened mailboxes and still allows immediate opt-out", async () => {
    const fetch = vi.fn<typeof globalThis.fetch>((_, init) => Promise.resolve(init?.method === "GET"
      ? Response.json({ key: toBase64Url(KEY) })
      : new Response(null, { status: 204 })));
    const cap = new Uint8Array(32).fill(1);
    const push = createPushClient({ base: "https://home.example", http: { fetch, timeoutMs: 1000 },
      features: () => Promise.resolve(["om.relay.push.v1"]), receiveCaps: () => Promise.resolve([cap]),
    });
    const r = rig({ push, existing: true, permission: "granted" });
    expect(await r.svc.enable({ confirmedMaximum: false })).toBe("enabled");
    await r.svc.refresh({ maximum: true });
    push.opened(new Uint8Array(32).fill(2));
    expect(fetch.mock.calls.map(([, init]) => init?.method).filter((m) => m !== "GET")).toEqual(["PUT"]);
    await r.svc.disable();
    expect(fetch.mock.calls.map(([, init]) => init?.method).filter((m) => m !== "GET")).toEqual(["PUT", "DELETE"]);
    expect(r.log).toEqual(["browser-unsub"]);
  });
  it("renews the registration at each start with the browser's own subscription, once per mailbox (ADR-0210)", async () => {
    const fetch = vi.fn<typeof globalThis.fetch>((_, init) => Promise.resolve(init?.method === "GET"
      ? Response.json({ key: toBase64Url(KEY) })
      : new Response(null, { status: 204 })));
    const caps = [new Uint8Array(32).fill(1), new Uint8Array(32).fill(2)];
    const push = createPushClient({ base: "https://home.example", http: { fetch, timeoutMs: 1000 },
      features: () => Promise.resolve(["om.relay.push.v1"]), receiveCaps: () => Promise.resolve(caps),
    });
    const puts = (): number => fetch.mock.calls.filter(([, init]) => init?.method === "PUT").length;
    const r = rig({ push, permission: "granted" });
    expect(await r.svc.enable({ confirmedMaximum: false })).toBe("enabled");
    expect(puts()).toBe(2);
    await r.svc.refresh({ maximum: false });
    expect(puts()).toBe(4);
    expect(r.log.filter((l) => l.startsWith("subscribe"))).toHaveLength(1);
  });
  it("disable tells the Home, drops the browser subscription and forgets the choice", async () => {
    const r = rig();
    await r.svc.enable({ confirmedMaximum: false });
    r.log.length = 0;
    await r.svc.disable();
    expect(r.log).toEqual(["unregister https://web.push.apple.com/new", "browser-unsub"]);
    expect(r.store.size).toBe(0);
    expect((await r.svc.status()).on).toBe(false);
  });
  it("refresh renews the registration only when it is on, and not at Maximum without the person's confirmation", async () => {
    const off = rig();
    await off.svc.refresh({ maximum: false });
    expect(off.log).toEqual([]);
    const on = rig();
    await on.svc.enable({ confirmedMaximum: false });
    on.log.length = 0;
    await on.svc.refresh({ maximum: false });
    expect(on.log).toEqual(["register https://web.push.apple.com/new"]);
    on.log.length = 0;
    await on.svc.refresh({ maximum: true });
    expect(on.log).toEqual([]);
    expect(await on.svc.status()).toMatchObject({ on: true, maxOk: false });
    const max = rig();
    await max.svc.enable({ confirmedMaximum: true });
    max.log.length = 0;
    await max.svc.refresh({ maximum: true });
    expect(max.log).toEqual(["register https://web.push.apple.com/new"]);
  });
});
