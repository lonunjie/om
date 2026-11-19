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
