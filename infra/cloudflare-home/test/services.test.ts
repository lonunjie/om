// SPDX-License-Identifier: MPL-2.0
// KeyMaterialProvider (PC-K), descriptor store (PC-G01) and IntroGate (PC-G02) cases against the real Worker bundle.
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import {
  concat, decodeDescriptorGet, decodeDrainResponse, decodeHomeError, decodeKeyFetchResponse, decodeTicketList, encode, encodeDescriptorPut, encodeIntroTicketRequest, encodeKeyFetch, encodeKeyPublish, encodeTicketParams,
  equalBytes, map, toHex, type HomeErrorBody,
} from "@om/home-wire";
import type { Home } from "./harness";
import { startHome } from "./harness";
import { asBody, bearer, messageRequest, nowS, rand } from "./kit";

let home: Home;
beforeAll(async () => { home = await startHome(); }, 120_000);
afterAll(async () => { await home.dispose(); });

const cbor = { "content-type": "application/cbor" };
async function send(path: string, init: RequestInit): Promise<{ status: number; body: Uint8Array; error?: HomeErrorBody }> {
  const r = await home.fetch(path, init);
  const body = new Uint8Array(await r.arrayBuffer());
  return r.status >= 200 && r.status < 300 ? { status: r.status, body } : { status: r.status, body, error: decodeHomeError(body) };
}

/** A structurally valid signed-AsyncKeyBundle shell: the Home reads device id, generation and expiry, and verifies nothing. */
function bundle(device: Uint8Array, generation: number, expiresAt = nowS() + 86400): Uint8Array {
  const inner = encode(map([[0, 9], [1, 1], [2, device], [5, 1], [6, generation], [12, rand(16)], [13, expiresAt]]));
  return encode(map([[1, inner], [2, [map([[0, device], [1, 1], [2, 3], [3, rand(64)]])]]]));
}

describe("PC-K key material", () => {
  const device = rand(16);
  const publishCap = rand(32);
  const put = (dev: Uint8Array, cap: Uint8Array, b: Uint8Array, opks: Uint8Array[] = [], pq: Uint8Array[] = []) =>
    send(`/v1/keys/bundles/${toHex(dev)}`, { method: "PUT", headers: { ...cbor, ...bearer(cap) }, body: encodeKeyPublish({ requestId: rand(16), bundle: b, opks, pqopks: pq }) });
  const fetchKeys = (dev: Uint8Array) =>
    send("/v1/keys/fetch", { method: "POST", headers: cbor, body: encodeKeyFetch({ requestId: rand(16), cap: rand(16), devices: [dev] }) });

  it("PC-K01/K02: publish claims the device; fetch returns the bundle then one-time items, consumed once", async () => {
    const opks = [rand(40), rand(40), rand(40)];
    const pq = [rand(50), rand(50)];
    expect((await put(device, publishCap, bundle(device, 1), opks, pq)).error).toBeUndefined();
    const stranger = await put(device, rand(32), bundle(device, 2));
    expect(stranger.error?.name).toBe("unauthorized");
    const f1 = await fetchKeys(device);
    const items = decodeKeyFetchResponse(f1.body).items;
    expect(items).toHaveLength(3);
    expect(equalBytes(items[1] ?? new Uint8Array(0), opks[0] ?? new Uint8Array(1))).toBe(true);
    expect(equalBytes(items[2] ?? new Uint8Array(0), pq[0] ?? new Uint8Array(1))).toBe(true);
    const f2 = decodeKeyFetchResponse((await fetchKeys(device)).body).items;
    expect(equalBytes(f2[1] ?? new Uint8Array(0), opks[1] ?? new Uint8Array(1))).toBe(true); // never the same item twice
  });

  it("PC-K05: a bundle for another device or a lower generation is refused; provider checks are availability aids only", async () => {
    expect((await put(device, publishCap, bundle(rand(16), 5))).error?.name).toBe("invalid_request");
    expect((await put(device, publishCap, bundle(device, 4))).error).toBeUndefined();
    expect((await put(device, publishCap, bundle(device, 3))).error?.name).toBe("stale_generation");
    expect((await put(device, publishCap, bundle(device, 5, nowS() - 86400))).error?.name).toBe("expired");
  });

  it("anonymous fetch cannot drain the pool: past the throttle a fetch gets the signed bundle without one-time items", async () => {
    const d = rand(16);
    const cap = rand(32);
    const many = Array.from({ length: 40 }, () => rand(40));
    await put(d, cap, bundle(d, 1), many, many);
    let lastLen = 3;
    for (let i = 0; i < 12; i++) lastLen = decodeKeyFetchResponse((await fetchKeys(d)).body).items.length;
    expect(lastLen).toBe(1);
  });

  it("limits and fetch of an unknown device", async () => {
    expect((await fetchKeys(rand(16))).error?.name).toBe("not_found");
    const d = rand(16);
    const cap = rand(32);
    expect((await put(d, cap, bundle(d, 1), Array.from({ length: 101 }, () => rand(8)))).error?.name).toBe("invalid_request"); // CDDL: at most 100
    expect((await put(d, cap, bundle(d, 1), Array.from({ length: 60 }, () => rand(8)))).error).toBeUndefined();
    const over = await put(d, cap, bundle(d, 1), Array.from({ length: 60 }, () => rand(8)));
    expect(over.error?.name).toBe("quota");
  });

  it("PC-K06: generation revoke is idempotent, needs the publish capability, and removes the stored pool", async () => {
    const d = rand(16);
    const cap = rand(32);
    await put(d, cap, bundle(d, 7), [rand(8)], [rand(8)]);
    const path = `/v1/keys/bundles/${toHex(d)}/generations/7`;
    expect((await send(path, { method: "DELETE", headers: bearer(rand(32)) })).error?.name).toBe("unauthorized");
    expect((await send(path, { method: "DELETE", headers: bearer(cap) })).error).toBeUndefined();
    expect((await send(path, { method: "DELETE", headers: bearer(cap) })).error).toBeUndefined();
    expect((await fetchKeys(d)).error?.name).toBe("not_found");
  });
});

