// SPDX-License-Identifier: MPL-2.0
// Client side of the optional wake channel (feature om.relay.wake.v1). While the app is on screen and the Home advertises
// the feature, one WebSocket per mailbox tells the client when something newer than it holds arrived, so it drains then
// instead of asking on a timer. Everything here is an optimisation: a socket that cannot open, drops, or goes quiet leaves
// the caller on its ordinary polling, and a missed wake costs at most the safety poll below. No message, no id and no
// capability ever travels in the URL; the receive capability is the first frame (home-api feature om.relay.wake.v1).
import {
  WAKE_CLOSE, WAKE_PING, WAKE_PONG, WAKE_SUBPROTOCOL, decodeWakeEvent, encodeWakeAuth, wakeLabel,
} from "@om/home-wire";

export type WakeSocketFactory = (url: string, protocols: string[]) => WebSocket;

export interface WakeCredentials { readonly queueId: Uint8Array; readonly receive: Uint8Array }

export interface WakeHubOptions {
  /** Home base URL (https or loopback http), without a trailing slash. */
  readonly base: string;
  /** `undefined` where the platform has no WebSocket: the hub then never wakes anything. */
  readonly webSocket: WakeSocketFactory | undefined;
  /** Whether the Home advertises the feature. Asked lazily, at most once per success and again after a failure. */
  readonly supported: () => Promise<boolean>;
  readonly credentials: (mailboxHex: string) => Promise<WakeCredentials>;
  readonly now?: () => number;
  /** Keepalive interval; a pong must follow within `pongMs`. */
  readonly pingMs?: number;
  readonly pongMs?: number;
  /** A live socket that has heard nothing newer still lets one real drain through this often (a lost wake costs at most this). */
  readonly safetyMs?: number;
  /** Time allowed from connecting to `ready` before the attempt counts as failed. */
  readonly connectMs?: number;
  /** Mailboxes served by a socket at once; any beyond this keep polling. */
  readonly maxSockets?: number;
}

export interface WakeStats { readonly sockets: number; readonly live: number; readonly wakes: number; readonly connects: number }

export interface WakeHub {
  /** Allowed to hold sockets (the app is on screen). Turning it off closes them all; on again, `watch` reopens as needed. */
  setActive(active: boolean): void;
  /** Declares interest in a mailbox and the newest sequence the caller already holds. Opens its socket if needed. Never throws, never waits. */
  watch(mailboxHex: string, after: number): void;
  /** True when a live socket vouches that nothing newer than `after` (and than the last drain) exists, so a drain would find nothing. */
  quiet(mailboxHex: string, after: number): boolean;
  /** Records that a drain returned items up to `seq` (0 when it found none). */
  drained(mailboxHex: string, seq: number): void;
  /** Called when a socket reports something newer than the caller holds. */
  onWake(listener: () => void): () => void;
  stats(): WakeStats;
  dispose(): void;
}

interface Link {
  readonly mailbox: string;
  ws: WebSocket | undefined;
  /** connecting: opened, awaiting `ready`. live: authenticated. down: no socket (backing off or refused). */
  state: "connecting" | "live" | "down";
  head: number;
  after: number;
  drainedUpTo: number;
  verifiedAt: number;
  failures: number;
  retry: ReturnType<typeof setTimeout> | undefined;
  ping: ReturnType<typeof setInterval> | undefined;
  pong: ReturnType<typeof setTimeout> | undefined;
  connect: ReturnType<typeof setTimeout> | undefined;
  /** The Home refused this mailbox's capability: leave it to polling, which reports the same thing. */
  gaveUp: boolean;
}

const BACKOFF_MS = [2000, 5000, 15_000, 30_000, 60_000] as const;
/** After this many attempts in a row that never reached `ready`, leave the mailbox to polling for a while. */
const MAX_FAILURES = 6;
const COOL_OFF_MS = 10 * 60_000;

