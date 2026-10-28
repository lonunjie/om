// SPDX-License-Identifier: MPL-2.0
import { describe, expect, it } from "vitest";
import { HomeError, decodeDepositRequest, encodeHomeError, fromHex, toHex } from "@om/home-wire";
import { createHttpProvider, createIndexedDbSecretStore, createMemorySecretStore, parseProviderUrl, type HttpProvider } from "./index";

const cfg = (u = "https://home.example"): { baseUrl: string } => {
  const c = parseProviderUrl(u);
  if (!c) throw new Error("bad url");
  return c;
};
const rand = (n: number): Uint8Array => crypto.getRandomValues(new Uint8Array(n));
const urlOf = (i: RequestInfo | URL): string => (typeof i === "string" ? i : i instanceof URL ? i.href : i.url);

describe("provider URL (configuration)", () => {
  it("accepts https; refuses http except an opted-in loopback; refuses anything that could carry a capability", () => {
    expect(parseProviderUrl("https://om-home.example.workers.dev")?.baseUrl).toBe("https://om-home.example.workers.dev");
    expect(parseProviderUrl("https://relay.example/om/")?.baseUrl).toBe("https://relay.example/om");
    expect(parseProviderUrl("http://relay.example")).toBeUndefined();
    expect(parseProviderUrl("http://127.0.0.1:8787")).toBeUndefined();
    expect(parseProviderUrl("http://127.0.0.1:8787", { allowLoopbackHttp: true })?.baseUrl).toBe("http://127.0.0.1:8787");
    expect(parseProviderUrl("http://relay.example", { allowLoopbackHttp: true })).toBeUndefined();
    for (const bad of ["https://u:p@relay.example", "https://relay.example/?cap=abc", "https://relay.example/#frag", "ftp://relay.example", "not a url", "", "javascript:alert(1)"]) {
      expect(parseProviderUrl(bad), bad).toBeUndefined();
    }
  });
});

