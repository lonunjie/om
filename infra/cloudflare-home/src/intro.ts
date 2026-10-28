// SPDX-License-Identifier: MPL-2.0
// IntroGate role (INVITES_INTROGATE_AND_SPAM.md section 5/6). One Durable Object per recipient service. The recipient (holder
// of the service capability) issues narrow tickets; an unknown sender submits one sealed MessageRequest per use. The gate
// rejects before any expensive work: ticket shape, expiry, use count, payload limit, queue quota and a per-service rate.
// It cannot open a request, learns no sender identity, and a ticket is never account or relationship authority.
//
// ticket_id = service_prefix(8) || random(8): the frozen 16-byte id carries the routing prefix, so a Home needs no global
// ticket index. Guessing a ticket id succeeds with probability 2^-64 per attempt against a per-service rate limit.
import { DurableObject } from "cloudflare:workers";
import {
  HomeError, concat, decodeTicketParams, encodeDrainResponse, encodeDepositReceipt, encodeTicketList, quotaDetail, sha256, parseDeposit,
  type IntroTicketRequest, type IntroductionTicket, type RelayDrainRequest,
} from "@om/home-wire";
import type { Env } from "./env";
import { LIMITS } from "./limits";
import { MAILBOX_SCHEMA, cursorOf, enqueue, nextExpiry, page, purgeExpired } from "./mailbox";
import { metaNumber, metaString, one, run, setMeta } from "./sql";
import { capHash, fingerprint, nowSeconds, reply, takeToken, type Reply } from "./util";

const SCHEMA = `${MAILBOX_SCHEMA}
CREATE TABLE IF NOT EXISTS tickets (id BLOB PRIMARY KEY, expires_at INTEGER NOT NULL, max_uses INTEGER NOT NULL, used INTEGER NOT NULL DEFAULT 0, payload_limit INTEGER NOT NULL, revoked INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS uses (ticket_id BLOB NOT NULL, fp TEXT NOT NULL, PRIMARY KEY (ticket_id, fp));
`;

