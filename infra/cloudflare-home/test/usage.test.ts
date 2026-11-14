// SPDX-License-Identifier: AGPL-3.0-or-later
// The personal limits profile (OM_PROFILE=personal) and its self-kept usage counter, against the real Worker bundle in workerd:
// PC-C03 (usage_state honesty), the healthy/soft/hard state machine, the admin gate of the read, and what a hard stop sheds.
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { USAGE_FEATURE, USAGE_PATH, encode, encodeKeyFetch, encodeKeyFetchGrant, encodeKeyPublish, map, parseUsageState, toHex } from "@om/home-wire";
import { startHome, type Home } from "./harness";
import { asBody, bearer, drainAll, errorOf, newCaps, nowS, rand, relay, sealed, type Caps } from "./kit";

/** The test's own accounting, independent of the Home's: one Worker request per call, and (Worker, meter + routed object) = 2 Durable Object calls for a /v1 call, 1 for anything else (a key fetch is charged separately, below). */
interface Tally { worker: number; durable: number }
function counted(h: Home, t: Tally): Home {
  return {
    ...h,
    fetch: (path, init) => { t.worker++; t.durable += path === "/v1/keys/fetch" ? 0 : path.startsWith("/v1/") ? 2 : 1; return h.fetch(path, init); },
  };
}

const read = (h: Home, cap: Uint8Array | undefined): Promise<Response> => h.fetch(USAGE_PATH, { headers: cap ? bearer(cap) : {} });

async function queue(h: Home): Promise<Caps> {
  const caps = newCaps();
  if (!(await relay.create(h, caps)).ok) throw new Error("create failed");
  return caps;
}

describe("the default profile is unchanged", () => {
  let home: Home;
  beforeAll(async () => { home = await startHome(); }, 120_000);
  afterAll(async () => { await home.dispose(); });

  it("does not advertise or serve the usage read, and keeps the multi-person limits", async () => {
    const doc = (await (await home.fetch("/.well-known/om-provider-capabilities")).json()) as { features: string[]; limits: Record<string, number>; cost_state: Record<string, unknown> };
    expect(doc.features).not.toContain(USAGE_FEATURE);
    expect(doc.limits["max_queue_items"]).toBe(512);
    expect(doc.cost_state).not.toHaveProperty("limits_profile");
    const c = await queue(home);
    expect((await read(home, c.admin)).status).toBe(404);
  });
});

describe("personal profile without the counter bound", () => {
  let home: Home;
  beforeAll(async () => { home = await startHome({ vars: { OM_PROFILE: "personal" }, usage: false }); }, 120_000);
  afterAll(async () => { await home.dispose(); });

  it("neither advertises nor serves the read, sheds nothing, and still applies the personal limits", async () => {
    const doc = (await (await home.fetch("/.well-known/om-provider-capabilities")).json()) as { features: string[]; limits: Record<string, number> };
    expect(doc.features).not.toContain(USAGE_FEATURE);
    expect(doc.limits["max_queue_items"]).toBe(128);
    const c = await queue(home);
    expect((await read(home, c.admin)).status).toBe(404);
    expect((await relay.deposit(home, c.deposit, sealed())).ok).toBe(true);
  });
});

describe("personal profile", () => {
  let home: Home;
  beforeAll(async () => { home = await startHome({ vars: { OM_PROFILE: "personal" } }); }, 120_000);
  afterAll(async () => { await home.dispose(); });

  it("advertises the personal limits and the usage read", async () => {
    const doc = (await (await home.fetch("/.well-known/om-provider-capabilities")).json()) as { features: string[]; limits: Record<string, number>; cost_state: Record<string, unknown> };
    expect(doc.features).toContain(USAGE_FEATURE);
    expect(doc.limits).toMatchObject({ max_queue_items: 128, max_queue_bytes: 8 * 1024 * 1024, max_deposit_capabilities: 16 });
    expect(doc.cost_state["limits_profile"]).toBe("personal");
  });

  it("enforces the smaller deposit-capability limit with a typed quota", async () => {
    const c = await queue(home);
    let minted = 0;
    let stop: string | undefined;
    while (stop === undefined && minted < 40) {
      const r = await relay.mint(home, c.admin);
      if (r.ok) minted++; else stop = errorOf(r).name;
    }
    expect(stop).toBe("quota");
    expect(minted).toBe(15); // 16 live deposit capabilities in all: the one made with the queue counts
  });

  it("the read needs the admin capability of a queue this Home holds, and is never cached", async () => {
    const c = await queue(home);
    const other = (name: string, cap: Uint8Array | undefined) => read(home, cap).then((r) => { expect(r.status, name).toBe(401); });
    await other("no credential", undefined);
    await other("receive capability", c.receive);
    await other("deposit capability", c.deposit);
    await other("forged", rand(48));
    await other("admin of a queue that does not exist", newCaps().admin);
    const ok = await read(home, c.admin);
    expect(ok.status).toBe(200);
    expect(ok.headers.get("cache-control")).toBe("no-store");
    expect((await home.fetch(USAGE_PATH, { method: "POST", headers: bearer(c.admin) })).status).toBe(400);
    const u = parseUsageState(await ok.json());
    expect(u).toMatchObject({ state: "healthy", healthy: true, billableOverage: "never", budgetRemaining: 0, profile: "personal" });
    expect(u.window).not.toBeNull();
    expect(JSON.stringify(u)).not.toContain(Buffer.from(c.admin).toString("base64"));
  });
});

