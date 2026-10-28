// SPDX-License-Identifier: MPL-2.0
// OM reference Home on Cloudflare: Worker front door for the Home API v1 reference HTTP binding (spec/protocol/home-api.md).
// The Worker routes by an opaque capability prefix or object id to a SQLite Durable Object and keeps nothing itself.
// It never reads a network address or any identifying header, writes no log, and sets no cookie.
import {
  CborError, HomeError, decodeAckRequest, decodeDepositRequest, decodeDescriptorPut, decodeDrainRequest, decodeIntroDrainRequest, decodeIntroTicketRequest, decodeKeyFetch,
  decodeKeyPublish, decodeMintRequest, decodeQueueCreate, decodeRotateRequest, encodeKeyFetchResponse, decodeKeyFetchResponse, fromHex, decode, Obj, type HomeErrorName,
} from "@om/home-wire";
import { capabilitiesDocument } from "./capabilities";
import type { Env } from "./env";
import { bearer, fail, fromReply, json, ok, preflight, readBody } from "./http";
import { LIMITS } from "./limits";
import type { Reply } from "./util";
import { DescriptorStore } from "./descriptors";
import { IntroGate } from "./intro";
import { KeyPool } from "./keys";
import { MailboxQueue } from "./queue";

export { DescriptorStore, IntroGate, KeyPool, MailboxQueue };

const hex = (b: Uint8Array): string => Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
const queueOf = (env: Env, cap: Uint8Array): DurableObjectStub<MailboxQueue> => env.QUEUES.get(env.QUEUES.idFromName(`q:${hex(cap.slice(0, 16))}`));
const introOf = (env: Env, cap: Uint8Array): DurableObjectStub<IntroGate> => env.INTRO.get(env.INTRO.idFromName(`i:${hex(cap.slice(0, 8))}`));

const BODY_MAX = LIMITS.maxDepositBytes + 2048;

/** A capability carries its routing prefix (16 bytes for a queue, 8 for an IntroGate service) plus at least 16 secret bytes. */
const wellFormed = (cap: Uint8Array, prefix: number): boolean => cap.length >= prefix + 16 && cap.length <= 96;

class Bad extends Error {
  constructor(readonly code: HomeErrorName) { super(code); }
}

async function body(request: Request, max: number): Promise<Uint8Array> {
  if (request.method !== "POST" && request.method !== "PUT") throw new Bad("invalid_request");
  const b = await readBody(request, max);
  if (b === undefined) throw new Bad("payload_too_large");
  return b;
}

function parse<T>(f: (b: Uint8Array) => T, b: Uint8Array): T {
  try { return f(b); } catch (e) { if (e instanceof CborError) throw new Bad("invalid_request"); throw e; }
}