describe("PC-G01 descriptors", () => {
  const url = (id: Uint8Array): string => `/v1/discovery/descriptors/${toHex(id)}`;
  const put = (id: Uint8Array, cap: Uint8Array, body: Uint8Array, expiresAt = nowS() + 3600) =>
    send(url(id), { method: "PUT", headers: { ...cbor, ...bearer(cap) }, body: encodeDescriptorPut({ requestId: rand(16), body, expiresAt }) });

  it("serves the current descriptor, replaces it, and tombstones a revoked one (not_found, never republishable by a stranger)", async () => {
    const id = rand(16);
    const cap = rand(32);
    expect((await put(id, cap, rand(300))).error).toBeUndefined();
    const second = rand(400);
    expect((await put(id, cap, second)).error).toBeUndefined();
    const got = decodeDescriptorGet((await send(url(id), { method: "GET" })).body);
    expect(equalBytes(got.body, second)).toBe(true);
    expect((await put(id, rand(32), rand(10))).error?.name).toBe("unauthorized");
    expect((await send(url(id), { method: "DELETE", headers: bearer(rand(32)) })).error?.name).toBe("unauthorized");
    expect((await send(url(id), { method: "DELETE", headers: bearer(cap) })).error).toBeUndefined();
    expect((await send(url(id), { method: "DELETE", headers: bearer(cap) })).error).toBeUndefined();
    expect((await send(url(id), { method: "GET" })).error?.name).toBe("not_found");
    expect((await put(id, cap, rand(10))).error?.name).toBe("revoked");
  });

  it("revoking before the publish landed still tombstones; expired and oversize are typed; unknown ids are not_found", async () => {
    const id = rand(16);
    const cap = rand(32);
    expect((await send(url(id), { method: "DELETE", headers: bearer(cap) })).error).toBeUndefined();
    expect((await put(id, cap, rand(10))).error?.name).toBe("revoked");
    expect((await put(rand(16), rand(32), rand(10), nowS() - 7200)).error?.name).toBe("expired");
    expect((await put(rand(16), rand(32), rand(40_000))).error?.name).toBe("payload_too_large");
    expect((await send(url(rand(16)), { method: "GET" })).error?.name).toBe("not_found");
  });

  it("rate-limits anonymous reads of one descriptor", async () => {
    const id = rand(16);
    await put(id, rand(32), rand(10));
    let limited: HomeErrorBody | undefined;
    for (let i = 0; i < 40 && !limited; i++) limited = (await send(url(id), { method: "GET" })).error;
    expect(limited?.name).toBe("rate_limited");
    expect(limited?.retryable).toBe(true);
  });
});