export function createWakeHub(opts: WakeHubOptions): WakeHub {
  const now = opts.now ?? (() => Date.now());
  const pingMs = opts.pingMs ?? 45_000;
  const pongMs = opts.pongMs ?? 10_000;
  const safetyMs = opts.safetyMs ?? 5 * 60_000;
  const connectMs = opts.connectMs ?? 10_000;
  const maxSockets = opts.maxSockets ?? 16;
  const url = `${opts.base.replace(/^http/, "ws")}/v1/relay/wake`;
  const links = new Map<string, Link>();
  const listeners = new Set<() => void>();
  let active = false;
  let disposed = false;
  let advertised: { readonly value: boolean; readonly at: number } | undefined;
  let asking: Promise<boolean> | undefined;
  let wakes = 0;
  let connects = 0;

  const notify = (): void => {
    wakes++;
    for (const l of [...listeners]) { try { l(); } catch { /* a listener must not break the channel */ } }
  };

  function clearTimers(l: Link): void {
    if (l.retry !== undefined) clearTimeout(l.retry);
    if (l.ping !== undefined) clearInterval(l.ping);
    if (l.pong !== undefined) clearTimeout(l.pong);
    if (l.connect !== undefined) clearTimeout(l.connect);
    l.retry = l.ping = l.pong = l.connect = undefined;
  }

  function drop(l: Link, code?: number): void {
    const ws = l.ws;
    l.ws = undefined;
    clearTimers(l);
    if (ws) {
      ws.onopen = ws.onmessage = ws.onclose = ws.onerror = null;
      try { ws.close(code === undefined || code < 1000 || code === 1005 || code === 1006 ? 1000 : code); } catch { /* already closed */ }
    }
    l.state = "down";
  }

  const usable = (l: Link): boolean => active && !disposed && !l.gaveUp;

  function schedule(l: Link, immediate = false): void {
    if (!usable(l)) return;
    let wait: number;
    if (immediate) wait = 0;
    else if (l.failures >= MAX_FAILURES) { l.failures = 0; wait = COOL_OFF_MS; }
    else wait = (BACKOFF_MS[Math.min(l.failures, BACKOFF_MS.length - 1)] ?? 60_000) * (0.75 + Math.random() * 0.5);
    l.retry = setTimeout(() => { l.retry = undefined; if (usable(l)) open(l); }, wait);
  }

  function failed(l: Link, code?: number): void {
    const wasLive = l.state === "live";
    drop(l, code);
    if (code === WAKE_CLOSE.unauthorized) { l.gaveUp = true; return; }
    if (code === WAKE_CLOSE.superseded) { l.failures = 0; schedule(l, true); return; }
    if (code === WAKE_CLOSE.tooMany) l.failures = Math.max(l.failures, 3);
    else if (!wasLive) l.failures++;
    schedule(l);
  }

  function onFrame(l: Link, data: unknown): void {
    if (typeof data === "string") {
      if (data === WAKE_PONG && l.pong !== undefined) { clearTimeout(l.pong); l.pong = undefined; }
      return;
    }
    if (!(data instanceof ArrayBuffer)) return;
    let e;
    try { e = decodeWakeEvent(new Uint8Array(data)); } catch { failed(l, WAKE_CLOSE.badFrame); return; }
    if (e.seq > l.head) l.head = e.seq;
    if (e.kind === "ready") {
      if (l.connect !== undefined) { clearTimeout(l.connect); l.connect = undefined; }
      l.state = "live";
      l.failures = 0;
      l.verifiedAt = now();
      l.ping = setInterval(() => { ping(l); }, pingMs);
    }
    if (e.seq > Math.max(l.drainedUpTo, l.after)) notify();
  }

  function ping(l: Link): void {
    const ws = l.ws;
    if (!ws || l.state !== "live" || l.pong !== undefined) return;
    try { ws.send(WAKE_PING); } catch { failed(l); return; }
    l.pong = setTimeout(() => { l.pong = undefined; failed(l); }, pongMs);
  }

  function open(l: Link): void {
    const factory = opts.webSocket;
    if (!factory || !usable(l) || l.ws) return;
    void (async () => {
      let creds: WakeCredentials;
      try { creds = await opts.credentials(l.mailbox); } catch { l.failures++; schedule(l); return; }
      if (!usable(l) || l.ws) return;
      let ws: WebSocket;
      try { ws = factory(url, [WAKE_SUBPROTOCOL, wakeLabel(creds.queueId)]); } catch { l.failures++; schedule(l); return; }
      connects++;
      ws.binaryType = "arraybuffer";
      l.ws = ws;
      l.state = "connecting";
      l.connect = setTimeout(() => { l.connect = undefined; failed(l); }, connectMs);
      ws.onopen = () => { try { ws.send(encodeWakeAuth({ cap: creds.receive })); } catch { failed(l); } };
      ws.onmessage = (ev) => { if (l.ws === ws) onFrame(l, ev.data); };
      ws.onclose = (ev) => { if (l.ws === ws) failed(l, ev.code); };
      ws.onerror = () => { /* a close follows; nothing to add */ };
    })();
  }

  function isSupported(): Promise<boolean> {
    if (advertised && (advertised.value || now() - advertised.at < 10 * 60_000)) return Promise.resolve(advertised.value);
    asking ??= opts.supported().catch(() => false).then((v) => { advertised = { value: v, at: now() }; asking = undefined; return v; });
    return asking;
  }

  function ensure(l: Link): void {
    if (l.ws || l.retry !== undefined || !usable(l) || !opts.webSocket) return;
    void isSupported().then((ok) => { if (ok && usable(l) && !l.ws && l.retry === undefined) open(l); });
  }

  return {
    setActive(next) {
      if (next === active) return;
      active = next;
      if (!next) for (const l of links.values()) { drop(l); l.failures = 0; }
      else for (const l of links.values()) ensure(l);
    },
    watch(mailbox, after) {
      let l = links.get(mailbox);
      if (!l) {
        if (links.size >= maxSockets) return;
        l = { mailbox, ws: undefined, state: "down", head: 0, after, drainedUpTo: 0, verifiedAt: 0, failures: 0, retry: undefined, ping: undefined, pong: undefined, connect: undefined, gaveUp: false };
        links.set(mailbox, l);
      }
      if (after > l.after) l.after = after;
      ensure(l);
    },
    quiet(mailbox, after) {
      const l = links.get(mailbox);
      if (!l || l.state !== "live") return false;
      if (l.head > Math.max(after, l.drainedUpTo)) return false;
      return now() - l.verifiedAt < safetyMs;
    },
    drained(mailbox, seq) {
      const l = links.get(mailbox);
      if (!l) return;
      if (seq > l.drainedUpTo) l.drainedUpTo = seq;
      // A real drain that found nothing newer than the socket announced is as good as a fresh `ready`.
      if (l.state === "live" && l.head <= Math.max(l.drainedUpTo, l.after)) l.verifiedAt = now();
    },
    onWake(listener) {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
    stats() {
      let sockets = 0;
      let live = 0;
      for (const l of links.values()) { if (l.ws) sockets++; if (l.state === "live") live++; }
      return { sockets, live, wakes, connects };
    },
    dispose() {
      disposed = true;
      active = false;
      for (const l of links.values()) drop(l);
      links.clear();
      listeners.clear();
    },
  };
}
