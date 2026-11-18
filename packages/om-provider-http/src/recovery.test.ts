// SPDX-License-Identifier: MPL-2.0
// The RecoveryStore client (ADR-0201) against a stubbed Home: the shape of every request (verb, path, headers, body), where a capability
// travels, when the owner's admission is sent, and how every answer, typed or not, is classified. The real Home is exercised by the
// loopback test in infra/cloudflare-home/test/recovery.client.test.ts.
import {
  HomeError, FEATURES, decode, decodeFanout, decodeInboxAck, decodeInboxDrain, decodeInboxRegister, decodeRecordPut, decodeVetoPost, encodeHomeError, encodeInboxDrainResponse, encodeRecordGetResponse, encodeVetoList, fromBase64Url, recoveryEtag, recoveryInboxId, recoveryLocator, toBase64Url, toHex, ADMISSION_HEADER,
} from "@om/home-wire";
import { describe, expect, it } from "vitest";
import type { NetCall } from "@om/core-wasm";
import { createHttpProvider, createMemorySecretStore, createRecoveryStoreClient, type HttpProvider, fanOutToAll, parseProviderUrl, recordLocator, recoveryStoreFor, type RecoveryStoreClient } from "./index";

const rand = (n: number): Uint8Array => crypto.getRandomValues(new Uint8Array(n));
const BASE = "https://home.example";
const cfg = (): { baseUrl: string } => { const c = parseProviderUrl(BASE); if (!c) throw new Error("bad url"); return c; };
const urlOf = (i: RequestInfo | URL): string => (typeof i === "string" ? i : i instanceof URL ? i.href : i.url);

interface Seen { url: string; method: string; headers: Headers; body: Uint8Array }
function mk(handler: (s: Seen) => Response | Promise<Response>, admission?: Uint8Array): { client: RecoveryStoreClient; seen: Seen[] } {
  const seen: Seen[] = [];
  const client = createRecoveryStoreClient({
    config: cfg(), timeoutMs: 1000, ...(admission ? { admission } : {}),
    fetch: async (i, init) => {
      const s: Seen = { url: urlOf(i), method: init?.method ?? "GET", headers: new Headers(init?.headers), body: init?.body ? new Uint8Array(init.body as Uint8Array) : new Uint8Array(0) };
      seen.push(s);
      return handler(s);
    },
  });
  return { client, seen };
}
const typed = (name: ConstructorParameters<typeof HomeError>[0], status = 400, retryAfterSeconds?: number): Response =>
  new Response(encodeHomeError(new HomeError(name, retryAfterSeconds === undefined ? {} : { retryAfterSeconds })) as BodyInit, { status, headers: { "content-type": "application/cbor" } });
const empty = (etag?: Uint8Array): Response => new Response(null, { status: 204, headers: etag ? { ETag: `"${toBase64Url(etag)}"` } : {} });
const secret = rand(32);

