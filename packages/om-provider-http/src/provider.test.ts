// SPDX-License-Identifier: MPL-2.0
import { describe, expect, it } from "vitest";
import { HomeError, blobCiphertextDigest, decode, decodeAckRequest, decodeDepositRequest, decodeDrainRequest, encodeBlobDigestHeader, encodeBlobPutResponse, encodeDrainResponse, encodeHomeError, fromHex, toBase64Url, toHex, type CborValue } from "@om/home-wire";
import type { NetCall } from "@om/core-wasm";
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

  it("deposits to a route's capability as given when the route is at this Home; derives for a legacy destination", async () => {
    const seen: { url: string; cap: string }[] = [];
    const p = sink(seen);
    const [o] = await p.allocateRoutes(1);
    if (!o) throw new Error("no offer");
    expect(await p.perform({ call: "deposit", mailbox: o.capability, relayUri: o.relayUri, bytes: toHex(rand(40)) })).toEqual({ result: "ok" });
    expect(seen.map((s) => s.cap)).toEqual([o.capability]);
    // A capability outside the spec's bounds is invalid, not sent.
    expect(await p.perform({ call: "deposit", mailbox: toHex(rand(8)), relayUri: o.relayUri, bytes: toHex(rand(40)) })).toEqual({ result: "err", error: { kind: "rejected", reason: "invalid" } });
    // A legacy destination (bare mailbox, no relay name) still gets the derived capability.
    await p.perform({ call: "deposit", mailbox: o.mailbox, bytes: toHex(rand(40)) });
    expect(seen[1]?.cap).toBe(o.capability);
  });
});

