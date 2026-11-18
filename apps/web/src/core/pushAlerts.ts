// The opt-in for alerts while OM is closed (ADR-0210): one switch per device. Nothing registers, and no permission is asked, until the person turns it on.
// The browser makes a push subscription for the Home's public key; the active Home is told its address (an opaque endpoint), and sends it an empty
// push when something arrives and nobody is connected. The service worker shows one generic notice (sw/push.ts). This file holds the decisions only;
// the browser objects are passed in so every way of declining is testable.
import type { PushClient, PushRegistration } from "@om/provider-http";

export interface SubscriptionLike {
  readonly endpoint: string;
  readonly options: { readonly applicationServerKey: ArrayBuffer | null };
  unsubscribe(): Promise<boolean>;
}
export interface RegistrationLike {
  readonly pushManager: {
    getSubscription(): Promise<SubscriptionLike | null>;
    subscribe(o: { readonly userVisibleOnly: true; readonly applicationServerKey: Uint8Array }): Promise<SubscriptionLike>;
  };
}

export interface AlertsEnv {
  /** The provider in use (`undefined` push when none); `null` when no Home is configured. */
  readonly provider: { readonly push: PushClient } | null;
  readonly serviceWorker: { readonly ready: Promise<RegistrationLike> } | undefined;
  readonly notification: { readonly permission: string; requestPermission(): Promise<string> } | undefined;
  readonly store: { get(k: string): string | null; set(k: string, v: string): void; remove(k: string): void };
  readonly isIos: boolean;
  /** Running as an installed app (display-mode standalone / navigator.standalone). */
  readonly standalone: boolean;
  /** Identity profiles on this device: one browser subscription is one endpoint for all of them, so more than one is not offered. */
  readonly profileCount: () => number;
  /** Where the service worker reads its notice (two short strings in the person's language). */
  readonly saveNotice: (json: string) => Promise<void>;
  readonly noticeText: { readonly title: string; readonly body: string };
}

export type AlertsAvailability = "ready" | "no_browser" | "needs_install" | "no_home" | "many_profiles";
export type EnableResult = "enabled" | "denied" | "refused" | "full" | "unreachable" | "failed" | "unavailable";
export interface AlertsStatus { readonly on: boolean; readonly permission: "granted" | "denied" | "default"; /** The person confirmed the cost of alerts at the Maximum profile. */ readonly maxOk: boolean }

export interface AlertsServices {
  availability(): Promise<AlertsAvailability>;
  status(): Promise<AlertsStatus>;
  enable(o: { readonly confirmedMaximum: boolean }): Promise<EnableResult>;
  disable(): Promise<void>;
  /** At each start, when on: renews the Home's registration (it lapses after 30 days) and picks up a subscription the browser replaced. */
  refresh(o: { readonly maximum: boolean }): Promise<void>;
}

const PREF = "om.alerts.v1";
export function matchesPushKey(subscription: SubscriptionLike, key: Uint8Array): boolean {
  const bound = subscription.options.applicationServerKey;
  if (bound === null) return false;
  const prior = new Uint8Array(bound);
  return prior.length === key.length && prior.every((b, i) => b === key[i]);
}

const readPref = (env: AlertsEnv): { on: boolean; maxOk: boolean } => {
  try {
    const v = JSON.parse(env.store.get(PREF) ?? "null") as { on?: unknown; maxOk?: unknown } | null;
    return v !== null && v.on === true ? { on: true, maxOk: v.maxOk === true } : { on: false, maxOk: false };
  } catch { return { on: false, maxOk: false }; }
};