describe("record put", () => {
  it("a create sends If-None-Match and the owner's admission, the write capability only as a bearer, and returns the ETag", async () => {
    const etag = rand(32);
    const { client, seen } = mk(() => empty(etag), secret);
    const writeCap = rand(32);
    const locator = rand(32);
    const requestId = rand(16);
    const env = rand(20000);
    const r = await client.putRecord({ locator, writeCap, envelope: env, requestId });
    expect(r).toEqual({ ok: true, value: etag });
    const s = seen[0];
    expect(s?.method).toBe("PUT");
    expect(s?.url).toBe(`${BASE}/v1/recovery/records/${toBase64Url(locator)}`);
    expect(s?.headers.get("if-none-match")).toBe("*");
    expect(s?.headers.get("if-match")).toBeNull();
    expect(s?.headers.get(ADMISSION_HEADER)).toBe(toBase64Url(secret));
    expect(s?.headers.get("authorization")).toBe(`Bearer ${toBase64Url(writeCap)}`);
    const body = decodeRecordPut(s?.body ?? new Uint8Array(0));
    expect(toHex(body.requestId)).toBe(toHex(requestId));
    expect(toHex(body.envelope)).toBe(toHex(env));
  });

  it("a replace sends If-Match with the quoted ETag and no admission; a capability is never in a URL, query or body", async () => {
    const { client, seen } = mk(() => empty(rand(32)), secret);
    const writeCap = rand(32);
    const ifMatch = rand(32);
    await client.putRecord({ locator: rand(32), writeCap, envelope: rand(100), ifMatch });
    const s = seen[0];
    expect(s?.headers.get("if-match")).toBe(`"${toBase64Url(ifMatch)}"`);
    expect(s?.headers.get("if-none-match")).toBeNull();
    expect(s?.headers.get(ADMISSION_HEADER)).toBeNull();
    expect(s?.url).not.toContain(toBase64Url(writeCap));
    expect(toHex(s?.body ?? new Uint8Array(0))).not.toContain(toHex(writeCap));
  });

  it("malformed input never reaches the network", async () => {
    const { client, seen } = mk(() => empty(rand(32)));
    expect((await client.putRecord({ locator: rand(32), writeCap: rand(31), envelope: rand(10) })).ok).toBe(false);
    expect((await client.putRecord({ locator: rand(31), writeCap: rand(32), envelope: rand(10) })).ok).toBe(false);
    expect((await client.putRecord({ locator: rand(32), writeCap: rand(32), envelope: rand(10), ifMatch: rand(5) })).ok).toBe(false);
    expect(seen).toHaveLength(0);
  });

  it("classifies answers: typed Home errors keep their name (rate_limited with its wait), a lost answer is `network`, a gateway page is `gateway`, a success without an ETag is not a success", async () => {
    const w = { locator: rand(32), writeCap: rand(32), envelope: rand(10) };
    expect(await mk(() => typed("stale_generation", 409)).client.putRecord(w)).toEqual({ ok: false, failure: { kind: "home", name: "stale_generation", retryable: false } });
    expect(await mk(() => typed("rate_limited", 429, 600)).client.putRecord(w)).toEqual({ ok: false, failure: { kind: "home", name: "rate_limited", retryable: true, retryAfterSeconds: 600 } });
    for (const name of ["quota", "payload_too_large", "admission_denied", "unauthorized"] as const) expect((await mk(() => typed(name)).client.putRecord(w))).toMatchObject({ ok: false, failure: { kind: "home", name } });
    expect(await mk(() => Promise.reject(new TypeError("reset"))).client.putRecord(w)).toEqual({ ok: false, failure: { kind: "network" } });
    expect(await mk(() => new Response("<html>502</html>", { status: 502, headers: { "content-type": "text/html" } })).client.putRecord(w)).toEqual({ ok: false, failure: { kind: "gateway", status: 502 } });
    expect(await mk(() => empty()).client.putRecord(w)).toEqual({ ok: false, failure: { kind: "gateway", status: 204 } });
  });

  it("an exact retry sends the same request id and the same bytes (the caller's fixed id is used as given)", async () => {
    const { client, seen } = mk(() => Promise.reject(new TypeError("lost")));
    const w = { locator: rand(32), writeCap: rand(32), envelope: rand(500), requestId: rand(16) };
    await client.putRecord(w);
    await client.putRecord(w);
    expect(toHex(seen[0]?.body ?? new Uint8Array(0))).toBe(toHex(seen[1]?.body ?? new Uint8Array(1)));
  });
});