async function route(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);
  const path = url.pathname;
  const m = request.method;

  if (m === "OPTIONS") return preflight();
  if (path === "/.well-known/om-provider-capabilities") return m === "GET" ? json(capabilitiesDocument(env)) : fail("invalid_request");
  if (path === "/" || path === "/healthz") return m === "GET" ? ok(new TextEncoder().encode("OM reference Home\n"), "text/plain; charset=utf-8") : fail("invalid_request");

  // ----------------------------------------------------------------------------------------------- relay
  if (path === "/v1/relay/queues") {
    const req = parse(decodeQueueCreate, await body(request, 1024));
    if (![req.receiveCap, req.depositCap, ...(req.adminCap ? [req.adminCap] : [])].every((c) => wellFormed(c, 16))) return fail("invalid_request");
    const prefix = hex(req.receiveCap.slice(0, 16));
    if (hex(req.depositCap.slice(0, 16)) !== prefix || (req.adminCap && hex(req.adminCap.slice(0, 16)) !== prefix)) return fail("invalid_request");
    return fromReply(await queueOf(env, req.receiveCap).create(req));
  }
  if (path === "/v1/relay/deposit") {
    const req = parse(decodeDepositRequest, await body(request, BODY_MAX));
    return fromReply(await queueOf(env, wellFormed(req.cap, 16) ? req.cap : new Uint8Array(16)).deposit(req));
  }
  if (path === "/v1/relay/drain") {
    const req = parse(decodeDrainRequest, await body(request, 1024));
    return fromReply(await queueOf(env, wellFormed(req.cap, 16) ? req.cap : new Uint8Array(16)).drain(req));
  }
  if (path === "/v1/relay/ack") {
    const req = parse(decodeAckRequest, await body(request, 8192));
    return fromReply(await queueOf(env, wellFormed(req.cap, 16) ? req.cap : new Uint8Array(16)).ack(req));
  }
  if (path === "/v1/relay/rotate") {
    const req = parse(decodeRotateRequest, await body(request, 1024));
    return fromReply(await queueOf(env, wellFormed(req.cap, 16) ? req.cap : new Uint8Array(16)).rotate(req));
  }
  if (path === "/v1/relay/deposit-capabilities") {
    const admin = bearer(request);
    if (!admin || !wellFormed(admin, 16)) return fail("unauthorized");
    return fromReply(await queueOf(env, admin).mint(admin, parse(decodeMintRequest, await body(request, 1024))));
  }
  const revokeMatch = /^\/v1\/relay\/deposit-capabilities\/([0-9a-f]{32})$/.exec(path);
  if (revokeMatch) {
    if (m !== "DELETE") return fail("invalid_request");
    const admin = bearer(request);
    if (!admin || !wellFormed(admin, 16)) return fail("unauthorized");
    return fromReply(await queueOf(env, admin).revokeDepositCapability(admin, fromHex(revokeMatch[1] ?? "")));
  }

  // ----------------------------------------------------------------------------------------------- descriptors
  const desc = /^\/v1\/discovery\/descriptors\/([0-9a-f]{32})$/.exec(path);
  if (desc) {
    const stub = env.DESCRIPTORS.get(env.DESCRIPTORS.idFromName(`d:${desc[1] ?? ""}`));
    if (m === "GET") return fromReply(await stub.get());
    const cap = bearer(request);
    if (!cap) return fail("unauthorized");
    if (m === "PUT") return fromReply(await stub.put(cap, parse(decodeDescriptorPut, await body(request, LIMITS.maxDescriptorBytes + 1024))));
    if (m === "DELETE") return fromReply(await stub.revoke(cap));
    return fail("invalid_request");
  }

  // ----------------------------------------------------------------------------------------------- keys
  const bundle = /^\/v1\/keys\/bundles\/([0-9a-f]{32})$/.exec(path);
  if (bundle && m === "PUT") {
    const cap = bearer(request);
    if (!cap) return fail("unauthorized");
    const device = fromHex(bundle[1] ?? "");
    return fromReply(await env.KEYS.get(env.KEYS.idFromName(`k:${bundle[1] ?? ""}`)).publish(cap, device, parse(decodeKeyPublish, await body(request, 4 * LIMITS.maxBundleBytes))));
  }
  const gen = /^\/v1\/keys\/bundles\/([0-9a-f]{32})\/generations\/(\d{1,15})$/.exec(path);
  if (gen) {
    if (m !== "DELETE") return fail("invalid_request");
    const cap = bearer(request);
    if (!cap) return fail("unauthorized");
    return fromReply(await env.KEYS.get(env.KEYS.idFromName(`k:${gen[1] ?? ""}`)).revokeGeneration(cap, Number(gen[2])));
  }
  if (path === "/v1/keys/fetch") {
    const req = parse(decodeKeyFetch, await body(request, 4096));
    // One flat list: for each requested device, its signed bundle then the one-time items consumed for this fetch.
    const items: Uint8Array[] = [];
    for (const d of req.devices) {
      if (d.length !== 16) return fail("invalid_request");
      const r: Reply = await env.KEYS.get(env.KEYS.idFromName(`k:${hex(d)}`)).fetchItems();
      if (!r.ok) return fromReply(r);
      items.push(...decodeKeyFetchResponse(r.body).items);
    }
    return ok(encodeKeyFetchResponse({ items }));
  }

  // ----------------------------------------------------------------------------------------------- IntroGate
  if (path === "/v1/intro/tickets") {
    const cap = bearer(request);
    if (!cap || !wellFormed(cap, 8)) return fail("unauthorized");
    return fromReply(await introOf(env, cap).issueTickets(cap, parse(decodeIntroTicketRequest, await body(request, 1024))));
  }
  if (path === "/v1/intro/requests") {
    const b = await body(request, LIMITS.maxTicketPayload + 4096);
    // The ticket id sits at a fixed place of the canonical MessageRequest; reading it only routes, the gate validates it.
    const prefix = ticketPrefix(b);
    if (!prefix) return fail("invalid_request");
    return fromReply(await env.INTRO.get(env.INTRO.idFromName(`i:${hex(prefix)}`)).submit(b));
  }
  if (path === "/v1/intro/drain") {
    const req = parse(decodeIntroDrainRequest, await body(request, 1024));
    if (!wellFormed(req.cap, 8)) return fail("unauthorized");
    return fromReply(await introOf(env, req.cap).drain(req));
  }
  return fail("not_found");
}

function ticketPrefix(b: Uint8Array): Uint8Array | undefined {
  try {
    const o = new Obj(decode(b));
    o.uint(0); o.uint(1);
    return o.bytes(2, 16, 16).slice(0, 8);
  } catch { return undefined; }
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    try {
      return await route(request, env);
    } catch (e) {
      if (e instanceof Bad) return fail(e.code);
      if (e instanceof HomeError) return fail(e.errorName);
      // A bug or a Durable Object that is briefly unavailable. Say retryable, reveal nothing, log nothing.
      return fail("retryable", { retryAfterSeconds: 2 });
    }
  },
} satisfies ExportedHandler<Env>;
