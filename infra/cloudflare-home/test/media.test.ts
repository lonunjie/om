// SPDX-License-Identifier: AGPL-3.0-or-later
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { admissionHash, decodeHomeError, decodeTurnLease, encodeTurnRequest, fromHex, toBase64Url } from "@om/home-wire";
import { startHome, type Home } from "./harness";
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
