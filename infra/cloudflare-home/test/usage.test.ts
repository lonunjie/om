// SPDX-License-Identifier: AGPL-3.0-or-later
// The personal limits profile (OM_PROFILE=personal) and its self-kept usage counter, against the real Worker bundle in workerd:
// PC-C03 (usage_state honesty), the healthy/soft/hard state machine, the admin gate of the read, and what a hard stop sheds.
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { USAGE_FEATURE, USAGE_PATH, parseUsageState } from "@om/home-wire";
import { startHome, type Home } from "./harness";
import { bearer, drainAll, errorOf, newCaps, rand, relay, sealed, type Caps } from "./kit";

/** The test's own accounting, independent of the Home's: one Worker request per call, and (Worker, meter + routed object) = 2 Durable Object calls for a /v1 call, 1 for anything else. */
interface Tally { worker: number; durable: number }
function counted(h: Home, t: Tally): Home {
  return {
    ...h,
    fetch: (path, init) => { t.worker++; t.durable += path.startsWith("/v1/") ? 2 : 1; return h.fetch(path, init); },
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
