// Service worker entry (built to a stable /sw.js, classic script, no imports at runtime).
// Update model: a new bundle is staged and verified in the background but stays "waiting";
// it activates only when a page sends `om.sw/activate` at a user-visible or idle boundary, never mid-session.
import { createShell, type ShellCache, type ShellStorage } from "./core";
import { idbKv } from "./idb-kv";
import { onNotificationClick, onPush, type PushDeps, type WindowLike } from "./push";
import { TRUST_ROOT } from "./trust-root";

interface ExtendableEventLike extends Event {
  waitUntil(p: Promise<unknown>): void;
}
interface FetchEventLike extends ExtendableEventLike {
  readonly request: Request;
  respondWith(r: Promise<Response>): void;
}
interface MessageEventLike extends ExtendableEventLike {
  readonly data: unknown;
}
interface PushEventLike extends ExtendableEventLike { readonly data?: unknown }
interface WindowClientLike { readonly visibilityState: string; postMessage(m: unknown): void; focus(): Promise<unknown> }
interface SwScope {
  readonly location: Location;
  readonly clients: { matchAll(o: { type: "window"; includeUncontrolled: boolean }): Promise<readonly WindowClientLike[]>; openWindow(url: string): Promise<unknown> };
  readonly registration: { showNotification(title: string, o: NotificationOptions): Promise<void> };
  addEventListener(type: "push" | "notificationclick", l: (e: PushEventLike) => void): void;
  readonly caches: CacheStorage;
  skipWaiting(): Promise<void>;
  addEventListener(type: "install" | "activate", l: (e: ExtendableEventLike) => void): void;
  addEventListener(type: "fetch", l: (e: FetchEventLike) => void): void;
  addEventListener(type: "message", l: (e: MessageEventLike) => void): void;
}
declare const __OM_BUNDLE_VERSION__: string;
const sw = self as unknown as SwScope;

const wrap = (c: Cache): ShellCache => ({
  put: (path, res) => c.put("/" + path, res),
  get: async (path) => c.match("/" + path),
  paths: async () => (await c.keys()).map((r) => new URL(r.url).pathname.slice(1)),
});
const storage: ShellStorage = {
  exclusive: (job) => navigator.locks.request("om-shell", job),
  open: async (name) => wrap(await sw.caches.open(name)),
  delete: async (name) => {
    await sw.caches.delete(name);
  },
  names: () => sw.caches.keys(),
};
const shell = createShell({
  storage,
  kv: idbKv,
  fetchFn: (path) => fetch("/" + path, { cache: "reload", credentials: "omit" }),
  trustRoot: TRUST_ROOT,
});

// A failed or refused stage rejects install: the previously active worker and its cache stay in service. A predecessor that
// serves a shell browsers refuse (0.0.0 and 0.0.1) is replaced at once: a phone stuck on an error page heals on its next try.
sw.addEventListener("install", (e) => {
  e.waitUntil(shell.stage().then(async () => { if (await shell.replacesBrokenShell()) await sw.skipWaiting(); }));
});
sw.addEventListener("activate", (e) => {
  // Recording the version also keeps it in the worker's bytes: a release that changes only the bundle still changes /sw.js,
  // which is what makes an installed copy fetch, verify and stage the new bundle.
  e.waitUntil(shell.activate().then(() => idbKv.set("worker-version", __OM_BUNDLE_VERSION__)));
});
sw.addEventListener("message", (e) => {
  const d = e.data;
  if (typeof d === "object" && d !== null && (d as { type?: unknown }).type === "om.sw/activate") e.waitUntil(sw.skipWaiting());
});
// Only same-origin GETs of precached bundle files (and navigations to the shell) are answered from cache.
// Everything else (API, relay, blob, cross-origin) is not intercepted and never cached.
sw.addEventListener("fetch", (e) => {
  const { request } = e;
  const url = new URL(request.url);
  if (request.method !== "GET" || url.origin !== sw.location.origin) return;
  e.respondWith(shell.respond(url.pathname, request.mode === "navigate").then((r) => r ?? fetch(request), () => fetch(request)));
});

// Push wake (ADR-0210): an empty push becomes one generic notice or an opaque wake of an open window. No fetch, no vault, no secret here.
const windowsOf = async (): Promise<readonly WindowLike[]> =>
  (await sw.clients.matchAll({ type: "window", includeUncontrolled: true })).map((c) => ({ visible: c.visibilityState === "visible", post: (m: unknown) => { c.postMessage(m); }, focus: () => c.focus() }));
const pushDeps: PushDeps = {
  windows: windowsOf,
  show: (title, o) => sw.registration.showNotification(title, { body: o.body, tag: o.tag }),
  read: async (k) => { const v = await idbKv.get(k); return typeof v === "string" ? v : null; },
  open: (url) => sw.clients.openWindow(url),
};
sw.addEventListener("push", (e) => { e.waitUntil(onPush(pushDeps)); });
sw.addEventListener("notificationclick", (e) => {
  (e as unknown as { notification?: { close(): void } }).notification?.close();
  e.waitUntil(onNotificationClick(pushDeps));
});