describe("PC-C03 usage_state honesty (personal profile)", () => {
  let raw: Home;
  beforeAll(async () => { raw = await startHome({ vars: { OM_PROFILE: "personal" } }); }, 120_000);
  afterAll(async () => { await raw.dispose(); });

  it("reported usage matches the runner's own accounting within 1% (and, the Home counting itself, exactly)", async () => {
    const t: Tally = { worker: 0, durable: 0 };
    const home = counted(raw, t);
    const c = await queue(home);
    for (let i = 0; i < 20; i++) expect((await relay.deposit(home, c.deposit, sealed())).ok).toBe(true);
    const items = await drainAll(home, c.receive);
    expect(items).toHaveLength(20);
    expect((await relay.ack(home, c.receive, items.map((x) => x.id))).ok).toBe(true);
    for (let i = 0; i < 10; i++) await home.fetch("/healthz");
    const r = await read(home, c.admin); // this request counts too: the tally above includes it from the call
    expect(r.status).toBe(200);
    const u = parseUsageState(await r.json());
    const used = (n: string): number => u.metrics.find((m) => m.name === n)?.used ?? NaN;
    expect(Math.abs(used("worker_requests") - t.worker) / t.worker).toBeLessThanOrEqual(0.01);
    expect(Math.abs(used("durable_object_requests") - t.durable) / t.durable).toBeLessThanOrEqual(0.01);
    expect(used("worker_requests")).toBe(t.worker);
    expect(used("durable_object_requests")).toBe(t.durable);
  });
});

describe("healthy, soft and hard (small ceiling)", () => {
  let raw: Home;
  beforeAll(async () => { raw = await startHome({ vars: { OM_PROFILE: "personal", OM_USAGE_CEILING_WORKER: "40" } }); }, 120_000);
  afterAll(async () => { await raw.dispose(); });

  it("goes soft at 80% and hard at 95%; hard sheds deposits and new queues but still serves drains, acks and reads", async () => {
    const t: Tally = { worker: 0, durable: 0 };
    const home = counted(raw, t);
    const stateNow = async (cap: Uint8Array) => parseUsageState(await (await read(home, cap)).json());
    const c = await queue(home); // 1
    for (let i = 0; i < 2; i++) expect((await relay.deposit(home, c.deposit, sealed())).ok).toBe(true); // 3
    expect((await stateNow(c.admin)).state).toBe("healthy"); // 4
    while (t.worker < 31) await home.fetch("/healthz"); // 31 of 40 is below 80% (32)
    expect((await stateNow(c.admin)).state).toBe("soft"); // 32 = 80%
    expect((await relay.deposit(home, c.deposit, sealed())).ok).toBe(true); // 33, soft still serves everything
    while (t.worker < 37) await home.fetch("/healthz"); // 37 = 92.5%
    const hardDeposit = await relay.deposit(home, c.deposit, sealed()); // 38 = 95%: hard
    expect(hardDeposit.ok).toBe(false);
    const e = errorOf(hardDeposit);
    expect(e.name).toBe("quota");
    expect(Array.from(e.detail ?? [])).toEqual(Array.from(decodeDetail(3)));
    expect(hardDeposit.status).toBe(429);
    const shedQueue = await relay.create(home, newCaps()); // 39: a new queue is new work too
    expect(!shedQueue.ok && shedQueue.error.name).toBe("quota");
    // Mail already held is never trapped: drain, ack and the owner's read still work at hard.
    const items = await drainAll(home, c.receive); // 40 (one page)
    expect(items).toHaveLength(3);
    expect((await relay.ack(home, c.receive, items.map((x) => x.id))).ok).toBe(true); // 41
    expect((await home.fetch("/.well-known/om-provider-capabilities")).status).toBe(200); // 42
    const u = await stateNow(c.admin); // 43
    expect(u).toMatchObject({ state: "hard", healthy: false, billableOverage: "never", budgetRemaining: 0 });
    expect(u.metrics.find((m) => m.name === "worker_requests")).toEqual({ name: "worker_requests", used: t.worker, ceiling: 40 });
  });
});

