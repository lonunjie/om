// SPDX-License-Identifier: AGPL-3.0-or-later
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { admissionHash, decodeHomeError, decodeTurnLease, encodeTurnRequest, fromHex, toBase64Url } from "@om/home-wire";
import { scratch, startHome, type Home } from "./harness";
import { asBody, rand } from "./kit";

const path = "/v1/media/turn";
const capabilities = "/.well-known/om-provider-capabilities";
const owner = rand(32);
const vars = async () => ({ OM_ADMISSION: await admissionHash(owner), OM_PROFILE: "personal" });
const issue = (home: Home, id: string, auth = true) => home.fetch(path, {
  method: "POST",
  headers: {
    "content-type": "application/cbor",
    ...(auth ? { "om-admission": toBase64Url(owner) } : {}),
  },
  body: asBody(encodeTurnRequest(fromHex(id))),
});

describe("optional media leases", () => {
  let home: Home;
  let disabled: Home;
  let exhausted: Home;
  const issued = new Map<string, { username: string; credential: string; expires_at: number }>();
  const backend = async (request: Request): Promise<Response> => {
    expect(request.headers.has("om-admission")).toBe(false);
    const { request_id: id } = await request.json() as { request_id: string };
    if (id === "ff".repeat(16)) return new Response(null, { status: 410 });
    if (id === "ee".repeat(16)) return new Response(null, { status: 302, headers: { location: "https://elsewhere.example/lease" } });
    let grant = issued.get(id);
    if (!grant) {
      if (issued.size === 2) return new Response(null, { status: 429 });
      grant = { username: "test-" + String(issued.size), credential: toBase64Url(rand(32)), expires_at: Math.floor(Date.now() / 1000) + 60 };
      issued.set(id, grant);
    }
    return Response.json({ ...grant, urls: ["turn:turn.example:3478"] });
  };

  beforeAll(async () => {
    const config = await vars();
    home = await startHome({ vars: { ...config, OM_MEDIA_STUN: "stun:turn.example:3478" }, media: backend });
    disabled = await startHome({ vars: config });
    exhausted = await startHome({ vars: { ...config, OM_USAGE_CEILING_WORKER: "1" }, media: () => Promise.reject(new Error("meter must refuse before issuance")) });
  }, 120_000);
  afterAll(async () => { await Promise.all([home, disabled, exhausted].filter(Boolean).map((instance) => instance.dispose())); });

  it("is explicitly optional and never serves an unadvertised media lease", async () => {
    const off = await (await disabled.fetch(capabilities)).json() as { features: string[] };
    expect(off.features).not.toContain("om.media.turn.v1");
    expect((await issue(disabled, "11".repeat(16))).status).toBe(404);
    const on = await (await home.fetch(capabilities)).json() as { features: string[]; media: unknown };
    expect(on.features).toContain("om.media.turn.v1");
    expect(on.media).toEqual({ stun_urls: ["stun:turn.example:3478"] });
    expect((await issue(home, "11".repeat(16), false)).status).toBe(401);
    expect(issued.size).toBe(0);
  });

  it("returns private, expiring, separate leases with exact retries and a hard ceiling", async () => {
    const id = "11".repeat(16);
    const a = await issue(home, id);
    expect(a.status).toBe(200);
    expect(a.headers.get("cache-control")).toBe("no-store");
    const now = Math.floor(Date.now() / 1000);
    const read = async (reply: Response) => decodeTurnLease(new Uint8Array(await reply.arrayBuffer()), now);
    const first = await read(a);
    expect(first?.expires_at).toBeGreaterThan(now);
    expect(await read(await issue(home, id))).toEqual(first);
    const other = await read(await issue(home, "22".repeat(16)));
    expect(other?.username).not.toBe(first?.username);
    expect(decodeHomeError(new Uint8Array(await (await issue(home, "33".repeat(16))).arrayBuffer())).name).toBe("quota");
    expect(decodeHomeError(new Uint8Array(await (await issue(exhausted, "44".repeat(16))).arrayBuffer())).name).toBe("quota");
    expect((await issue(home, "ff".repeat(16))).status).toBe(410);
    expect((await issue(home, "ee".repeat(16))).status).toBe(503);
    expect(issued.size).toBe(2);
  });
});

