// SPDX-License-Identifier: MPL-2.0
// Client side of the optional push wake (om.relay.push.v1, ADR-0210): what is sent to the Home, to which mailboxes, and how every refusal reads.
import { describe, expect, it } from "vitest";
import { ADMISSION_HEADER, FEATURES, HomeError, PUSH_PATHS, encodeHomeError, toBase64Url } from "@om/home-wire";
import { createPushClient } from "./push";
import { createHttpProvider } from "./provider";
import { createMemorySecretStore } from "./secrets";
import { parseProviderUrl } from "./config";

const ENDPOINT = "https://web.push.apple.com/abc";
const admission = new Uint8Array(32).fill(7);
const caps = [new Uint8Array(32).fill(1), new Uint8Array(32).fill(2)];

interface Rec { method: string; path: string; headers: Headers; body: string }
function client(answer: (r: Rec) => Response, opts: { features?: string[]; admission?: Uint8Array | undefined } = {}) {
  const seen: Rec[] = [];
  const c = createPushClient({
    base: "https://home.example", receiveCaps: () => Promise.resolve(caps),
    http: { timeoutMs: 1000, ...(opts.admission === undefined && "admission" in opts ? {} : { admission }), fetch: async (i, init) => { const req = new Request(i, init); const r = { method: req.method, path: new URL(req.url).pathname, headers: req.headers, body: await req.text() }; seen.push(r); return answer(r); } },
    features: () => Promise.resolve(opts.features ?? [FEATURES.relayPush]),
  });
  return { c, seen };
}
const typed = (name: ConstructorParameters<typeof HomeError>[0], status: number) => new Response(encodeHomeError(new HomeError(name)) as BodyInit, { status, headers: { "content-type": "application/cbor" } });
const keyResponse = (r: Rec) => r.path === PUSH_PATHS.key ? Response.json({ key: toBase64Url(new Uint8Array(65).fill(4)) }) : Response.json({ id: "x".repeat(32), expires_at: 1 });

describe("push client", () => {
  it("supported only when the Home advertises the feature", async () => {
    expect(await client(keyResponse).c.supported()).toBe(true);
    expect(await client(keyResponse, { features: [] }).c.supported()).toBe(false);
  });
  it("reads the Home's public key as 65 bytes and nothing else", async () => {
    expect(await client(keyResponse).c.publicKey()).toEqual(new Uint8Array(65).fill(4));
    expect(await client(() => Response.json({ key: "AAAA" })).c.publicKey()).toBeUndefined();
    expect(await client(() => new Response(null, { status: 404 })).c.publicKey()).toBeUndefined();
  });
  it("registers one endpoint for every mailbox with the receive capability as bearer and the admission secret, nothing in the URL", async () => {
    const { c, seen } = client(keyResponse);
    expect(await c.register(ENDPOINT)).toEqual({ registered: 2, failed: 0, reason: undefined });
    expect(seen).toHaveLength(2);
    for (const [i, r] of seen.entries()) {
      expect(r.method).toBe("PUT");
      expect(r.path).toBe(PUSH_PATHS.subscription);
      expect(r.headers.get("authorization")).toBe(`Bearer ${toBase64Url(caps[i] as Uint8Array)}`);
      expect(r.headers.get(ADMISSION_HEADER)).toBe(toBase64Url(admission));
      expect(r.headers.get("content-type")).toBe("application/json");
      expect(JSON.parse(r.body)).toEqual({ endpoint: ENDPOINT });
    }
  });
  it("says why when nothing could be registered: the Home refused this client's admission, or the endpoint", async () => {
    const denied = client(() => typed("unauthorized", 401));
    expect((await denied.c.register(ENDPOINT)).reason).toBe("refused");
    const full = client(() => typed("quota", 429));
    expect((await full.c.register(ENDPOINT)).reason).toBe("full");
    const down = client(() => { throw new Error("offline"); });
    expect(await down.c.register(ENDPOINT)).toEqual({ registered: 0, failed: 2, reason: "unreachable" });
  });
  it("a Home without the feature is not called at all", async () => {
    const { c, seen } = client(keyResponse, { features: [] });
    expect((await c.register(ENDPOINT)).reason).toBe("unsupported");
    await c.unregister(ENDPOINT);
    expect(seen).toHaveLength(0);
  });
  it("unregisters with the capability only, never with the admission secret", async () => {
    const { c, seen } = client(() => new Response(null, { status: 204 }));
    await c.unregister(ENDPOINT);
    expect(seen.map((r) => r.method)).toEqual(["DELETE", "DELETE"]);
    for (const r of seen) { expect(r.headers.get(ADMISSION_HEADER)).toBeNull(); expect(r.headers.get("authorization")).toMatch(/^Bearer /); }
  });
});

describe("provider.push", () => {
  const parsed = parseProviderUrl("http://127.0.0.1:1", { allowLoopbackHttp: true });
  if (!parsed) throw new Error("url");
  it("registers the mailboxes it has opened, and each one opened afterwards", async () => {
    const puts: string[] = [];
    const p = createHttpProvider({
      config: parsed, secrets: createMemorySecretStore(), allowLoopbackHttp: true, webSocket: false, admission,
      fetch: (i, init) => {
        const req = new Request(i, init); const path = new URL(req.url).pathname;
        if (path === "/.well-known/om-provider-capabilities") return Promise.resolve(Response.json({ schema_version: 1, api_versions: ["1"], features: [FEATURES.relayPush] }));
        if (path === "/v1/relay/queues") return Promise.resolve(new Response(null, { status: 204 }));
        if (path === PUSH_PATHS.subscription) { puts.push(req.method); return Promise.resolve(Response.json({ id: "x", expires_at: 1 })); }
        return Promise.resolve(new Response(null, { status: 404 }));
      },
    });
    expect(await p.push.supported()).toBe(true);
    expect(await p.push.register(ENDPOINT)).toMatchObject({ registered: 0, failed: 0 }); // nothing opened yet
    const open = (m: string) => p.perform({ call: "open_queue", mailbox: m, relayUri: "http://127.0.0.1:1/" } as never);
    await open("00".repeat(16));
    await new Promise((r) => setTimeout(r, 20));
    expect(puts).toEqual(["PUT"]); // the kept endpoint followed the new mailbox
    let woke = 0;
    p.onWake(() => { woke++; });
    p.nudge();
    expect(woke).toBe(1);
    await p.push.unregister(ENDPOINT);
    expect(puts).toEqual(["PUT", "DELETE"]);
    await open("11".repeat(16));
    await new Promise((r) => setTimeout(r, 20));
    expect(puts).toEqual(["PUT", "DELETE"]); // forgotten: not registered any more
  });
});