describe("record get and delete", () => {
  it("get derives the locator from the read capability, sends the capability only as a bearer, an optional OM-Request-Id, and no body", async () => {
    const readCap = rand(32);
    const env = rand(20000);
    const etag = await recoveryEtag(env);
    const { client, seen } = mk(() => new Response(encodeRecordGetResponse(env) as BodyInit, { status: 200, headers: { "content-type": "application/cbor", ETag: `"${toBase64Url(etag)}"` } }));
    const r = await client.getRecord(readCap);
    expect(r.ok && toHex(r.value.envelope)).toBe(toHex(env));
    expect(r.ok && toHex(r.value.etag)).toBe(toHex(etag));
    const s = seen[0];
    expect(s?.method).toBe("GET");
    expect(s?.body).toHaveLength(0);
    expect(s?.url).toBe(`${BASE}/v1/recovery/records/${toBase64Url(await recordLocator(readCap))}`);
    expect(s?.url).not.toContain(toBase64Url(readCap));
    expect(s?.headers.get("authorization")).toBe(`Bearer ${toBase64Url(readCap)}`);
    expect(fromBase64Url(s?.headers.get("om-request-id") ?? "")).toHaveLength(16);
    expect(s?.headers.get(ADMISSION_HEADER)).toBeNull();
    expect(toHex(await recoveryLocator(readCap))).toBe(toHex(await recordLocator(readCap)));
  });

  it("not_found is a typed failure; a body that is not a record answer is not a success", async () => {
    expect(await mk(() => typed("not_found", 404)).client.getRecord(rand(32))).toEqual({ ok: false, failure: { kind: "home", name: "not_found", retryable: false } });
    expect((await mk(() => new Response(new Uint8Array([1, 2, 3]) as BodyInit, { status: 200, headers: { ETag: `"${toBase64Url(rand(32))}"` } })).client.getRecord(rand(32))).ok).toBe(false);
  });

  it("delete is idempotent: not_found counts as done; any other failure is reported", async () => {
    const { client, seen } = mk(() => typed("not_found", 404));
    expect(await client.deleteRecord(rand(32), rand(32))).toEqual({ ok: true, value: undefined });
    expect(seen[0]?.method).toBe("DELETE");
    expect(await mk(() => typed("rate_limited", 429, 3)).client.deleteRecord(rand(32), rand(32))).toMatchObject({ ok: false, failure: { name: "rate_limited" } });
    expect(await mk(() => empty()).client.deleteRecord(rand(32), rand(32))).toEqual({ ok: true, value: undefined });
  });
});