describe("PC-G02 IntroGate", () => {
  const service = concat(rand(8), rand(24));
  const issue = (cap: Uint8Array, p: { ttlSeconds: number; maxUses: number; payloadLimitBytes: number }, count = 1, requestId = rand(16)) =>
    send("/v1/intro/tickets", { method: "POST", headers: { ...cbor, ...bearer(cap) }, body: encodeIntroTicketRequest({ requestId, params: encodeTicketParams(p), count }) });
  const submit = (b: Uint8Array) => send("/v1/intro/requests", { method: "POST", headers: cbor, body: asBody(b) });
  const drain = (cap: Uint8Array, after = 0) =>
    send("/v1/intro/drain", { method: "POST", headers: cbor, body: encode(map([[0, 1], [1, rand(16)], [2, cap], [3, after]])) });

  it("a single-use ticket admits one request; exact retries are idempotent; a second distinct request is spent_token", async () => {
    const t = decodeTicketList((await issue(service, { ttlSeconds: 3600, maxUses: 1, payloadLimitBytes: 2048 })).body)[0];
    if (!t) throw new Error("no ticket");
    expect(equalBytes(t.ticketId.slice(0, 8), service.slice(0, 8))).toBe(true);
    const req = messageRequest(t.ticketId);
    expect((await submit(req)).error).toBeUndefined();
    expect((await submit(req)).error).toBeUndefined(); // lost-response retry does not consume another use
    expect((await submit(messageRequest(t.ticketId))).error?.name).toBe("spent_token");
    const drained = await drain(service);
    expect(drained.error).toBeUndefined();
  });

  it("rejects before expensive work: unknown ticket, oversize payload, wrong object, foreign service capability", async () => {
    const t = decodeTicketList((await issue(service, { ttlSeconds: 3600, maxUses: 4, payloadLimitBytes: 700 })).body)[0];
    if (!t) throw new Error("no ticket");
    expect((await submit(messageRequest(concat(t.ticketId.slice(0, 8), rand(8))))).error?.name).toBe("admission_denied");
    expect((await submit(messageRequest(t.ticketId, 5000))).error?.name).toBe("payload_too_large");
    expect((await submit(rand(100))).error?.name).toBe("invalid_request");
    expect((await issue(concat(service.slice(0, 8), rand(24)), { ttlSeconds: 60, maxUses: 1, payloadLimitBytes: 100 })).error?.name).toBe("unauthorized");
    expect((await drain(concat(service.slice(0, 8), rand(24)))).error?.name).toBe("unauthorized");
    expect((await issue(service, { ttlSeconds: 365 * 86400, maxUses: 1, payloadLimitBytes: 100 })).error?.name).toBe("policy_denied");
  });

  it("drain returns requests in order and the next drain with `after` acknowledges them", async () => {
    const s2 = concat(rand(8), rand(24));
    const tickets = decodeTicketList((await issue(s2, { ttlSeconds: 3600, maxUses: 1, payloadLimitBytes: 2048 }, 3)).body);
    expect(tickets).toHaveLength(3);
    for (const t of tickets) expect((await submit(messageRequest(t.ticketId))).error).toBeUndefined();
    const a = decodeDrainResponse((await drain(s2)).body);
    expect(a.items).toHaveLength(3);
    const seqs = a.items.map((i) => i.seq ?? 0);
    expect(seqs).toEqual([...seqs].sort((x, y) => x - y));
    const b = decodeDrainResponse((await drain(s2, seqs[0] ?? 0)).body);
    expect(b.items).toHaveLength(2);
    expect(decodeDrainResponse((await drain(s2)).body).items).toHaveLength(2); // the first was acknowledged by `after`
  });

  it("a retried issue request returns the same tickets", async () => {
    const s3 = concat(rand(8), rand(24));
    const rid = rand(16);
    const a = decodeTicketList((await issue(s3, { ttlSeconds: 600, maxUses: 2, payloadLimitBytes: 1000 }, 2, rid)).body);
    const b = decodeTicketList((await issue(s3, { ttlSeconds: 600, maxUses: 2, payloadLimitBytes: 1000 }, 2, rid)).body);
    expect(a.map((t) => toHex(t.ticketId))).toEqual(b.map((t) => toHex(t.ticketId)));
  });
});