describe("a peer's Home (cross-Home first contact and delivery)", () => {
  const PEER = "https://peer.example";
  const doc = (): Response => new Response(JSON.stringify({ schema_version: 1, provider_id: "peer", provider_kinds: ["relay"], api_versions: ["1"], features: [] }), { status: 200, headers: { "content-type": "application/json" } });
  /** A stub of two Homes: records every request with the Home it went to and whether it carried the admission header. */
  function world(opts: { peerDoc?: () => Response; peerDown?: boolean } = {}) {
    const seen: { home: string; path: string; method: string; admission: boolean; auth: boolean }[] = [];
    const secret = rand(32);
    const p = createHttpProvider({
      config: cfg(), secrets: createMemorySecretStore(), admission: secret, timeoutMs: 1000,
      fetch: (i, init) => {
        const u = new URL(urlOf(i));
        const h = new Headers(init?.headers);
        seen.push({ home: u.origin, path: u.pathname, method: init?.method ?? "GET", admission: h.has("om-admission"), auth: h.has("authorization") });
        if (u.origin === PEER && opts.peerDown) return Promise.reject(new TypeError("down"));
        if (u.pathname === "/.well-known/om-provider-capabilities") return Promise.resolve(u.origin === PEER ? (opts.peerDoc ?? doc)() : doc());
        return Promise.resolve(new Response(null, { status: 204 }));
      },
    });
    return { p, seen };
  }
  const cap = toHex(rand(32));

  it("deposits to the Home the route names, after one capabilities read, and never sends it the owner's admission secret", async () => {
    const { p, seen } = world();
    expect(await p.perform({ call: "deposit", mailbox: cap, relayUri: `${PEER}/`, bytes: toHex(rand(40)) })).toEqual({ result: "ok" });
    expect(await p.perform({ call: "deposit", mailbox: cap, relayUri: `${PEER}/`, bytes: toHex(rand(41)) })).toEqual({ result: "ok" });
    const atPeer = seen.filter((s) => s.home === PEER);
    expect(atPeer.map((s) => s.path)).toEqual(["/.well-known/om-provider-capabilities", "/v1/relay/deposit", "/v1/relay/deposit"]); // vetted once, not per call
    expect(seen.every((s) => s.home === PEER)).toBe(true); // nothing went to the configured Home
    expect(atPeer.some((s) => s.admission)).toBe(false);
  });

  it("refuses a Home it will not contact: not https, credentials, or something that does not answer like an OM Home", async () => {
    const { p, seen } = world({ peerDoc: () => new Response("<html>hello</html>", { status: 200 }) });
    for (const relayUri of ["http://peer.example/", "https://u:p@peer.example/", "https://peer.example/?x=1", "ftp://peer.example/", "nonsense"]) {
      expect(await p.perform({ call: "deposit", mailbox: cap, relayUri, bytes: toHex(rand(40)) }), relayUri).toEqual({ result: "err", error: { kind: "rejected", reason: "unsupported" } });
    }
    expect(seen).toHaveLength(0); // nothing was sent to any of them
    expect(await p.perform({ call: "deposit", mailbox: cap, relayUri: `${PEER}/`, bytes: toHex(rand(40)) })).toEqual({ result: "err", error: { kind: "rejected", reason: "unsupported" } });
    expect(seen.map((s) => s.path)).toEqual(["/.well-known/om-provider-capabilities"]); // the read, and no deposit
  });

  it("a peer's Home that is down is a retry, never a rejection, and does not mark the person's own Home unreachable", async () => {
    const { p } = world({ peerDown: true });
    expect(await p.perform({ call: "deposit", mailbox: cap, relayUri: `${PEER}/`, bytes: toHex(rand(40)) })).toEqual({ result: "err", error: { kind: "unavailable" } });
    expect(p.reachability()).toBe("unknown");
  });

  it("reads an invite from the service the link names, submits the request to the one the descriptor names, and fetches keys with the relationship capability", async () => {
    const { p, seen } = world();
    const id = toHex(rand(16));
    const r = await p.perform({ call: "get_invite", inviteId: id, service: `${PEER}/` });
    expect(r.result).toBe("err"); // the stub answers 204 with no descriptor: what matters is where it went
    expect(seen.find((s) => s.path === `/v1/discovery/descriptors/${id}`)).toMatchObject({ home: PEER, method: "GET", auth: false });
    await p.perform({ call: "submit_intro", service: `${PEER}/`, bytes: toHex(rand(100)) });
    expect(seen.find((s) => s.path === "/v1/intro/requests")).toMatchObject({ home: PEER, method: "POST", auth: false });
    await p.perform({ call: "fetch_key_material", deviceId: toHex(rand(16)), service: `${PEER}/`, capability: cap });
    expect(seen.find((s) => s.path === "/v1/keys/fetch")).toMatchObject({ home: PEER });
    expect(seen.filter((s) => s.home === PEER).some((s) => s.admission)).toBe(false);
  });

  it("owner operations stay at the configured Home: grants, tickets and the intro drain never go to a peer", async () => {
    const { p, seen } = world();
    const device = toHex(rand(16));
    await p.perform({ call: "grant_key_fetch", deviceId: device, capability: cap });
    await p.perform({ call: "put_invite", inviteId: toHex(rand(16)), descriptor: toHex(rand(50)), expiresAt: Date.now() + 86_400_000, payloadLimit: 32_768 });
    await p.poll(toHex(rand(16)), 0, "intro");
    const mine = seen.filter((s) => ["/v1/intro/tickets", "/v1/intro/drain"].includes(s.path) || s.path.endsWith("/fetch-caps"));
    expect(mine.length).toBe(3);
    expect(mine.every((s) => s.home === "https://home.example")).toBe(true);
  });

  it("a descriptor that names no Home is read from the configured one, as a link without a service never reaches here", async () => {
    const { p, seen } = world();
    await p.perform({ call: "get_invite", inviteId: toHex(rand(16)) });
    expect(seen.every((s) => s.home === "https://home.example")).toBe(true);
  });
});