// ADR-0228 s4: an exact retry finds its reservation without consulting the budget gate; only a new attempt is gated.
describe("exact lease retries and the hard meter", () => {
  const reservations = new Map<string, { username: string; credential: string; expires_at: number }>();
  const calls: string[] = [];
  const backend = async (request: Request): Promise<Response> => {
    const { request_id: id } = await request.json() as { request_id: string };
    calls.push(id);
    if (id === "ff".repeat(16)) return new Response(null, { status: 410 });
    let grant = reservations.get(id);
    if (!grant) {
      grant = { username: "u-" + String(reservations.size), credential: toBase64Url(rand(32)), expires_at: Math.floor(Date.now() / 1000) + 60 };
      reservations.set(id, grant);
    }
    return Response.json({ ...grant, urls: ["turn:turn.example:3478"] });
  };
  const hex16 = (n: number): string => n.toString(16).padStart(32, "0");
  const lease = async (reply: Response) => decodeTurnLease(new Uint8Array(await reply.arrayBuffer()), Math.floor(Date.now() / 1000));
  const errorOf = async (reply: Response) => decodeHomeError(new Uint8Array(await reply.arrayBuffer())).name;
  /** Counted requests until the Home's own worker budget is hard (ceiling 40, hard from 38). */
  const exhaust = async (home: Home, n = 45) => { for (let i = 0; i < n; i++) await home.fetch("/healthz"); };
  const withCeiling = async (ceiling: string) => ({ ...(await vars()), OM_MEDIA_STUN: "stun:turn.example:3478", OM_USAGE_CEILING_WORKER: ceiling });

  it("returns the existing lease to the exact retry after the meter turned hard, and refuses a new attempt", async () => {
    reservations.clear(); calls.length = 0;
    const home = await startHome({ vars: await withCeiling("40"), media: backend });
    try {
      const first = await lease(await issue(home, hex16(1)));
      expect(first?.username).toBe("u-0");
      await exhaust(home);
      expect(await errorOf(await issue(home, hex16(2)))).toBe("quota");
      expect(reservations.size).toBe(1); // the refusal never reached the backend
      const again = await issue(home, hex16(1));
      expect(again.status).toBe(200);
      expect(await lease(again)).toEqual(first);
      expect(reservations.size).toBe(1); // one reservation for the attempt, however often it is retried
    } finally { await home.dispose(); }
  }, 120_000);

  it("keeps that lookup across a restart, and an expired id never creates a reservation", async () => {
    reservations.clear(); calls.length = 0;
    const state = scratch();
    try {
      const config = await withCeiling("40");
      let home = await startHome({ vars: config, persist: state.dir, media: backend });
      const first = await lease(await issue(home, hex16(1)));
      expect((await issue(home, "ff".repeat(16))).status).toBe(410);
      await exhaust(home);
      await home.dispose();
      home = await startHome({ vars: config, persist: state.dir, media: backend });
      try {
        expect(await lease(await issue(home, hex16(1)))).toEqual(first);
        expect(await errorOf(await issue(home, hex16(3)))).toBe("quota");
        expect(await errorOf(await issue(home, "ff".repeat(16)))).toBe("expired");
        expect(reservations.size).toBe(1);
      } finally { await home.dispose(); }
    } finally { state.remove(); }
  }, 120_000);

  it("remembers at most 64 recent attempts, oldest first out", async () => {
    reservations.clear(); calls.length = 0;
    const home = await startHome({ vars: await withCeiling("400"), media: backend });
    try {
      for (let n = 1; n <= 70; n++) expect((await issue(home, hex16(n))).status).toBe(200);
      await exhaust(home, 400);
      expect(await errorOf(await issue(home, hex16(1)))).toBe("quota"); // left the table: gated like a new attempt
      expect((await issue(home, hex16(70))).status).toBe(200); // still in the table
      expect((await issue(home, hex16(7))).status).toBe(200);
      expect(reservations.size).toBe(70);
    } finally { await home.dispose(); }
  }, 180_000);
});

