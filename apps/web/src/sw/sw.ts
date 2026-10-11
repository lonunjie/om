// Service worker entry (built to a stable /sw.js, classic script, no imports at runtime).
// Update model: a new bundle is staged and verified in the background but stays "waiting";
// it activates only when a page sends `om.sw/activate` at a user-visible or idle boundary, never mid-session.
import { createShell, type ShellCache, type ShellStorage } from "./core";
import { idbKv } from "./idb-kv";
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
interface SwScope {
  readonly location: Location;
  readonly caches: CacheStorage;
  skipWaiting(): Promise<void>;
  addEventListener(type: "install" | "activate", l: (e: ExtendableEventLike) => void): void;
  addEventListener(type: "fetch", l: (e: FetchEventLike) => void): void;
  addEventListener(type: "message", l: (e: MessageEventLike) => void): void;
}
const sw = self as unknown as SwScope;

const wrap = (c: Cache): ShellCache => ({
  put: (path, res) => c.put("/" + path, res),
  get: async (path) => c.match("/" + path),
  paths: async () => (await c.keys()).map((r) => new URL(r.url).pathname.slice(1)),
});
const storage: ShellStorage = {
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

// A failed or refused stage rejects install: the previously active worker and its cache stay in service.
sw.addEventListener("install", (e) => {
  e.waitUntil(shell.stage());
});
sw.addEventListener("activate", (e) => {
  e.waitUntil(shell.activate());
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
