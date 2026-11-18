// Wires the alerts decisions (pushAlerts.ts) to this browser: the service worker, the Notification API, a per-device preference and the notice the
// worker shows. Nothing here runs until the person opens Settings > Alerts (or alerts were already on, at start).
import type { PushClient } from "@om/provider-http";
import type { AlertsServices } from "@om/ui";
import { alertsNotice } from "../copy/en";
import { idbKv } from "../sw/idb-kv";
import { NOTICE_KEY } from "../sw/push";
import { createAlertsServices, type RegistrationLike } from "./pushAlerts";

type Win = typeof globalThis & { navigator: Navigator & { standalone?: boolean }; Notification?: { readonly permission: string; requestPermission(): Promise<string> } };

/** The alerts for this page, or `undefined` where there is no page (tests, workers). `profileCount` is read each time it matters. */
export function browserAlerts(provider: { readonly push: PushClient } | null, profileCount: () => number): AlertsServices | undefined {
  const w = globalThis as Win;
  if (typeof w.navigator === "undefined") return undefined;
  const nav = w.navigator;
  const ua = nav.userAgent;
  const store = (() => { try { return w.localStorage; } catch { return undefined; } })();
  const sw = "serviceWorker" in nav && typeof PushManager !== "undefined" ? { ready: nav.serviceWorker.ready as unknown as Promise<RegistrationLike> } : undefined;
  return createAlertsServices({
    provider,
    serviceWorker: sw,
    notification: typeof w.Notification === "undefined" ? undefined : w.Notification,
    store: {
      get: (k) => { try { return store?.getItem(k) ?? null; } catch { return null; } },
      set: (k, v) => { try { store?.setItem(k, v); } catch { /* the choice lasts until the page closes only */ } },
      remove: (k) => { try { store?.removeItem(k); } catch { /* nothing to remove */ } },
    },
    isIos: /iPad|iPhone|iPod/.test(ua) || (nav.platform === "MacIntel" && nav.maxTouchPoints > 1),
    standalone: nav.standalone === true || (typeof w.matchMedia === "function" && w.matchMedia("(display-mode: standalone)").matches),
    profileCount,
    saveNotice: async (json) => { await idbKv.set(NOTICE_KEY, json); },
    noticeText: alertsNotice,
  });
}