describe("failure classification (stubbed Home)", () => {
  const typed = (e: HomeError, status = 400): Response => new Response(encodeHomeError(e) as BodyInit, { status, headers: { "content-type": "application/cbor" } });
  const mk = (handler: (url: string, init: RequestInit) => Response | Promise<Response>): HttpProvider =>
    createHttpProvider({ config: cfg(), secrets: createMemorySecretStore(), timeoutMs: 1000, fetch: async (i, init) => handler(urlOf(i), init ?? {}) });
  const deposit = { call: "deposit", mailbox: toHex(rand(16)), bytes: toHex(rand(40)) } as const;

  it("maps every registered Home error to a Core class; retryable ones never become rejections", async () => {
    const table: [HomeError, unknown][] = [
      [new HomeError("revoked"), { kind: "rejected", reason: "revoked" }],
      [new HomeError("expired"), { kind: "rejected", reason: "expired" }],
      [new HomeError("unauthorized"), { kind: "rejected", reason: "unauthorized" }],
      [new HomeError("payload_too_large"), { kind: "rejected", reason: "invalid" }],
      [new HomeError("invalid_request"), { kind: "rejected", reason: "invalid" }],
      [new HomeError("stale_generation"), { kind: "rejected", reason: "stale_state" }],
      [new HomeError("invalid_signature"), { kind: "rejected", reason: "integrity_failure" }],
      [new HomeError("policy_denied"), { kind: "rejected", reason: "policy_denied" }],
      [new HomeError("spent_token"), { kind: "rejected", reason: "conflict" }],
      [new HomeError("moved"), { kind: "rejected", reason: "unsupported" }], // never followed without a verified proof
      [new HomeError("not_found"), { kind: "not_found" }],
      [new HomeError("retryable"), { kind: "unavailable" }],
      [new HomeError("quota"), { kind: "unavailable" }], // also what a queue that does not exist yet answers
      [new HomeError("rate_limited", { retryAfterSeconds: 3 }), { kind: "unavailable" }],
    ];
    for (const [err, want] of table) {
      const p = mk(() => typed(err));
      expect(await p.perform({ ...deposit, mailbox: toHex(rand(16)) }), err.errorName).toEqual({ result: "err", error: want });
    }
  });

  it("an unknown error code is a non-retryable protocol failure", async () => {
    const body = new Uint8Array([0xa2, 0x00, 0x01, 0x01, 0x18, 0x63]);
    const p = mk(() => new Response(body as BodyInit, { status: 400, headers: { "content-type": "application/cbor" } }));
    expect(await p.perform(deposit)).toEqual({ result: "err", error: { kind: "rejected", reason: "unsupported" } });
  });

  it("a mutating request with no answer is `timeout` if the Home is reachable and `unavailable` if it is not; reads are always `unavailable`", async () => {
    let probeUp = true;
    const lost = mk((url) => {
      if (url.endsWith("/.well-known/om-provider-capabilities") && probeUp) return new Response("{}", { status: 200 });
      return Promise.reject(new TypeError(probeUp ? "connection reset" : "down"));
    });
    expect(await lost.perform(deposit)).toEqual({ result: "err", error: { kind: "timeout" } });
    probeUp = false;
    expect(await lost.perform(deposit)).toEqual({ result: "err", error: { kind: "unavailable" } });
    expect(await lost.perform({ call: "get_invite", inviteId: toHex(rand(16)) })).toEqual({ result: "err", error: { kind: "unavailable" } });
    expect(await lost.perform({ call: "fetch_key_material", deviceId: toHex(rand(16)) })).toEqual({ result: "err", error: { kind: "unavailable" } });
  });

  it("a gateway error page (no typed body) after a mutation is unknown, not a rejection", async () => {
    const p = mk((url) => (url.endsWith("capabilities") ? new Response("{}", { status: 200 }) : new Response("<html>502</html>", { status: 502, headers: { "content-type": "text/html" } })));
    expect(await p.perform(deposit)).toEqual({ result: "err", error: { kind: "timeout" } });
  });

  it("quota and rate limits back off locally instead of hammering the Home", async () => {
    let calls = 0;
    const p = mk(() => { calls += 1; return typed(new HomeError("rate_limited", { retryAfterSeconds: 30 })); });
    const d = { ...deposit, mailbox: toHex(rand(16)) };
    expect((await p.perform(d)).result).toBe("err");
    expect((await p.perform(d)).result).toBe("err");
    expect((await p.perform(d)).result).toBe("err");
    expect(calls).toBe(1);
  });
});

describe("routes (audit M1)", () => {
  const sink = (seen: { url: string; cap: string }[]): HttpProvider => createHttpProvider({
    config: cfg(), secrets: createMemorySecretStore(),
    fetch: (i, init) => { seen.push({ url: urlOf(i), cap: toHex(decodeDepositRequest(init?.body as Uint8Array).cap) }); return Promise.resolve(new Response(null, { status: 204 })); },
  });

  it("allocates fresh mailboxes whose route is this Home's relay uri and the deposit capability the Home derives for them", async () => {
    const p = sink([]);
    const offers = await p.allocateRoutes(3);
    expect(offers).toHaveLength(3);
    expect(new Set(offers.map((o) => o.mailbox)).size).toBe(3);
    for (const o of offers) {
      expect(o.mailbox).toMatch(/^[0-9a-f]{32}$/);
      expect(o.relayUri).toBe("https://home.example/");
      const entry = await p.routeEntry(fromHex(o.mailbox), 1);
      expect(entry.relayUri).toBe(o.relayUri);
      expect(o.capability).toBe(toHex(entry.capability));
      expect(o.capability).toHaveLength(64);
    }
  });

  it("deposits to a route's capability as given when the route is at this Home; refuses another Home's route; derives for a legacy destination", async () => {
    const seen: { url: string; cap: string }[] = [];
    const p = sink(seen);
    const [o] = await p.allocateRoutes(1);
    if (!o) throw new Error("no offer");
    expect(await p.perform({ call: "deposit", mailbox: o.capability, relayUri: o.relayUri, bytes: toHex(rand(40)) })).toEqual({ result: "ok" });
    expect(seen.map((s) => s.cap)).toEqual([o.capability]);
    // Another Home's route is not deposited at the wrong place.
    const other = await p.perform({ call: "deposit", mailbox: o.capability, relayUri: "https://other.example/", bytes: toHex(rand(40)) });
    expect(other).toEqual({ result: "err", error: { kind: "rejected", reason: "unsupported" } });
    expect(seen).toHaveLength(1);
    // A capability outside the spec's bounds is invalid, not sent.
    expect(await p.perform({ call: "deposit", mailbox: toHex(rand(8)), relayUri: o.relayUri, bytes: toHex(rand(40)) })).toEqual({ result: "err", error: { kind: "rejected", reason: "invalid" } });
    // A legacy destination (bare mailbox, no relay name) still gets the derived capability.
    await p.perform({ call: "deposit", mailbox: o.mailbox, bytes: toHex(rand(40)) });
    expect(seen[1]?.cap).toBe(o.capability);
  });
});