describe("fan-out, veto, board and inbox", () => {
  it("fan-out and veto post carry the signed bytes with a content-derived request id (so a retry is exact) and no capability", async () => {
    const { client, seen } = mk(() => empty());
    const signed = rand(300);
    await client.fanOut(signed);
    await client.fanOut(signed);
    await client.postVeto(signed);
    expect(seen.map((s) => [s.method, new URL(s.url).pathname])).toEqual([["POST", "/v1/recovery/requests"], ["POST", "/v1/recovery/requests"], ["POST", "/v1/recovery/vetoes"]]);
    expect(seen[0]?.headers.get("authorization")).toBeNull();
    const a = decodeFanout(seen[0]?.body ?? new Uint8Array(0));
    const b = decodeFanout(seen[1]?.body ?? new Uint8Array(0));
    expect(toHex(a.requestId)).toBe(toHex(b.requestId));
    expect(toHex(a.signed)).toBe(toHex(signed));
    expect(toHex(decodeVetoPost(seen[2]?.body ?? new Uint8Array(0)).signed)).toBe(toHex(signed));
    expect(toHex(decodeVetoPost(seen[2]?.body ?? new Uint8Array(0)).requestId)).not.toBe(toHex(a.requestId)); // a veto and a request never share an id
  });

  it("the veto board is a plain GET by digest with no Authorization, and returns the vetoes", async () => {
    const digest = rand(32);
    const vetoes = [rand(50), rand(60)];
    const { client, seen } = mk(() => new Response(encodeVetoList(vetoes) as BodyInit, { status: 200 }));
    const r = await client.readVetoes(digest);
    expect(r.ok && r.value.map(toHex)).toEqual(vetoes.map(toHex));
    expect(seen[0]?.url).toBe(`${BASE}/v1/recovery/requests/${toBase64Url(digest)}/vetoes`);
    expect(seen[0]?.headers.get("authorization")).toBeNull();
    expect((await mk(() => typed("rate_limited", 429, 10)).client.readVetoes(digest))).toMatchObject({ ok: false, failure: { retryAfterSeconds: 10 } });
  });

  it("the inbox is indexed by a hash of the account id, never the id; its capability is in the frozen body slot; registering sends the admission, draining and acknowledging do not", async () => {
    const accountId = rand(32);
    const cap = rand(32);
    const item = { id: rand(16), kind: 1 as const, bytes: rand(40) };
    const { client, seen } = mk((s) => (s.url.endsWith("/drain") ? new Response(encodeInboxDrainResponse({ items: [item], cursor: new Uint8Array(8) }) as BodyInit, { status: 200 }) : empty()), secret);
    await client.registerInbox({ accountId, cap });
    const d = await client.drainInbox({ accountId, cap }, 4);
    await client.ackInbox({ accountId, cap }, [item.id]);
    const inbox = toBase64Url(await recoveryInboxId(accountId));
    expect(seen.map((s) => [s.method, new URL(s.url).pathname])).toEqual([["PUT", `/v1/recovery/inbox/${inbox}`], ["POST", `/v1/recovery/inbox/${inbox}/drain`], ["POST", `/v1/recovery/inbox/${inbox}/ack`]]);
    for (const s of seen) { expect(s.url).not.toContain(toBase64Url(accountId)); expect(s.headers.get("authorization")).toBeNull(); }
    expect(seen.map((s) => s.headers.get(ADMISSION_HEADER))).toEqual([toBase64Url(secret), null, null]);
    expect(toHex(decodeInboxRegister(seen[0]?.body ?? new Uint8Array(0)).cap)).toBe(toHex(cap));
    expect(decodeInboxDrain(seen[1]?.body ?? new Uint8Array(0)).pageSize).toBe(4);
    expect(decodeInboxAck(seen[2]?.body ?? new Uint8Array(0)).itemIds.map(toHex)).toEqual([toHex(item.id)]);
    expect(d.ok && d.value.items).toEqual([item]);
    expect(d.ok && d.value.more).toBe(true);
    expect(decode(seen[1]?.body ?? new Uint8Array(0))).toBeInstanceOf(Map);
  });

  it("an inbox call with a wrong-sized capability or account id, or an empty ack, never reaches the network", async () => {
    const { client, seen } = mk(() => empty());
    expect((await client.registerInbox({ accountId: rand(32), cap: rand(16) })).ok).toBe(false);
    expect((await client.drainInbox({ accountId: rand(31), cap: rand(32) })).ok).toBe(false);
    expect((await client.ackInbox({ accountId: rand(32), cap: rand(32) }, [])).ok).toBe(false);
    expect(seen).toHaveLength(0);
  });

  it("fan-out to every Home: one failing Home never stops the others, and the call succeeds when any Home took it", async () => {
    const good = mk(() => empty());
    const down = mk(() => Promise.reject(new TypeError("down")));
    const limited = mk(() => typed("rate_limited", 429, 60));
    const r = await fanOutToAll([down.client, limited.client, good.client], rand(100));
    expect(r.ok).toBe(true);
    expect(r.results.map((x) => (x.ok ? "ok" : x.failure.kind === "home" ? x.failure.name : x.failure.kind))).toEqual(["network", "rate_limited", "ok"]);
    expect((await fanOutToAll([down.client], rand(100))).ok).toBe(false);
    expect(good.seen).toHaveLength(1);
  });
});

describe("capabilities (is there a RecoveryStore here)", () => {
  const doc = (d: Record<string, unknown>): Response => new Response(JSON.stringify(d), { status: 200 });
  const scope = toBase64Url(rand(32));
  it("returns the scope id and limits only when the feature is advertised with a well-formed scope", async () => {
    const on = await mk(() => doc({ features: [FEATURES.recoveryStore], recovery_scope_id: scope, limits: { max_recovery_records: 16 } })).client.info();
    expect(on && toBase64Url(on.scopeId)).toBe(scope);
    expect(on?.limits).toEqual({ max_recovery_records: 16 });
    for (const d of [
      { features: ["om.relay.queues.v1"], recovery_scope_id: scope }, // not advertised: "no RecoveryStore here"
      { features: [FEATURES.recoveryStore] }, // no scope
      { features: [FEATURES.recoveryStore], recovery_scope_id: "short" },
      { features: [FEATURES.recoveryStore], recovery_scope_id: `${scope.slice(0, 42)}B` }, // not the canonical form of 32 bytes
    ]) expect(await mk(() => doc(d)).client.info()).toBeUndefined();
    expect(await mk(() => new Response("nope", { status: 200 })).client.info()).toBeUndefined();
    expect(await mk(() => Promise.reject(new TypeError("down"))).client.info()).toBeUndefined();
  });

  it("a store address is validated like every Home address: https, or an opted-in loopback", () => {
    expect(recoveryStoreFor("http://home.example", {})).toBeUndefined();
    expect(recoveryStoreFor("https://home.example", {})).toBeDefined();
    expect(recoveryStoreFor("http://127.0.0.1:8787", { allowLoopbackHttp: true })).toBeDefined();
    expect(recoveryStoreFor("https://home.example/?cap=1", {})).toBeUndefined();
  });
});

