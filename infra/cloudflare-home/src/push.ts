// SPDX-License-Identifier: AGPL-3.0-or-later
// The optional push wake (om.relay.push.v1, ADR-0210). One Durable Object for the whole Home holds (a) the Home's own VAPID key, made
// here on first use and never exported, and (b) the browser push endpoints this Home's owner registered, per mailbox. When an item
// arrives for a mailbox nobody is listening on, it sends each endpoint an EMPTY push: no body, no content, no id, no identity. The
// push service therefore learns an endpoint and a time, nothing else. A push is a hint; losing it costs time only.
import { DurableObject } from "cloudflare:workers";
import { PUSH_LIMITS, checkPushEndpoint, pushSubscriptionId, toBase64Url } from "@om/home-wire";
import type { Env } from "./env";
import { metaString, one, all, run, setMeta } from "./sql";
import { nowSeconds } from "./util";

const SCHEMA = `
CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v);
CREATE TABLE IF NOT EXISTS subs (label TEXT NOT NULL, id TEXT NOT NULL, endpoint TEXT NOT NULL, expires_at INTEGER NOT NULL, last_sent INTEGER NOT NULL DEFAULT 0, day INTEGER NOT NULL DEFAULT 0, day_count INTEGER NOT NULL DEFAULT 0, failures INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (label, id));
`;

/** A registration that fails this many sends in a row (not "gone") is forgotten; the device registers again at its next start. */
const MAX_FAILURES = 5;
const SEND_TIMEOUT_MS = 8000;
const JWT_SECONDS = 6 * 3600;

const positive = (v: string | undefined, fallback: number): number => { const n = Number(v); return v !== undefined && v.trim() !== "" && Number.isSafeInteger(n) && n >= 0 ? n : fallback; };
export const pushPacing = (env: Env): { minInterval: number; perDay: number } => ({ minInterval: positive(env.OM_PUSH_MIN_INTERVAL_SECONDS, PUSH_LIMITS.minIntervalSeconds), perDay: positive(env.OM_PUSH_PER_DAY, PUSH_LIMITS.perDay) });
export const extraPushHosts = (env: Env): readonly string[] => (env.OM_PUSH_HOSTS ?? "").split(",").map((h) => h.trim()).filter((h) => h !== "");

export type RegisterResult = { readonly ok: true; readonly id: string; readonly expiresAt: number } | { readonly ok: false; readonly error: "invalid_request" | "quota" };

const utf8 = (s: string): Uint8Array => new TextEncoder().encode(s);
const part = (o: unknown): string => toBase64Url(utf8(JSON.stringify(o)));