export function createAlertsServices(env: AlertsEnv): AlertsServices {
  let revision = 0;
  let maximum = false;
  const permission = (): "granted" | "denied" | "default" => { const p = env.notification?.permission; return p === "granted" || p === "denied" ? p : "default"; };

  async function availability(): Promise<AlertsAvailability> {
    if (env.serviceWorker === undefined || env.notification === undefined) return "no_browser";
    if (env.isIos && !env.standalone) return "needs_install";
    if (env.provider === null || !(await env.provider.push.supported().catch(() => false))) return "no_home";
    return env.profileCount() > 1 ? "many_profiles" : "ready";
  }

  async function subscription(push: PushClient, valid: () => boolean): Promise<{ sub: SubscriptionLike; created: boolean } | undefined> {
    const reg = await env.serviceWorker?.ready;
    if (reg === undefined || !valid()) return undefined;
    const have = await reg.pushManager.getSubscription();
    const key = await push.publicKey();
    if (key === undefined || !valid()) return undefined;
    if (have !== null) {
      if (matchesPushKey(have, key)) return { sub: have, created: false };
      if (!await have.unsubscribe() || !valid()) return undefined;
    }
    return { sub: await reg.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: key }), created: true };
  }

  function allowed(confirmedMaximum: boolean): boolean {
    return env.profileCount() === 1 && permission() === "granted" && (!maximum || confirmedMaximum);
  }

  async function disable(): Promise<void> {
    revision++;
    env.provider?.push.pause();
    env.store.remove(PREF);
    try {
      const sub = await (await env.serviceWorker?.ready)?.pushManager.getSubscription();
      if (sub == null) return;
      await Promise.all([
        env.provider?.push.unregister(sub.endpoint).catch(() => undefined),
        sub.unsubscribe().catch(() => false),
      ]);
    } catch { /* the registration lapses by itself */ }
  }

  const fail = (reason: PushRegistration["reason"]): EnableResult => (reason === "refused" ? "refused" : reason === "full" ? "full" : reason === "unreachable" ? "unreachable" : "failed");

  return {
    availability,
    status() { const p = readPref(env); return Promise.resolve({ on: p.on, permission: permission(), maxOk: p.maxOk }); },
    async enable({ confirmedMaximum }) {
      const current = ++revision;
      // The permission prompt must come from the person's own tap, so it is the first thing awaited.
      if (env.notification === undefined || env.serviceWorker === undefined || env.provider === null || env.profileCount() > 1 || (env.isIos && !env.standalone)) return "unavailable";
      if (permission() === "denied") return "denied";
      if (permission() === "default" && (await env.notification.requestPermission().catch(() => "default")) !== "granted") return "denied";
      if ((await availability()) !== "ready") return "unavailable";
      const push = env.provider.push;
      const canRegister = (): boolean => allowed(confirmedMaximum);
      const eligible = (): boolean => revision === current && canRegister();
      let result: Awaited<ReturnType<typeof subscription>>;
      try {
        result = await subscription(push, eligible);
      } catch { return "failed"; }
      if (result === undefined) return eligible() ? "failed" : "unavailable";
      const { sub, created } = result;
      if (!eligible()) {
        if (created) await sub.unsubscribe().catch(() => false);
        return "unavailable";
      }
      const r = await push.register(sub.endpoint, canRegister).catch((): PushRegistration => ({ registered: 0, failed: 1, reason: "unreachable" }));
      if (!eligible()) return "unavailable";
      // No mailbox is open yet (just started): the provider keeps the endpoint and registers each mailbox as it opens, and the next start renews it.
      if (r.registered === 0 && (r.failed > 0 || r.reason === "unsupported")) {
        push.pause();
        if (created) await sub.unsubscribe().catch(() => false);
        return fail(r.reason);
      }
      env.store.set(PREF, JSON.stringify({ on: true, maxOk: confirmedMaximum }));
      await env.saveNotice(JSON.stringify({ title: env.noticeText.title, body: env.noticeText.body })).catch(() => undefined);
      return "enabled";
    },
    disable,
    async refresh({ maximum: strict }) {
      const current = ++revision;
      maximum = strict;
      const p = readPref(env);
      const push = env.provider?.push;
      if (p.on && env.profileCount() > 1) { await disable(); return; }
      if (push === undefined) return;
      const canRegister = (): boolean => allowed(p.maxOk);
      if (!p.on || !canRegister()) {
        push.pause();
        return;
      }
      try {
        const eligible = (): boolean => revision === current && canRegister();
        const result = await subscription(push, eligible);
        if (result !== undefined && eligible()) await push.register(result.sub.endpoint, canRegister);
      } catch { /* retried at the next start */ }
    },
  };
}