// The adapter contract (infra/MEDIA_TURN_ADAPTER_CONTRACT.md): every way a backend can answer maps to one client-visible outcome,
// identically on the Cloudflare Home (a service binding) and on the SQLite Home (a Unix socket). These cases run on every
// target of `pnpm conformance:provider`, so a difference between the two adapters fails here.
describe("media backend adapter contract", () => {
  const now = () => Math.floor(Date.now() / 1000);
  const good = (extra: Record<string, unknown> = {}) => ({ urls: ["turn:turn.example:3478?transport=udp"], username: "u", credential: "c", expires_at: now() + 60, ...extra });
  const hex16 = (n: number): string => n.toString(16).padStart(32, "0");
  const errorOf = async (reply: Response) => decodeHomeError(new Uint8Array(await reply.arrayBuffer())).name;
  const seen: { method: string; type: string | null; body: string }[] = [];
  /** Each case is one request id; the backend answers by id so one Home serves the whole table. */
  const answers = new Map<string, (signal: AbortSignal) => Response | Promise<Response>>();
  const backend = async (request: Request): Promise<Response> => {
    const text = await request.text();
    seen.push({ method: request.method, type: request.headers.get("content-type"), body: text });
    const id = (JSON.parse(text) as { request_id: string }).request_id;
    return (answers.get(id) ?? (() => new Response(null, { status: 500 })))(request.signal);
  };
  let home: Home;
  beforeAll(async () => { home = await startHome({ vars: { ...(await vars()), OM_MEDIA_STUN: "stun:turn.example:3478" }, media: backend }); }, 120_000);
  afterAll(async () => { await home.dispose(); });

  it("passes the backend nothing but the request id, as JSON, and never the owner's admission", async () => {
    answers.set(hex16(1), () => Response.json(good()));
    seen.length = 0;
    expect((await issue(home, hex16(1))).status).toBe(200);
    expect(seen).toHaveLength(1);
    expect(seen[0]?.method).toBe("POST");
    expect(seen[0]?.type).toMatch(/^application\/json/);
    expect(seen[0]?.body).toBe(JSON.stringify({ request_id: hex16(1) }));
  });

  const malformed: [string, () => Response][] = [
    ["a lease valid for longer than 600 seconds", () => Response.json(good({ expires_at: now() + 601 }))],
    ["a lease that has already expired", () => Response.json(good({ expires_at: now() - 1 }))],
    ["no server URL", () => Response.json(good({ urls: [] }))],
    ["a URL that is not a TURN URL", () => Response.json(good({ urls: ["https://turn.example/"] }))],
    ["more than eight URLs", () => Response.json(good({ urls: Array.from({ length: 9 }, () => "turn:turn.example:3478") }))],
    ["a credential with a space in it", () => Response.json(good({ credential: "a b" }))],
    ["a missing username", () => Response.json({ urls: ["turn:turn.example:3478"], credential: "c", expires_at: now() + 60 })],
    ["a reply that is not JSON", () => new Response("not json", { status: 200 })],
    ["an empty reply", () => new Response(null, { status: 200 })],
    ["a reply larger than 4096 bytes", () => Response.json(good({ credential: "x".repeat(200), pad: "y".repeat(4200) }))],
    ["a server error", () => new Response(null, { status: 500 })],
    ["a redirect", () => new Response(null, { status: 307, headers: { location: "https://elsewhere.example/lease" } })],
    ["an unexpected refusal code", () => new Response(null, { status: 403 })],
  ];
  it.each(malformed)("answers `retryable`, with no lasting refusal, for %s", async (_label, answer) => {
    const id = hex16(100 + malformed.findIndex(([label]) => label === _label));
    answers.set(id, answer);
    const first = await issue(home, id);
    expect(first.status).toBe(503);
    expect(await errorOf(first)).toBe("retryable");
    // The failure is not remembered as a verdict: the exact retry reaches the backend again and can be served.
    answers.set(id, () => Response.json(good()));
    expect((await issue(home, id)).status).toBe(200);
  }, 60_000);

  it("maps a budget refusal to `quota` and an expired reservation to `expired`, remembering each", async () => {
    answers.set(hex16(2), () => new Response(null, { status: 429 }));
    answers.set(hex16(3), () => new Response(null, { status: 410 }));
    expect(await errorOf(await issue(home, hex16(2)))).toBe("quota");
    expect(await errorOf(await issue(home, hex16(3)))).toBe("expired");
    // The determined outcome is answered from the Home's own table: the backend is not asked again, even if it would now say yes.
    answers.set(hex16(2), () => Response.json(good()));
    answers.set(hex16(3), () => Response.json(good()));
    seen.length = 0;
    expect(await errorOf(await issue(home, hex16(2)))).toBe("quota");
    expect(await errorOf(await issue(home, hex16(3)))).toBe("expired");
    expect(seen).toHaveLength(0);
  });

  it("gives up on a backend that does not answer within five seconds", async () => {
    // A backend honours the cancellation it is given, as the socket adapter's does; the Home must give up after five seconds either way.
    answers.set(hex16(4), (signal) => new Promise<Response>((resolve, reject) => {
      const timer = setTimeout(() => { resolve(Response.json(good())); }, 8000);
      signal.addEventListener("abort", () => { clearTimeout(timer); reject(new Error("cancelled")); }, { once: true });
    }));
    const started = Date.now();
    expect(await errorOf(await issue(home, hex16(4)))).toBe("retryable");
    expect(Date.now() - started).toBeLessThan(7500);
  }, 30_000);

  it("rejects a request body that is not exactly one 16-byte id, before the backend is asked", async () => {
    seen.length = 0;
    const post = (body: Uint8Array) => home.fetch(path, { method: "POST", headers: { "content-type": "application/cbor", "om-admission": toBase64Url(owner) }, body: asBody(body) });
    expect((await post(new Uint8Array([0xa0]))).status).toBe(400);
    expect((await post(encodeTurnRequest(new Uint8Array(15)))).status).toBe(400);
    expect((await home.fetch(path, { method: "GET", headers: { "om-admission": toBase64Url(owner) } })).status).toBe(400);
    expect(seen).toHaveLength(0);
  });
});
