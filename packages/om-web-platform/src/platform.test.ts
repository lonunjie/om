import { describe, expect, it, vi } from "vitest";
import { degrade, describeLimits, requireCapability } from "@om/platform-api";
import type { CapabilityState } from "@om/platform-api";
import type { RegistrationLike, WebEnv } from "./env";
import { createWebPlatform } from "./platform";

const spies = () => ({
  requestPermission: vi.fn(() => Promise.resolve("granted" as const)),
  getUserMedia: vi.fn(() => Promise.resolve({ getTracks: () => [] })),
  select: vi.fn(() => Promise.resolve([])),
  showOpenFilePicker: vi.fn(() => Promise.resolve([])),
  share: vi.fn(() => Promise.resolve()),
  writeText: vi.fn(() => Promise.resolve()),
  vibrate: vi.fn(() => true),
  subscribe: vi.fn(),
  getNotifications: vi.fn(() => Promise.resolve([])),
});

interface Opts {
  push?: boolean; share?: boolean; picker?: boolean; input?: boolean; getNotifications?: boolean;
  media?: boolean; contacts?: boolean; vibrate?: boolean;
}

function fakeEnv(o: Opts, s = spies()): WebEnv {
  const reg: RegistrationLike = { pushManager: { subscribe: s.subscribe, getSubscription: () => Promise.resolve(null) } };
  return {
    navigator: {
      onLine: true,
      ...(o.share ? { share: s.share } : {}),
      ...(o.vibrate ? { vibrate: s.vibrate } : {}),
      clipboard: { writeText: s.writeText },
      ...(o.media ? { mediaDevices: { getUserMedia: s.getUserMedia } } : {}),
      ...(o.push || o.getNotifications ? { serviceWorker: { ready: Promise.resolve(reg) } } : {}),
      ...(o.contacts ? { contacts: { select: s.select } } : {}),
    },
    ...(o.push ? { PushManager: {}, Notification: { permission: "default" as const, requestPermission: s.requestPermission } } : {}),
    ...(o.getNotifications ? { ServiceWorkerRegistration: { prototype: { getNotifications: s.getNotifications } } } : {}),
    ...(o.picker ? { showOpenFilePicker: s.showOpenFilePicker } : {}),
    ...(o.input ? { document: { visibilityState: "visible", createElement: () => { throw new Error("not called while probing"); } } } : {}),
    indexedDB: {},
    crypto: { subtle: {} },
  };
}

describe("UI-02: missing APIs", () => {
  const platform = createWebPlatform({});
  it("reports unavailable without throwing and degrades explicitly", () => {
    const report = platform.capabilities();
    for (const f of ["push", "share", "files", "camera", "microphone", "notificationRemoval", "haptics", "contactsLocal"] as const) {
      expect(report.families[f].state).toBe("unavailable");
      expect(requireCapability(report, f)).toMatchObject({ ok: false });
      expect(platform[f]().state).toBe("unavailable");
    }
    expect(report.backgroundPush).toBe("none");
    expect(report.localContacts).toBe("unavailable");
    expect(degrade(report, "share", { available: () => "native", unavailable: () => "none" })).toBe("none");
  });
  it("sensitive clipboard writes fail closed", async () => {
    const p = createWebPlatform(fakeEnv({}));
    const c = p.clipboard();
    expect(c.state).toBe("partial");
    if (c.state === "unavailable") throw new Error("expected api");
    expect(await c.api.write("secret", { sensitive: true })).toBe("sensitive_unsupported");
  });
});

type Row = [string, Opts, { push: CapabilityState; share: CapabilityState; files: CapabilityState; notificationRemoval: CapabilityState }];
const rows: Row[] = [
  ["macOS Chrome", { push: true, share: true, picker: true, input: true, getNotifications: true, media: true }, { push: "available", share: "available", files: "available", notificationRemoval: "partial" }],
  ["macOS Safari", { push: true, share: true, input: true, getNotifications: true, media: true }, { push: "available", share: "available", files: "partial", notificationRemoval: "partial" }],
  ["macOS Firefox", { push: true, input: true, getNotifications: true, media: true }, { push: "available", share: "unavailable", files: "partial", notificationRemoval: "partial" }],
  ["iOS Safari (tab)", { share: true, input: true, media: true, vibrate: false }, { push: "unavailable", share: "available", files: "partial", notificationRemoval: "unavailable" }],
  ["Android Chrome", { push: true, share: true, input: true, getNotifications: true, media: true, contacts: true, vibrate: true }, { push: "available", share: "available", files: "partial", notificationRemoval: "partial" }],
];

describe("probe table", () => {
  it.each(rows)("%s", (_name, opts, want) => {
    const report = createWebPlatform(fakeEnv(opts)).capabilities();
    expect(report.families.push.state).toBe(want.push);
    expect(report.families.share.state).toBe(want.share);
    expect(report.families.files.state).toBe(want.files);
    expect(report.families.notificationRemoval.state).toBe(want.notificationRemoval);
    expect(report.families.secureSurface.state).toBe("unavailable");
    expect(report.families.backupExclusion.state).toBe("unavailable");
    expect(report.families.inputPrivacy.state).toBe("partial");
    expect(report.backgroundPush).toBe(want.push === "available" ? "webpush" : "none");
    expect(report.localDiscovery).toBe("manual_known_endpoint");
    expect(report.families.externalTransport.state).toBe("unavailable");
    expect(describeLimits(report)).toEqual(expect.arrayContaining(["secure_surface_unavailable", "backup_exclusion_unavailable", "clipboard_sensitive_unsupported"]));
  });
});

describe("probing is side-effect free", () => {
  it("never calls permission-requesting or user-gesture APIs", () => {
    const s = spies();
    const p = createWebPlatform(fakeEnv({ push: true, share: true, picker: true, input: true, getNotifications: true, media: true, contacts: true, vibrate: true }, s));
    p.capabilities();
    for (const k of Object.keys(p) as (keyof typeof p)[]) if (k !== "capabilities") p[k]();
    for (const spy of Object.values(s)) expect(spy).not.toHaveBeenCalled();
  });
});
