// SPDX-License-Identifier: MPL-2.0
// Client side of the optional push wake (om.relay.push.v1, ADR-0210). One browser push endpoint is registered at the person's own Home for
// every mailbox this install holds there, so the Home can send that endpoint an empty wake when something arrives and nobody is connected.
// The receive capability travels as the bearer and the owner's admission secret in its header; the endpoint is the only thing in the body.
import { FEATURES, PUSH_PATHS, fromBase64Url, parsePushRequest } from "@om/home-wire";
import { exchange, type HttpOptions } from "./http";

/** Why a registration did not (fully) happen, in the words the screen needs. */
export type PushRefusal = "unsupported" | "refused" | "full" | "unreachable";

export interface PushRegistration {
  readonly registered: number;
  readonly failed: number;
  /** Set when nothing was registered although something was to be: the reason of the first failure. */
  readonly reason: PushRefusal | undefined;
}

export interface PushClient {
  /** The Home advertises om.relay.push.v1. */
  supported(): Promise<boolean>;
  /** The Home's public signing key (65-byte P-256 point), the `applicationServerKey` of the browser subscription; `undefined` when unavailable or malformed. */
  publicKey(): Promise<Uint8Array | undefined>;
  /** Registers `endpoint` for every mailbox held here and for each one opened while enabled. */
  register(endpoint: string, allowed?: () => boolean): Promise<PushRegistration>;
  /** Stop scheduling puts. Existing registrations retain their original lease. */
  pause(): void;
  /** Removes `endpoint` from every mailbox held here. Never throws; a Home that is unreachable forgets it when the registration lapses. */
  unregister(endpoint: string): Promise<void>;
  /** A mailbox was just opened: registers the kept endpoint for it, if any. Never throws. */
  opened(receiveCap: Uint8Array): void;
}

export interface PushClientOptions {
  readonly base: string;
  readonly http: HttpOptions;
  /** The receive capability of every mailbox held at this Home. */
  readonly receiveCaps: () => Promise<readonly Uint8Array[]>;
  /** The Home's advertised features, or `undefined` when its capabilities cannot be read. */
  readonly features: () => Promise<readonly unknown[] | undefined>;
}

const JSON_HEADERS = { "content-type": "application/json" } as const;

export function createPushClient(opts: PushClientOptions): PushClient {
  let kept: { endpoint: string; allowed: () => boolean } | undefined;
  let revision = 0;
  const pause = (): void => { revision++; kept = undefined; };
  const supported = async (): Promise<boolean> => (await opts.features())?.includes(FEATURES.relayPush) === true;
  const bodyOf = (endpoint: string): Uint8Array => new TextEncoder().encode(JSON.stringify({ endpoint }));

  async function put(endpoint: string, cap: Uint8Array): Promise<PushRefusal | undefined> {
    const r = await exchange(opts.http, `${opts.base}${PUSH_PATHS.subscription}`, { method: "PUT", body: bodyOf(endpoint), bearer: cap, headers: JSON_HEADERS });
    if (r.kind === "ok") return undefined;
    if (r.kind === "network") return "unreachable";
    if (r.kind === "home") return r.error.name === "quota" || r.error.name === "rate_limited" ? "full" : "refused";
    return r.status === 429 ? "full" : r.status === 404 ? "unsupported" : "unreachable";
  }

  return {
    supported,
    async publicKey() {
      const r = await exchange(opts.http, `${opts.base}${PUSH_PATHS.key}`, { method: "GET" });
      if (r.kind !== "ok") return undefined;
      try {
        const k = (JSON.parse(new TextDecoder().decode(r.body)) as { key?: unknown }).key;
        if (typeof k !== "string") return undefined;
        const raw = fromBase64Url(k);
        return raw.length === 65 && raw[0] === 4 ? raw : undefined;
      } catch { return undefined; }
    },
    async register(endpoint, allowed = () => true) {
      const current = ++revision;
      if (parsePushRequest(JSON.stringify({ endpoint })) === undefined || !(await supported())) return { registered: 0, failed: 0, reason: "unsupported" };
      if (revision !== current || !allowed()) return { registered: 0, failed: 0, reason: undefined };
      kept = { endpoint, allowed };
      let registered = 0, failed = 0;
      let reason: PushRefusal | undefined;
      for (const cap of await opts.receiveCaps()) {
        if (revision !== current || !allowed()) break;
        const r = await put(endpoint, cap);
        if (r === undefined) registered++; else { failed++; reason ??= r; }
      }
      return { registered, failed, reason: registered === 0 ? reason : undefined };
    },
    pause,
    async unregister(endpoint) {
      pause();
      if (!(await supported())) return;
      for (const cap of await opts.receiveCaps()) {
        await exchange(opts.http, `${opts.base}${PUSH_PATHS.subscription}`, { method: "DELETE", body: bodyOf(endpoint), bearer: cap, headers: JSON_HEADERS }).catch(() => undefined);
      }
    },
    opened(cap) {
      const e = kept;
      if (e?.allowed()) void put(e.endpoint, cap).catch(() => undefined);
    },
  };
}