describe("the Core transport adapter for the record (recovery_put, recovery_get, recoveryPlace)", () => {
  const typedErr = (name: ConstructorParameters<typeof HomeError>[0], status = 409): Response =>
    new Response(encodeHomeError(new HomeError(name)) as BodyInit, { status, headers: { "content-type": "application/cbor" } });
  const provider = (handler: (s: { url: string; method: string; headers: Headers }) => Response, admission?: Uint8Array): HttpProvider =>
    createHttpProvider({ config: cfg(), secrets: createMemorySecretStore(), timeoutMs: 1000, ...(admission ? { admission } : {}), fetch: (i, init) => Promise.resolve(handler({ url: urlOf(i), method: init?.method ?? "GET", headers: new Headers(init?.headers) })) });
  const put = (extra: object = {}): NetCall => ({ call: "recovery_put", host: "home.example", locator: toHex(rand(32)), writeCap: toHex(rand(32)), requestId: toHex(rand(16)), bytes: toHex(rand(64)), ...extra });

  it("a put answers the new ETag as the hex of its string, and a moved ETag is stale_state (Core reads to settle it)", async () => {
    const etag = rand(32);
    const ok = await provider(() => empty(etag)).perform(put());
    expect(ok).toEqual({ result: "bytes", bytes: toHex(new TextEncoder().encode(toBase64Url(etag))) });
    expect(await provider(() => typedErr("stale_generation")).perform(put({ ifMatch: toBase64Url(rand(32)) }))).toEqual({ result: "err", error: { kind: "rejected", reason: "stale_state" } });
  });

  it("the ETag Core hands back is sent as a quoted If-Match; a first write is If-None-Match", async () => {
    const seen: Headers[] = [];
    const p = provider((s) => { seen.push(s.headers); return empty(rand(32)); });
    const tag = toBase64Url(rand(32));
    await p.perform(put({ ifMatch: tag }));
    await p.perform(put());
    expect(seen[0]?.get("if-match")).toBe(`"${tag}"`);
    expect(seen[1]?.get("if-none-match")).toBe("*");
  });

  it("an unreachable Home is unavailable for a read, never a rejection", async () => {
    const p = createHttpProvider({ config: cfg(), secrets: createMemorySecretStore(), timeoutMs: 100, fetch: () => Promise.reject(new Error("down")) });
    expect(await p.perform({ call: "recovery_get", host: "home.example", locator: toHex(rand(32)), readCap: toHex(rand(32)) })).toEqual({ result: "err", error: { kind: "unavailable" } });
  });

  it("a read of an absent record is not_found, a found one answers [etag, envelope]", async () => {
    const env = rand(50);
    const etag = rand(32);
    const found = await provider(() => new Response(encodeRecordGetResponse(env) as BodyInit, { status: 200, headers: { ETag: `"${toBase64Url(etag)}"` } })).perform({ call: "recovery_get", host: "home.example", locator: toHex(rand(32)), readCap: toHex(rand(32)) });
    expect(found).toEqual({ result: "many", items: [toHex(new TextEncoder().encode(toBase64Url(etag))), toHex(env)] });
    expect(await provider(() => typedErr("not_found", 404)).perform({ call: "recovery_get", host: "home.example", locator: toHex(rand(32)), readCap: toHex(rand(32)) })).toEqual({ result: "err", error: { kind: "not_found" } });
  });

  it("recoveryPlace is the configured Home's address and scope when it advertises the store, and null when it does not", async () => {
    const scope = rand(32);
    const doc = (features: string[]): Response => new Response(JSON.stringify({ features, recovery_scope_id: toBase64Url(scope), limits: {} }), { status: 200 });
    expect(await provider(() => doc([FEATURES.recoveryStore])).recoveryPlace?.()).toEqual({ host: "home.example", scope: toHex(scope) });
    expect(await provider(() => doc([])).recoveryPlace?.()).toBeNull();
  });
});