/** The encoding of `quotaDetail(cls, true)`, spelled independently: map {0: cls, 1: true}. */
function decodeDetail(cls: number): Uint8Array {
  return new Uint8Array([0xa2, 0x00, cls, 0x01, 0xf5]);
}

describe("key fetch is metered by its real fan-out", () => {
  const cbor = { "content-type": "application/cbor" };
  let raw: Home;
  beforeAll(async () => { raw = await startHome({ vars: { OM_PROFILE: "personal" } }); }, 120_000);
  afterAll(async () => { await raw.dispose(); });

  /** A structurally valid signed-AsyncKeyBundle shell: the Home reads device id, generation and expiry, and verifies nothing. */
  const bundle = (device: Uint8Array): Uint8Array =>
    encode(map([[1, encode(map([[0, 9], [1, 1], [2, device], [5, 1], [6, 1], [12, rand(16)], [13, nowS() + 86400]]))], [2, [map([[0, device], [1, 1], [2, 3], [3, rand(64)]])]]]));

  /** Publishes `n` devices and grants one fetch capability on every one of them (setup, not counted by the assertions below). */
  async function devices(h: Home, n: number): Promise<{ ids: Uint8Array[]; cap: Uint8Array }> {
    const cap = rand(32);
    const ids: Uint8Array[] = [];
    for (let i = 0; i < n; i++) {
      const id = rand(16);
      const publish = rand(32);
      const put = await h.fetch(`/v1/keys/bundles/${toHex(id)}`, { method: "PUT", headers: { ...cbor, ...bearer(publish) }, body: asBody(encodeKeyPublish({ requestId: rand(16), bundle: bundle(id), opks: [], pqopks: [] })) });
      expect(put.status).toBe(204);
      const grant = await h.fetch(`/v1/keys/bundles/${toHex(id)}/fetch-caps`, { method: "PUT", headers: { ...cbor, ...bearer(publish) }, body: asBody(encodeKeyFetchGrant({ requestId: rand(16), caps: [cap] })) });
      expect(grant.status).toBe(204);
      ids.push(id);
    }
    return { ids, cap };
  }

  const used = async (h: Home, cap: Uint8Array): Promise<{ worker: number; durable: number }> => {
    const u = parseUsageState(await (await read(h, cap)).json());
    const n = (name: string): number => u.metrics.find((m) => m.name === name)?.used ?? NaN;
    return { worker: n("worker_requests"), durable: n("durable_object_requests") };
  };

  /** What one call costs the meter: the two usage reads around it differ by the call plus the second read's own cost (1 Worker request, 2 Durable Object calls). */
  async function cost(h: Home, adminCap: Uint8Array, call: () => Promise<Response>): Promise<{ worker: number; durable: number; response: Response }> {
    const before = await used(h, adminCap);
    const response = await call();
    await response.arrayBuffer();
    const after = await used(h, adminCap);
    return { worker: after.worker - before.worker - 1, durable: after.durable - before.durable - 2, response };
  }

  const fetchKeys = (h: Home, cap: Uint8Array, devs: Uint8Array[]) => (): Promise<Response> => h.fetch("/v1/keys/fetch", { method: "POST", headers: cbor, body: asBody(encodeKeyFetch({ requestId: rand(16), cap, devices: devs })) });

  it.each([1, 2, 64])("a fetch of %i device(s) is charged one Worker request and one Durable Object call per device plus two (the meter, the charge)", async (n) => {
    const q = await queue(raw);
    const { ids, cap } = await devices(raw, n);
    const c = await cost(raw, q.admin, fetchKeys(raw, cap, ids));
    expect(c.response.status).toBe(200);
    expect(c.worker).toBe(1);
    expect(c.durable).toBe(1 + n + 1);
  });

  it("charges the calls actually made when a later device fails (partial failure)", async () => {
    const q = await queue(raw);
    const { ids, cap } = await devices(raw, 2);
    const stranger = rand(16); // no bundle and no grant here: unauthorized after the two real calls before it
    const c = await cost(raw, q.admin, fetchKeys(raw, cap, [ids[0] as Uint8Array, ids[1] as Uint8Array, stranger, rand(16), rand(16)]));
    expect(c.response.status).toBe(401);
    expect(c.worker).toBe(1);
    expect(c.durable).toBe(1 + 3 + 1); // three devices called, the failing third included; the last two never called
  });

  it("charges only the first call when the capability is refused", async () => {
    const q = await queue(raw);
    const { ids } = await devices(raw, 3);
    const c = await cost(raw, q.admin, fetchKeys(raw, rand(32), ids));
    expect(c.response.status).toBe(401);
    expect(c.durable).toBe(1 + 1 + 1);
  });

  it("charges the calls made before a malformed device id is reached, and nothing for a request that reaches no device", async () => {
    const q = await queue(raw);
    const { ids, cap } = await devices(raw, 1);
    const mixed = await cost(raw, q.admin, fetchKeys(raw, cap, [ids[0] as Uint8Array, rand(17)]));
    expect(mixed.response.status).toBe(400);
    expect(mixed.durable).toBe(1 + 1 + 1);
    const garbage = await cost(raw, q.admin, () => raw.fetch("/v1/keys/fetch", { method: "POST", headers: cbor, body: asBody(rand(12)) }));
    expect(garbage.response.status).toBe(400);
    expect(garbage.worker).toBe(1);
    expect(garbage.durable).toBe(1); // the meter's own call only: no device was called
  });
});