export class PushHub extends DurableObject<Env> {
  private readonly sql: SqlStorage;
  private key: { priv: CryptoKey; pub: string } | undefined;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(SCHEMA);
  }

  /** The Home's VAPID key: made on first use from the Worker's own randomness, kept only in this object's storage. Returns the public half. */
  private async vapid(): Promise<{ priv: CryptoKey; pub: string }> {
    if (this.key !== undefined) return this.key;
    let jwk = metaString(this.sql, "vapid_jwk");
    let pub = metaString(this.sql, "vapid_pub");
    if (jwk === undefined || pub === undefined) {
      const pair = await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, ["sign", "verify"]) as CryptoKeyPair;
      jwk = JSON.stringify((await crypto.subtle.exportKey("jwk", pair.privateKey)) as unknown as JsonWebKey);
      pub = toBase64Url(new Uint8Array((await crypto.subtle.exportKey("raw", pair.publicKey)) as ArrayBuffer));
      setMeta(this.sql, "vapid_jwk", jwk);
      setMeta(this.sql, "vapid_pub", pub);
    }
    const priv = await crypto.subtle.importKey("jwk", JSON.parse(jwk) as JsonWebKey, { name: "ECDSA", namedCurve: "P-256" }, false, ["sign"]);
    this.key = { priv, pub };
    return this.key;
  }

  async publicKey(): Promise<string> { return (await this.vapid()).pub; }

  async register(label: string, endpoint: string, subject: string): Promise<RegisterResult> {
    if (!checkPushEndpoint(endpoint, extraPushHosts(this.env)).ok) return { ok: false, error: "invalid_request" };
    const now = nowSeconds();
    run(this.sql, "DELETE FROM subs WHERE expires_at <= ?", now);
    const id = await pushSubscriptionId(endpoint);
    const expiresAt = now + PUSH_LIMITS.lifetimeSeconds;
    const have = one<{ n: number }>(this.sql, "SELECT COUNT(*) AS n FROM subs WHERE label = ? AND id = ?", label, id)?.n === 1;
    if (!have) {
      if ((one<{ n: number }>(this.sql, "SELECT COUNT(*) AS n FROM subs WHERE label = ?", label)?.n ?? 0) >= PUSH_LIMITS.perQueue) return { ok: false, error: "quota" };
      if ((one<{ n: number }>(this.sql, "SELECT COUNT(*) AS n FROM subs")?.n ?? 0) >= PUSH_LIMITS.perHome) return { ok: false, error: "quota" };
      run(this.sql, "INSERT INTO subs (label, id, endpoint, expires_at) VALUES (?, ?, ?, ?)", label, id, endpoint, expiresAt);
    } else {
      run(this.sql, "UPDATE subs SET expires_at = ?, failures = 0 WHERE label = ? AND id = ?", expiresAt, label, id);
    }
    setMeta(this.sql, "subject", subject);
    await this.vapid();
    return { ok: true, id, expiresAt };
  }

  async unregister(label: string, endpoint: string): Promise<void> {
    run(this.sql, "DELETE FROM subs WHERE label = ? AND id = ?", label, await pushSubscriptionId(endpoint));
  }

  /** Something new arrived for `label` and no client is listening. Sends at most one empty push per registration per interval and per UTC day. */
  async notify(label: string): Promise<void> {
    const now = nowSeconds();
    const { minInterval, perDay } = pushPacing(this.env);
    const today = Math.floor(now / 86_400);
    const rows = all<{ id: string; endpoint: string; expires_at: number; last_sent: number; day: number; day_count: number; failures: number }>(this.sql, "SELECT id, endpoint, expires_at, last_sent, day, day_count, failures FROM subs WHERE label = ?", label);
    const due = rows.filter((r) => r.expires_at > now && now - r.last_sent >= minInterval && (r.day !== today || r.day_count < perDay));
    if (due.length === 0) return;
    // Claim the slot before the network call: a second arrival that interleaves with the send must find the interval already used.
    for (const r of due) run(this.sql, "UPDATE subs SET last_sent = ?, day = ?, day_count = ? WHERE label = ? AND id = ?", now, today, r.day === today ? r.day_count + 1 : 1, label, r.id);
    const { priv, pub } = await this.vapid();
    const subject = metaString(this.sql, "subject") ?? "https://invalid.example";
    await Promise.all(due.map(async (r) => {
      const verdict = await this.send(r.endpoint, priv, pub, subject, now);
      if (verdict === "gone") run(this.sql, "DELETE FROM subs WHERE label = ? AND id = ?", label, r.id);
      else if (verdict === "failed") run(this.sql, r.failures + 1 >= MAX_FAILURES ? "DELETE FROM subs WHERE label = ? AND id = ?" : "UPDATE subs SET failures = failures + 1 WHERE label = ? AND id = ?", label, r.id);
      else if (r.failures > 0) run(this.sql, "UPDATE subs SET failures = 0 WHERE label = ? AND id = ?", label, r.id);
    }));
  }

  private async send(endpoint: string, priv: CryptoKey, pub: string, subject: string, now: number): Promise<"sent" | "gone" | "failed"> {
    try {
      const head = part({ typ: "JWT", alg: "ES256" });
      const claims = part({ aud: new URL(endpoint).origin, exp: now + JWT_SECONDS, sub: subject });
      const sig = new Uint8Array(await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, priv, utf8(`${head}.${claims}`)));
      // No body, no Content-Encoding, no Crypto-Key: an empty push needs no payload encryption. Never follows a redirect.
      const res = await fetch(endpoint, {
        method: "POST", redirect: "manual", signal: AbortSignal.timeout(SEND_TIMEOUT_MS),
        headers: { Authorization: `vapid t=${head}.${claims}.${toBase64Url(sig)}, k=${pub}`, TTL: String(PUSH_LIMITS.ttlSeconds), Urgency: "normal" },
      });
      await res.body?.cancel();
      if (res.status === 404 || res.status === 410) return "gone";
      return res.status >= 200 && res.status < 300 ? "sent" : "failed";
    } catch { return "failed"; }
  }
}