describe("device-link ceremony queues (deviation D24)", () => {
  const PEER = "https://peer.example";
  const doc = (): Response => new Response(JSON.stringify({ schema_version: 1, provider_id: "peer", provider_kinds: ["relay"], api_versions: ["1"], features: [] }), { status: 200, headers: { "content-type": "application/json" } });
  const item = { id: rand(16), bytes: rand(30), seq: 5 };
  /** A stub of the existing device's Home (the peer, from the new device's side): records every request and serves one queued item. */
  function world() {
    const seen: { home: string; path: string; cap?: string; admission: boolean }[] = [];
    const p = createHttpProvider({
      config: cfg(), secrets: createMemorySecretStore(), admission: rand(32), timeoutMs: 1000,
      fetch: (i, init) => {
        const u = new URL(urlOf(i));
        const h = new Headers(init?.headers);
        const body = init?.body instanceof Uint8Array ? init.body : undefined;
        let cap: string | undefined;
        if (u.pathname === "/v1/relay/drain" && body) cap = toHex(decodeDrainRequest(body).cap);
        if (u.pathname === "/v1/relay/ack" && body) cap = toHex(decodeAckRequest(body).cap);
        seen.push({ home: u.origin, path: u.pathname, ...(cap ? { cap } : {}), admission: h.has("om-admission") });
        if (u.pathname === "/.well-known/om-provider-capabilities") return Promise.resolve(doc());
        if (u.pathname === "/v1/relay/drain") return Promise.resolve(new Response(encodeDrainResponse({ items: [item] }) as BodyInit, { status: 200, headers: { "content-type": "application/cbor" } }));
        return Promise.resolve(new Response(null, { status: 204 }));
      },
    });
    return { p, seen };
  }

  it("opens a short-lived queue at its own Home and answers the receive capability the other device will present", async () => {
    const { p, seen } = world();
    const r = await p.perform({ call: "open_queue", mailbox: toHex(rand(16)), relayUri: "https://home.example/" });
    expect(r.result).toBe("bytes");
    expect(r.result === "bytes" && r.bytes).toMatch(/^[0-9a-f]{64,}$/);
    expect(seen.map((s) => s.path)).toEqual(["/v1/relay/queues"]);
  });

  it("refuses to open a queue at any Home but its own, and sends nothing", async () => {
    const { p, seen } = world();
    expect(await p.perform({ call: "open_queue", mailbox: toHex(rand(16)), relayUri: `${PEER}/` })).toEqual({ result: "err", error: { kind: "rejected", reason: "unsupported" } });
    expect(seen).toHaveLength(0);
  });

  it("drains and acknowledges another install's queue with the capability its link carried, at the Home the link names; it claims no queue and never sends the owner's admission", async () => {
    const { p, seen } = world();
    const capability = toHex(rand(48));
    const got = await p.poll(toHex(rand(16)), 0, "relay", `${PEER}/`, capability);
    expect(got).toEqual([{ seq: 5, bytes: toHex(item.bytes) }]);
    // Acknowledge by that capability too (the item id was learned by the drain above).
    const mailbox = toHex(rand(16));
    const again = await p.poll(mailbox, 0, "relay", `${PEER}/`, capability);
    expect(again).toHaveLength(1);
    expect(await p.perform({ call: "ack", mailbox, seq: 5, relayUri: `${PEER}/`, capability })).toEqual({ result: "ok" });
    expect(seen.every((s) => s.home === PEER)).toBe(true); // nothing went to the person's own Home
    expect(seen.some((s) => s.path === "/v1/relay/queues")).toBe(false); // it never creates or claims the queue
    expect(seen.some((s) => s.admission)).toBe(false);
    const withCap = seen.filter((s) => s.cap !== undefined);
    expect(withCap.length).toBeGreaterThanOrEqual(3);
    expect(withCap.every((s) => s.cap === capability)).toBe(true);
  });

  it("fails closed: a malformed capability, an unreachable Home or a bad link polls nothing and acknowledges nothing", async () => {
    const { p, seen } = world();
    expect(await p.poll(toHex(rand(16)), 0, "relay", `${PEER}/`, "zz-not-hex")).toEqual([]);
    expect(await p.poll(toHex(rand(16)), 0, "relay", "http://peer.example/", toHex(rand(48)))).toEqual([]);
    expect(await p.perform({ call: "ack", mailbox: toHex(rand(16)), seq: 1, relayUri: "ftp://peer.example/", capability: toHex(rand(48)) })).toEqual({ result: "err", error: { kind: "rejected", reason: "unsupported" } });
    expect(seen.some((s) => s.path === "/v1/relay/drain")).toBe(false);
  });
});