describe("key fetch at the hard stop and without the counter", () => {
  const cbor = { "content-type": "application/cbor" };
  const bundleOf = (device: Uint8Array): Uint8Array =>
    encode(map([[1, encode(map([[0, 9], [1, 1], [2, device], [5, 1], [6, 1], [12, rand(16)], [13, nowS() + 86400]]))], [2, [map([[0, device], [1, 1], [2, 3], [3, rand(64)]])]]]));
  async function oneDevice(h: Home): Promise<{ id: Uint8Array; cap: Uint8Array }> {
    const [id, publish, cap] = [rand(16), rand(32), rand(32)];
    expect((await h.fetch(`/v1/keys/bundles/${toHex(id)}`, { method: "PUT", headers: { ...cbor, ...bearer(publish) }, body: asBody(encodeKeyPublish({ requestId: rand(16), bundle: bundleOf(id), opks: [], pqopks: [] })) })).status).toBe(204);
    expect((await h.fetch(`/v1/keys/bundles/${toHex(id)}/fetch-caps`, { method: "PUT", headers: { ...cbor, ...bearer(publish) }, body: asBody(encodeKeyFetchGrant({ requestId: rand(16), caps: [cap] })) })).status).toBe(204);
    return { id, cap };
  }
  const fetchKeys = (h: Home, cap: Uint8Array, devs: Uint8Array[]): Promise<Response> => h.fetch("/v1/keys/fetch", { method: "POST", headers: cbor, body: asBody(encodeKeyFetch({ requestId: rand(16), cap, devices: devs })) });

  it("at hard a key fetch is still served (it adds no work to hold) and is still charged for its fan-out", async () => {
    const raw = await startHome({ vars: { OM_PROFILE: "personal", OM_USAGE_CEILING_DO: "60" } });
    try {
      const q = await queue(raw);
      const d = await oneDevice(raw);
      let state = "healthy";
      while (state !== "hard") state = parseUsageState(await (await read(raw, q.admin)).json()).state; // every read is itself counted
      expect((await relay.deposit(raw, q.deposit, sealed())).ok).toBe(false); // hard: new work is shed
      const before = parseUsageState(await (await read(raw, q.admin)).json()).metrics.find((m) => m.name === "durable_object_requests")?.used ?? NaN;
      const r = await fetchKeys(raw, d.cap, [d.id, d.id]);
      expect(r.status).toBe(200); // the same device twice is two real calls
      const after = parseUsageState(await (await read(raw, q.admin)).json()).metrics.find((m) => m.name === "durable_object_requests")?.used ?? NaN;
      expect(after - before - 2).toBe(1 + 2 + 1);
    } finally { await raw.dispose(); }
  });

  it("without the counter nothing is metered or shed, and a fetch is served", async () => {
    const raw = await startHome({ vars: { OM_PROFILE: "personal" }, usage: false });
    try {
      const d = await oneDevice(raw);
      expect((await fetchKeys(raw, d.cap, [d.id])).status).toBe(200);
    } finally { await raw.dispose(); }
  });
});