export class IntroGate extends DurableObject<Env> {
  private readonly sql: SqlStorage;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(SCHEMA);
  }

  private get hard(): boolean {
    return this.env.OM_HARD_FREE_TIER === "1";
  }

  private async schedule(): Promise<void> {
    const t = nextExpiry(this.sql);
    const tickets = one<{ t: number | null }>(this.sql, "SELECT MIN(expires_at) AS t FROM tickets")?.t ?? undefined;
    const soonest = [t, tickets].filter((x): x is number => x !== undefined);
    if (soonest.length === 0) { await this.ctx.storage.deleteAlarm(); return; }
    await this.ctx.storage.setAlarm(Math.max(Date.now() + 1000, Math.min(...soonest) * 1000 + 1000));
  }

  override async alarm(): Promise<void> {
    const now = nowSeconds();
    purgeExpired(this.sql, now);
    run(this.sql, "DELETE FROM uses WHERE ticket_id IN (SELECT id FROM tickets WHERE expires_at + ? < ?)", LIMITS.skewSeconds, now);
    run(this.sql, "DELETE FROM tickets WHERE expires_at + ? < ?", LIMITS.skewSeconds, now);
    await this.schedule();
  }

  /** Recipient authority over this service: first use claims it, every later use must present the same capability. */
  private async serviceAuth(cap: Uint8Array, claim: boolean): Promise<void> {
    const h = await capHash(cap);
    const owner = metaString(this.sql, "owner");
    if (owner === undefined) {
      if (!claim) throw new HomeError("unauthorized");
      setMeta(this.sql, "owner", h);
      return;
    }
    if (owner !== h) throw new HomeError("unauthorized");
  }

  issueTickets(cap: Uint8Array, req: IntroTicketRequest): Promise<Reply> {
    return reply(async () => {
      let params;
      try { params = decodeTicketParams(req.params); } catch { throw new HomeError("invalid_request"); }
      const count = req.count ?? 1;
      if (count < 1 || count > 16) throw new HomeError("invalid_request");
      if (params.ttlSeconds < 1 || params.ttlSeconds > LIMITS.maxTicketSeconds || params.maxUses < 1 || params.maxUses > LIMITS.maxTicketUses
        || params.payloadLimitBytes < 1 || params.payloadLimitBytes > LIMITS.maxTicketPayload) throw new HomeError("policy_denied");
      // A retried issue request returns the same tickets: ids derive from (service, request_id, index).
      await this.serviceAuth(cap, true);
      const prefix = cap.slice(0, 8);
      const now = nowSeconds();
      const active = one<{ n: number }>(this.sql, "SELECT COUNT(*) AS n FROM tickets WHERE revoked = 0 AND expires_at + ? >= ?", LIMITS.skewSeconds, now)?.n ?? 0;
      const out: IntroductionTicket[] = [];
      const binding = (await sha256(prefix)).slice(0, 16);
      for (let i = 0; i < count; i++) {
        const idTail = (await sha256(prefix, req.requestId, new Uint8Array([i]))).slice(0, 8);
        const id = concat(prefix, idTail);
        const row = one<{ expires_at: number; max_uses: number; payload_limit: number }>(this.sql, "SELECT expires_at, max_uses, payload_limit FROM tickets WHERE id = ?", id);
        if (!row) {
          if (active + out.length >= LIMITS.maxTicketsPerService) throw new HomeError("quota", { detail: quotaDetail(4, this.hard) });
          run(this.sql, "INSERT INTO tickets (id, expires_at, max_uses, payload_limit) VALUES (?, ?, ?, ?)", id, now + params.ttlSeconds, params.maxUses, params.payloadLimitBytes);
        }
        out.push({
          ticketId: id, serviceBinding: binding, expiresAt: row?.expires_at ?? now + params.ttlSeconds, maxUses: row?.max_uses ?? params.maxUses,
          payloadLimitBytes: row?.payload_limit ?? params.payloadLimitBytes, nonce: (await sha256(id, new Uint8Array([1]))).slice(0, 16),
          ...(params.allowAttachments === undefined ? {} : { allowAttachments: params.allowAttachments }),
          ...(params.allowCalls === undefined ? {} : { allowCalls: params.allowCalls }),
        });
      }
      await this.schedule();
      return encodeTicketList(out);
    });
  }

  /** Revoke one ticket (idempotent). */
  revokeTicket(cap: Uint8Array, ticketId: Uint8Array): Promise<Reply> {
    return reply(async () => {
      await this.serviceAuth(cap, false);
      run(this.sql, "UPDATE tickets SET revoked = 1 WHERE id = ?", ticketId);
      return new Uint8Array(0);
    });
  }

  /** `POST /v1/intro/requests`: no credential beyond the ticket inside the MessageRequest. */
  submit(request: Uint8Array): Promise<Reply> {
    return reply(async () => {
      const now = nowSeconds();
      const mr = (() => { try { return parseDeposit(request); } catch { return undefined; } })();
      if (mr?.kind !== "message_request") throw new HomeError("invalid_request");
      const ticket = one<{ expires_at: number; max_uses: number; used: number; payload_limit: number; revoked: number }>(
        this.sql, "SELECT expires_at, max_uses, used, payload_limit, revoked FROM tickets WHERE id = ?", mr.ticketId);
      if (!ticket || ticket.revoked) throw new HomeError("admission_denied"); // unknown, revoked: one answer, no oracle
      if (ticket.expires_at + LIMITS.skewSeconds < now) throw new HomeError("expired");
      // The rate limit is charged only to requests that name a real ticket, so junk aimed at a service's public ticket prefix
      // cannot starve the people holding real tickets.
      const t = takeToken(metaNumber(this.sql, "tokens", LIMITS.submitBurst), metaNumber(this.sql, "tokens_at", now), now, LIMITS.submitBurst, LIMITS.submitRefillPerSecond);
      setMeta(this.sql, "tokens", t.tokens);
      setMeta(this.sql, "tokens_at", now);
      if (!t.allowed) throw new HomeError("rate_limited", { retryAfterSeconds: t.waitSeconds });
      if (request.length > ticket.payload_limit + 1024) throw new HomeError("payload_too_large"); // framing + HPKE enc + tag over the payload limit
      const fp = await fingerprint(request);
      const retry = one(this.sql, "SELECT 1 AS x FROM uses WHERE ticket_id = ? AND fp = ?", mr.ticketId, fp) !== undefined;
      if (!retry && ticket.used >= ticket.max_uses) throw new HomeError("spent_token");
      purgeExpired(this.sql, now);
      const pending = one<{ n: number }>(this.sql, "SELECT COUNT(*) AS n FROM items")?.n ?? 0;
      if (!retry && pending >= LIMITS.maxPendingRequests) throw new HomeError("quota", { detail: quotaDetail(1, this.hard) });
      const q = enqueue(this.sql, { bytes: request, fp, tupleKey: undefined, expiresAt: Math.min(ticket.expires_at, now + LIMITS.maxRetentionSeconds), capHash: undefined, hard: this.hard });
      if (!retry) {
        run(this.sql, "INSERT INTO uses (ticket_id, fp) VALUES (?, ?)", mr.ticketId, fp);
        run(this.sql, "UPDATE tickets SET used = used + 1 WHERE id = ?", mr.ticketId);
      }
      await this.schedule();
      return encodeDepositReceipt({ itemId: q.id, seq: q.seq });
    });
  }

  /** Drain accepted requests with sequence greater than `after`; items at or below `after` are acknowledged and deleted. */
  drain(req: RelayDrainRequest): Promise<Reply> {
    return reply(async () => {
      await this.serviceAuth(req.cap, false);
      const after = req.after ?? 0;
      run(this.sql, "DELETE FROM items WHERE seq <= ?", after);
      const p = page(this.sql, after, nowSeconds());
      const last = p.items.at(-1);
      await this.schedule();
      return encodeDrainResponse({ items: p.items.map((i) => ({ id: i.id, bytes: i.bytes, seq: i.seq })), ...(p.more && last ? { next: cursorOf(last.seq) } : {}) });
    });
  }
}