describe("the Core transport adapter for the owner flow (request, veto, board, inbox)", () => {
  const provider = (handler: (s: { url: string; method: string; body: Uint8Array }) => Response | Promise<Response>): HttpProvider =>
    createHttpProvider({
      config: cfg(), secrets: createMemorySecretStore(), timeoutMs: 1000,
      fetch: async (i, init) => handler({ url: urlOf(i), method: init?.method ?? "GET", body: init?.body ? new Uint8Array(init.body as Uint8Array) : new Uint8Array(0) }),
    });
  const hostOf = { host: "home.example" } as const;

  it("a request and a veto are posted to the Home the call names, answer ok and carry only the signed bytes", async () => {
    const seen: string[] = [];
    const p = provider((s) => { seen.push(`${s.method} ${new URL(s.url).pathname}`); return empty(); });
    expect(await p.perform({ call: "recovery_request_post", ...hostOf, bytes: toHex(rand(200)) })).toEqual({ result: "ok" });
    expect(await p.perform({ call: "recovery_veto_post", ...hostOf, bytes: toHex(rand(100)) })).toEqual({ result: "ok" });
    expect(seen).toEqual(["POST /v1/recovery/requests", "POST /v1/recovery/vetoes"]);
  });

  it("the veto board answers many signed vetoes, and an unreachable Home is unavailable, never an answer of no vetoes", async () => {
    const vetoes = [rand(40), rand(41)];
    const got = await provider(() => new Response(encodeVetoList(vetoes) as BodyInit, { status: 200 })).perform({ call: "recovery_veto_read", ...hostOf, requestDigest: toHex(rand(32)) });
    expect(got).toEqual({ result: "many", items: vetoes.map(toHex) });
    const down = createHttpProvider({ config: cfg(), secrets: createMemorySecretStore(), timeoutMs: 100, fetch: () => Promise.reject(new Error("down")) });
    expect(await down.perform({ call: "recovery_veto_read", ...hostOf, requestDigest: toHex(rand(32)) })).toEqual({ result: "err", error: { kind: "unavailable" } });
  });

  it("the inbox registers, drains into the canonical item maps Core reads, and acknowledges by id", async () => {
    const accountId = rand(32);
    const cap = rand(32);
    const item = { id: rand(16), kind: 2 as const, bytes: rand(40) };
    const seen: string[] = [];
    const p = provider((s) => { seen.push(`${s.method} ${new URL(s.url).pathname.split("/").slice(-1)[0] ?? ""}`); return s.url.endsWith("/drain") ? new Response(encodeInboxDrainResponse({ items: [item] }) as BodyInit, { status: 200 }) : empty(); });
    expect(await p.perform({ call: "recovery_inbox_register", ...hostOf, accountId: toHex(accountId), cap: toHex(cap) })).toEqual({ result: "ok" });
    const drained = await p.perform({ call: "recovery_inbox_drain", ...hostOf, accountId: toHex(accountId), cap: toHex(cap) });
    expect(drained.result).toBe("many");
    const m = decode(fromHexLocal(drained.result === "many" ? drained.items[0] ?? "" : ""));
    expect(m).toBeInstanceOf(Map);
    expect([...(m as Map<number, unknown>).keys()]).toEqual([0, 1, 2]);
    expect(await p.perform({ call: "recovery_inbox_ack", ...hostOf, accountId: toHex(accountId), cap: toHex(cap), itemIds: [toHex(item.id)] })).toEqual({ result: "ok" });
    expect(seen.at(-1)).toBe("POST ack");
  });

  it("a malformed capability reaches no Home and is a rejection", async () => {
    let hit = false;
    const p = provider(() => { hit = true; return empty(); });
    expect((await p.perform({ call: "recovery_inbox_drain", ...hostOf, accountId: toHex(rand(32)), cap: toHex(rand(16)) })).result).toBe("err");
    expect(hit).toBe(false);
  });
});

function fromHexLocal(h: string): Uint8Array {
  return new Uint8Array((h.match(/../g) ?? []).map((b) => parseInt(b, 16)));
}