describe("revoke_key_generation", () => {
  it("is a DELETE of that generation under the device's bundle, authorised by the owner's key-publish capability", async () => {
    const seen: { url: string; init: RequestInit }[] = [];
    const p = createHttpProvider({
      config: cfg(), secrets: createMemorySecretStore(),
      fetch: (i, init) => { seen.push({ url: urlOf(i), init: init ?? {} }); return Promise.resolve(new Response(null, { status: 204 })); },
    });
    const dev = toHex(rand(16));
    expect(await p.perform({ call: "revoke_key_generation", deviceId: dev, generation: 3 })).toEqual({ result: "ok" });
    expect(seen).toHaveLength(1);
    expect(seen[0]?.url.endsWith(`/v1/keys/bundles/${dev}/generations/3`)).toBe(true);
    expect(seen[0]?.init.method).toBe("DELETE");
    expect((seen[0]?.init.headers as Record<string, string>)["authorization"]).toMatch(/^Bearer [A-Za-z0-9_-]{40,}$/);
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


describe("files (om.blob.spool.v1), stubbed Home", () => {
  const typed = (e: HomeError, status = 400): Response => new Response(encodeHomeError(e) as BodyInit, { status, headers: { "content-type": "application/cbor" } });
  const mk = (handler: (url: string, init: RequestInit) => Response | Promise<Response>, extra: { allowLoopbackHttp?: boolean; baseUrl?: string } = {}): HttpProvider => {
    const config = parseProviderUrl(extra.baseUrl ?? "https://home.example", { allowLoopbackHttp: extra.allowLoopbackHttp === true });
    if (!config) throw new Error("bad url");
    return createHttpProvider({ config, secrets: createMemorySecretStore(), timeoutMs: 1000, admission: rand(32), ...(extra.allowLoopbackHttp ? { allowLoopbackHttp: true } : {}), fetch: async (i, init) => handler(urlOf(i), init ?? {}) });
  };
  const sealed = async (): Promise<{ ct: Uint8Array; call: Extract<NetCall, { call: "put_blob" }> }> => {
    const ct = rand(300);
    const ownerCap = rand(48);
    return { ct, call: { call: "put_blob", blobId: toHex(rand(32)), ownerCap: toHex(ownerCap), requestId: toHex(rand(16)), digest: toHex(await blobCiphertextDigest(ct)), length: ct.length } };
  };
  const putReply = (call: { ownerCap: string; digest: string }, over: { digest?: Uint8Array; locator?: Uint8Array } = {}): Response =>
    new Response(encodeBlobPutResponse({ readCap: Uint8Array.from([...(over.locator ?? fromHex(call.ownerCap).slice(0, 16)), ...rand(32)]), digest: over.digest ?? fromHex(call.digest), expiresAt: 1_900_000_000 }) as BodyInit, { status: 200, headers: { "content-type": "application/cbor" } });

  it("put sends the digest header and the owner admission, and answers the receipt Core expects", async () => {
    const { ct, call } = await sealed();
    let seen: { url: string; headers: Record<string, string> } | undefined;
    const p = mk((url, init) => { seen = { url, headers: init.headers as Record<string, string> }; return putReply(call); });
    const r = await p.perform(call, { ciphertext: ct });
    expect(seen?.url).toBe("https://home.example/v1/blobs");
    expect(seen?.headers["om-ciphertext-digest"]).toBe(encodeBlobDigestHeader(fromHex(call.digest)));
    expect(seen?.headers["om-admission"]).toBeDefined();
    if (r.result !== "bytes") throw new Error(JSON.stringify(r));
    const receipt = decode(fromHex(r.bytes));
    const m = receipt as Map<number, CborValue>;
    expect(m.get(2)).toEqual(fromHex(call.digest));
    expect(m.get(1)).toEqual([`https://home.example/v1/blobs/${call.ownerCap.slice(0, 32)}`]);
    expect((m.get(0) as Uint8Array).length).toBe(48);
  });

  it("maps payload_too_large to too_large, a full spool to quota, an absent feature to unsupported; a timeout stays unknown", async () => {
    const { ct, call } = await sealed();
    const cases: [HomeError, unknown][] = [
      [new HomeError("payload_too_large"), { kind: "rejected", reason: "too_large" }],
      [new HomeError("quota"), { kind: "rejected", reason: "quota" }],
      [new HomeError("not_found"), { kind: "rejected", reason: "unsupported" }],
      [new HomeError("unauthorized"), { kind: "rejected", reason: "unauthorized" }],
      [new HomeError("retryable"), { kind: "unavailable" }],
    ];
    for (const [e, want] of cases) expect(await mk(() => typed(e)).perform(call, { ciphertext: ct }), e.errorName).toEqual({ result: "err", error: want });
    let up = true;
    const lost = mk((url) => (url.endsWith("capabilities") && up ? new Response("{}") : Promise.reject(new TypeError("reset"))));
    expect(await lost.perform(call, { ciphertext: ct })).toEqual({ result: "err", error: { kind: "timeout" } });
    up = false;
    expect(await lost.perform(call, { ciphertext: ct })).toEqual({ result: "err", error: { kind: "unavailable" } });
  });

  it("never sends bytes that are not what Core sealed, and refuses a Home that stored something else (and removes it)", async () => {
    const { ct, call } = await sealed();
    let calls = 0;
    const p = mk(() => { calls += 1; return putReply(call); });
    expect(await p.perform(call)).toEqual({ result: "err", error: { kind: "rejected", reason: "integrity_failure" } }); // no staged bytes
    expect(await p.perform(call, { ciphertext: ct.slice(1) })).toEqual({ result: "err", error: { kind: "rejected", reason: "integrity_failure" } });
    expect(calls).toBe(0);
    const methods: string[] = [];
    const lying = mk((_url, init) => { methods.push(init.method ?? ""); return putReply(call, { digest: rand(32) }); });
    expect(await lying.perform(call, { ciphertext: ct })).toEqual({ result: "err", error: { kind: "rejected", reason: "integrity_failure" } });
    await new Promise((r) => setTimeout(r, 10));
    expect(methods).toEqual(["POST", "DELETE"]);
  });

  it("get tries locations in order with the read capability as bearer, and hands back only bytes that hash to the digest", async () => {
    const ct = rand(200);
    const digest = toHex(await blobCiphertextDigest(ct));
    const readCap = rand(48);
    const loc = (host: string): string => `https://${host}/v1/blobs/${toHex(readCap.slice(0, 16))}`;
    const seen: string[] = [];
    const p = mk((url, init) => {
      seen.push(`${init.method ?? ""} ${url}`);
      if (url.endsWith("/.well-known/om-provider-capabilities")) return new Response(JSON.stringify({ schema_version: 1, api_versions: ["1"] }));
      if (url.startsWith("https://gone.example/")) return typed(new HomeError("not_found"), 404);
      expect((init.headers as Record<string, string>)["authorization"]).toBe(`Bearer ${toBase64Url(readCap)}`);
      return new Response(ct as BodyInit, { status: 200, headers: { "content-type": "application/octet-stream" } });
    });
    const call = { call: "get_blob", locations: [loc("gone.example"), loc("home.example")], readCap: toHex(readCap), digest, length: ct.length } as const;
    expect(await p.perform(call)).toEqual({ result: "bytes", bytes: toHex(ct) });
    expect(seen.some((s) => s.startsWith("GET https://gone.example/v1/blobs/"))).toBe(true);
    // The object changed under the same name: refused, never passed on.
    expect(await p.perform({ ...call, digest: toHex(rand(32)) })).toEqual({ result: "err", error: { kind: "rejected", reason: "integrity_failure" } });
    expect(await p.perform({ ...call, length: ct.length + 1 })).toEqual({ result: "err", error: { kind: "rejected", reason: "integrity_failure" } });
    // Every location gone: not found. A location that is not a blob address is never contacted.
    const none = mk(() => typed(new HomeError("not_found"), 404));
    expect(await none.perform({ ...call, locations: ["https://home.example/not-a-blob", "http://home.example/v1/blobs/" + toHex(readCap.slice(0, 16))] })).toEqual({ result: "err", error: { kind: "not_found" } });
  });

  it("delete uses the owner capability, treats absent as done, and only for an object at the person's own Home", async () => {
    const ownerCap = rand(48);
    const location = `https://home.example/v1/blobs/${toHex(ownerCap.slice(0, 16))}`;
    let auth = "";
    const p = mk((_u, init) => { auth = (init.headers as Record<string, string>)["authorization"] ?? ""; return new Response(null, { status: 200 }); });
    expect(await p.perform({ call: "delete_blob", blobId: toHex(rand(32)), ownerCap: toHex(ownerCap), location })).toEqual({ result: "ok" });
    expect(auth).toBe(`Bearer ${toBase64Url(ownerCap)}`);
    const absent = mk(() => typed(new HomeError("not_found"), 404));
    expect(await absent.perform({ call: "delete_blob", blobId: toHex(rand(32)), ownerCap: toHex(ownerCap), location })).toEqual({ result: "ok" });
    expect(await p.perform({ call: "delete_blob", blobId: toHex(rand(32)), ownerCap: toHex(ownerCap), location: location.replace("home.example", "other.example") })).toEqual({ result: "err", error: { kind: "rejected", reason: "unsupported" } });
  });

  it("a loopback Home reached over plain http (a test build only) is named with https, so Core's rule stays as it is", async () => {
    const { ct, call } = await sealed();
    const p = mk(() => putReply(call), { allowLoopbackHttp: true, baseUrl: "http://127.0.0.1:8788" });
    const r = await p.perform(call, { ciphertext: ct });
    if (r.result !== "bytes") throw new Error(JSON.stringify(r));
    expect((decode(fromHex(r.bytes)) as Map<number, CborValue>).get(1)).toEqual([`https://127.0.0.1:8788/v1/blobs/${call.ownerCap.slice(0, 32)}`]);
    // And read back as http when fetching.
    const seen: string[] = [];
    const g = mk((url) => { seen.push(url); return new Response(ct as BodyInit); }, { allowLoopbackHttp: true, baseUrl: "http://127.0.0.1:8788" });
    await g.perform({ call: "get_blob", locations: [`https://127.0.0.1:8788/v1/blobs/${call.ownerCap.slice(0, 32)}`], readCap: toHex(rand(48)), digest: call.digest, length: ct.length });
    expect(seen).toEqual([`http://127.0.0.1:8788/v1/blobs/${call.ownerCap.slice(0, 32)}`]);
  });

  it("reads the advertised limits from the capabilities document; a Home without the feature holds no files; an unreachable Home keeps the last answer (rejects)", async () => {
    const doc = (features: string[], limits: Record<string, number>): Response => new Response(JSON.stringify({ schema_version: 1, api_versions: ["1"], features, limits }));
    expect(await mk(() => doc(["om.blob.spool.v1"], { max_blob_bytes: 4194304, max_blob_spool_bytes: 33554432, max_blobs: 64 })).blobLimits?.()).toEqual({ maxBlobBytes: 4194304, maxSpoolBytes: 33554432, maxBlobs: 64 });
    expect(await mk(() => doc(["om.relay.queues.v1"], { max_blob_bytes: 4194304 })).blobLimits?.()).toBeNull();
    expect(await mk(() => doc(["om.blob.spool.v1"], {})).blobLimits?.()).toBeNull(); // advertised without a bound: fail closed
    await expect(mk(() => Promise.reject(new TypeError("down"))).blobLimits?.()).rejects.toThrow();
  });
});