describe("request hygiene", () => {
  it("bearer capabilities are in the Authorization header or the body, never in a URL; no cookies, no redirects, no referrer", async () => {
    const seen: { url: string; init: RequestInit }[] = [];
    const p = createHttpProvider({
      config: cfg(), secrets: createMemorySecretStore(),
      fetch: (i, init) => { seen.push({ url: urlOf(i), init: init ?? {} }); return Promise.resolve(new Response(null, { status: 204 })); },
    });
    const dev = toHex(rand(16));
    await p.perform({ call: "deposit", mailbox: toHex(rand(16)), bytes: toHex(rand(40)) });
    await p.perform({ call: "publish_key_material", deviceId: dev, bundle: toHex(rand(50)), opks: [toHex(rand(8))], pqopks: [] });
    await p.perform({ call: "put_invite", inviteId: toHex(rand(16)), descriptor: toHex(rand(30)), expiresAt: Date.now() + 1e6 });
    await p.perform({ call: "revoke_invite", inviteId: toHex(rand(16)) });
    expect(seen.length).toBeGreaterThanOrEqual(4);
    for (const { url, init } of seen) {
      expect(url).not.toMatch(/[?#]/);
      expect(init.credentials).toBe("omit");
      expect(init.redirect).toBe("error");
      expect(init.referrerPolicy).toBe("no-referrer");
      expect(init.cache).toBe("no-store");
    }
    const keyPut = seen.find((s) => s.url.endsWith(`/v1/keys/bundles/${dev}`));
    expect((keyPut?.init.headers as Record<string, string>)["authorization"]).toMatch(/^Bearer [A-Za-z0-9_-]{40,}$/);
    // the URL carries only the public device id, never the publish capability
    const cap = (keyPut?.init.headers as Record<string, string>)["authorization"]?.slice(7) ?? "";
    expect(keyPut?.url.includes(cap)).toBe(false);
  });

  it("request ids are content-derived: an exact retry carries the same id, different bytes a different one", async () => {
    const ids: string[] = [];
    const secrets = createMemorySecretStore();
    const mk2 = (): HttpProvider => createHttpProvider({
      config: cfg(), secrets,
      fetch: (_i, init) => { ids.push(toHex(decodeDepositRequest(init?.body as Uint8Array).requestId)); return Promise.resolve(new Response(null, { status: 204 })); },
    });
    const mailbox = toHex(rand(16));
    const bytes = toHex(rand(40));
    await mk2().perform({ call: "deposit", mailbox, bytes });
    await mk2().perform({ call: "deposit", mailbox, bytes });
    await mk2().perform({ call: "deposit", mailbox, bytes: toHex(rand(40)) });
    expect(ids[0]).toBe(ids[1]);
    expect(ids[2]).not.toBe(ids[0]);
  });
});

describe("secret store", () => {
  it("IndexedDB store persists the install secret across adapter instances", async () => {
    const { IDBFactory } = await import("fake-indexeddb");
    const idb = new IDBFactory();
    const a = createIndexedDbSecretStore(idb);
    expect(await a.get("x")).toBeUndefined();
    await a.put("x", new Uint8Array([1, 2, 3]));
    expect(Array.from((await createIndexedDbSecretStore(idb).get("x")) ?? [])).toEqual([1, 2, 3]);
  });
});

