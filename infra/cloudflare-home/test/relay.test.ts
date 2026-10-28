// SPDX-License-Identifier: MPL-2.0
// Provider conformance cases (docs/architecture/providers/PROVIDER_CONFORMANCE_SUITE.md) for the relay role, run against the
// real Worker bundle in workerd. Case ids are those of the suite.
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { equalBytes } from "@om/home-wire";
import { startHome, scratch, type Home } from "./harness";
import { decode } from "@om/home-wire";
import { drainAll, errorOf, newCaps, nowS, rand, relay, sealed, messageRequest } from "./kit";

let home: Home;
beforeAll(async () => { home = await startHome(); }, 120_000);
afterAll(async () => { await home.dispose(); });

async function queue(h: Home = home) {
  const caps = newCaps();
  const r = await relay.create(h, caps);
  if (!r.ok) throw new Error("create failed");
  return caps;
}

describe("PC-A capability semantics", () => {
  it("PC-A01: deposit is retrievable only with the matching receive capability", async () => {
    const c = await queue();
    const env = sealed();
    const d = await relay.deposit(home, c.deposit, env);
    expect(d.ok).toBe(true);
    const viaDeposit = await relay.drain(home, c.deposit);
    expect(viaDeposit.ok).toBe(false);
    expect(errorOf(viaDeposit).name).toBe("unauthorized");
    const got = await drainAll(home, c.receive);
    expect(got).toHaveLength(1);
    expect(equalBytes(got[0]?.bytes ?? new Uint8Array(0), env)).toBe(true);
    // the receive capability cannot deposit
    const viaReceive = await relay.deposit(home, c.receive, sealed());
    expect(viaReceive.ok).toBe(false);
  });

  it("PC-A02: forged, truncated and other-queue capabilities answer exactly like a full queue (no oracle)", async () => {
    const a = await queue();
    const b = await queue();
    const answers = new Map<string, number>();
    for (const cap of [rand(48), a.deposit.slice(0, 20), b.deposit.slice(0, 16), new Uint8Array(48), (() => { const x = a.deposit.slice(); x[40] = (x[40] ?? 0) ^ 1; return x; })()]) {
      const r = await relay.deposit(home, cap, sealed());
      expect(r.ok).toBe(false);
      if (!r.ok) {
        const key = `${r.error.name}:${String(r.status)}:${Array.from(r.error.detail ?? []).join(",")}`;
        answers.set(key, (answers.get(key) ?? 0) + 1);
      }
    }
    expect(answers.size).toBe(1);
    expect([...answers.keys()][0]).toMatch(/^quota:429:/);
  });

  it("PC-A03: rotation issues a new pair; the old pair works through the overlap; rotating with a stale capability is refused", async () => {
    const c = await queue();
    expect((await relay.deposit(home, c.deposit, sealed())).ok).toBe(true);
    const r = await relay.rotate(home, c.receive);
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(equalBytes(r.value.receiveCap.slice(0, 16), c.prefix)).toBe(true); // same queue, new secrets
    expect(equalBytes(r.value.receiveCap, c.receive)).toBe(false);
    expect((await relay.deposit(home, r.value.depositCap, sealed())).ok).toBe(true);
    expect((await relay.deposit(home, c.deposit, sealed())).ok).toBe(true); // overlap
    expect(await drainAll(home, r.value.receiveCap)).toHaveLength(3);
    expect(await drainAll(home, c.receive)).toHaveLength(3); // old receive capability still drains during the overlap
    const again = await relay.rotate(home, c.receive);
    expect(!again.ok && again.error.name).toBe("stale_generation");
    expect((await relay.rotate(home, r.value.receiveCap)).ok).toBe(true);
  });

  it("PC-A04: ack removes items; an exact retry is idempotent; an unknown id is not_found", async () => {
    const c = await queue();
    const items = [] as Uint8Array[];
    for (let i = 0; i < 3; i++) {
      const d = await relay.deposit(home, c.deposit, sealed());
      if (d.ok) items.push(d.value.itemId);
    }
    const rid = rand(16);
    expect((await relay.ack(home, c.receive, items.slice(0, 2), rid)).ok).toBe(true);
    expect((await relay.ack(home, c.receive, items.slice(0, 2), rid)).ok).toBe(true); // lost-response retry
    expect((await relay.ack(home, c.receive, items.slice(0, 2))).ok).toBe(true); // acknowledged ids stay known
    const left = await drainAll(home, c.receive);
    expect(left).toHaveLength(1);
    expect(equalBytes(left[0]?.id ?? new Uint8Array(0), items[2] ?? new Uint8Array(1))).toBe(true);
    const unknown = await relay.ack(home, c.receive, [rand(16)]);
    expect(!unknown.ok && unknown.error.name).toBe("not_found");
    const wrong = await relay.ack(home, c.deposit, items.slice(2));
    expect(!wrong.ok && wrong.error.name).toBe("unauthorized");
  });

  it("PC-A05: a capability is accepted only by the Home that issued it", async () => {
    const other = await startHome();
    try {
      const c = await queue();
      expect((await relay.deposit(home, c.deposit, sealed())).ok).toBe(true);
      const r = await relay.deposit(other, c.deposit, sealed());
      expect(r.ok).toBe(false);
    } finally { await other.dispose(); }
  }, 120_000);
});

describe("relay semantics", () => {
  it("accepts only the two protocol objects a mailbox can receive", async () => {
    const c = await queue();
    const r1 = await relay.deposit(home, c.deposit, rand(2000));
    expect(!r1.ok && r1.error.name).toBe("invalid_request");
    expect((await relay.deposit(home, c.deposit, messageRequest(rand(16)))).ok).toBe(true);
    expect((await relay.deposit(home, c.deposit, sealed())).ok).toBe(true);
    const clearSender = await relay.deposit(home, c.deposit, sealed({ ct: 1000 })); // not a profile-1 bucket length
    expect(!clearSender.ok && clearSender.error.name).toBe("invalid_request");
  });

  it("replay: an exact wrapper is stored once; a different wrapper under a used delivery tuple is rejected", async () => {
    const c = await queue();
    const keyId = rand(16);
    const deliveryId = rand(16);
    const first = sealed({ keyId, deliveryId });
    const a = await relay.deposit(home, c.deposit, first);
    const b = await relay.deposit(home, c.deposit, first); // new request_id, same bytes: exact transport retry
    expect(a.ok && b.ok && equalBytes(a.value.itemId, b.value.itemId)).toBe(true);
    const collide = await relay.deposit(home, c.deposit, sealed({ keyId, deliveryId }));
    expect(!collide.ok && collide.error.name).toBe("invalid_request");
    expect(await drainAll(home, c.receive)).toHaveLength(1);
    const rid = rand(16);
    expect((await relay.deposit(home, c.deposit, sealed(), rid)).ok).toBe(true);
    const reuse = await relay.deposit(home, c.deposit, sealed(), rid);
    expect(!reuse.ok && reuse.error.name).toBe("invalid_request"); // same request_id, different bytes
  });

  it("expiry: a past wrapper is expired, one beyond the retention bound is refused, a short one is kept", async () => {
    const c = await queue();
    const past = await relay.deposit(home, c.deposit, sealed({ expiresAt: nowS() - 3600 }));
    expect(!past.ok && past.error.name).toBe("expired");
    const far = await relay.deposit(home, c.deposit, sealed({ expiresAt: nowS() + 400 * 86400 }));
    expect(!far.ok && far.error.name).toBe("invalid_request");
    expect((await relay.deposit(home, c.deposit, sealed({ expiresAt: nowS() + 60 }))).ok).toBe(true);
  });

  it("PC-C01: size, rate and item quotas are typed", async () => {
    const c = await queue();
    const big = await relay.deposit(home, c.deposit, new Uint8Array(200_000));
    expect(!big.ok && big.error.name).toBe("payload_too_large");
    // per-capability rate: a burst of 30 then rate_limited with a retry hint
    let limited;
    for (let i = 0; i < 40 && !limited; i++) {
      const r = await relay.deposit(home, c.deposit, sealed());
      if (!r.ok) limited = r.error;
    }
    expect(limited?.name).toBe("rate_limited");
    expect(limited?.retryable).toBe(true);
    expect(limited?.retryAfterSeconds ?? 0).toBeGreaterThanOrEqual(1);
  });

  it("PC-C01: the queue item quota answers quota, flagged hard on a free plan", async () => {
    const c = await queue();
    let stored = 0;
    let quota;
    for (let k = 0; k < 40 && !quota; k++) {
      const m = await relay.mint(home, c.admin);
      if (!m.ok) throw new Error("mint failed");
      for (let i = 0; i < 30; i++) {
        const r = await relay.deposit(home, m.value.cap, sealed());
        if (r.ok) stored += 1; else { quota = r.error; break; }
      }
    }
    expect(stored).toBe(512);
    expect(quota?.name).toBe("quota");
    expect(quota?.retryable).toBe(false);
    const detail = decode(quota?.detail ?? new Uint8Array(0)) as ReadonlyMap<number, unknown>;
    expect(detail.get(0)).toBe(1); // class: items
    expect(detail.get(1)).toBe(true); // hard stop: the test Home declares the free plan
  }, 120_000);

  it("pagination: pages in order, no gaps or duplicates, continuation cursor only while more remain", async () => {
    const c = await queue();
    const sent: string[] = [];
    for (let k = 0; k < 4; k++) {
      const m = await relay.mint(home, c.admin);
      if (!m.ok) throw new Error("mint failed");
      for (let i = 0; i < 30; i++) {
        const e = sealed();
        const r = await relay.deposit(home, m.value.cap, e);
        if (!r.ok) throw new Error("deposit failed");
        sent.push(Buffer.from(e).toString("hex"));
      }
    }
    const first = await relay.drain(home, c.receive);
    expect(first.ok && first.value.items.length).toBe(64);
    expect(first.ok && first.value.next).toBeDefined();
    const all = await drainAll(home, c.receive);
    expect(all).toHaveLength(120);
    expect(all.map((i) => i.seq)).toEqual([...all.map((i) => i.seq)].sort((a, b) => a - b));
    expect(new Set(all.map((i) => i.seq)).size).toBe(120);
    expect(new Set(all.map((i) => Buffer.from(i.bytes).toString("hex")))).toEqual(new Set(sent));
    const tail = await drainAll(home, c.receive, all[99]?.seq ?? 0);
    expect(tail).toHaveLength(20);
  }, 120_000);

  it("minted deposit capabilities are independent: revoking one leaves the others; revoke is idempotent; mint needs the admin capability", async () => {
    const c = await queue();
    const m1 = await relay.mint(home, c.admin);
    const m2 = await relay.mint(home, c.admin);
    if (!m1.ok || !m2.ok) throw new Error("mint failed");
    expect((await relay.deposit(home, m1.value.cap, sealed())).ok).toBe(true);
    expect((await relay.revoke(home, c.admin, m1.value.id)).ok).toBe(true);
    expect((await relay.revoke(home, c.admin, m1.value.id)).ok).toBe(true);
    expect((await relay.revoke(home, c.admin, rand(16))).ok).toBe(true); // not_found is success-equivalent
    const after = await relay.deposit(home, m1.value.cap, sealed());
    expect(!after.ok && after.error.name).toBe("revoked");
    expect((await relay.deposit(home, m2.value.cap, sealed())).ok).toBe(true);
    expect((await relay.deposit(home, c.deposit, sealed())).ok).toBe(true);
    const bad = await relay.mint(home, c.receive);
    expect(!bad.ok && bad.error.name).toBe("unauthorized");
    const limited = await relay.mint(home, c.admin, { maxItems: 1 });
    if (!limited.ok) throw new Error("mint failed");
    expect((await relay.deposit(home, limited.value.cap, sealed())).ok).toBe(true);
    const over = await relay.deposit(home, limited.value.cap, sealed());
    expect(!over.ok && over.error.name).toBe("quota");
  });

  it("queue creation is idempotent for identical input and refuses a second claimant", async () => {
    const c = newCaps();
    const rid = rand(16);
    expect((await relay.create(home, c, rid)).ok).toBe(true);
    expect((await relay.create(home, c, rid)).ok).toBe(true);
    expect((await relay.create(home, c)).ok).toBe(true); // same caps, new request id
    const squatter = { ...c, receive: new Uint8Array([...c.prefix, ...rand(32)]) };
    const r = await relay.create(home, squatter);
    expect(!r.ok && r.error.name).toBe("unauthorized");
  });

  it("PC-H02/H03: stored items and request_id idempotency survive a Home restart", async () => {
    const s = scratch();
    let h = await startHome({ persist: s.dir });
    try {
      const c = newCaps();
      expect((await relay.create(h, c)).ok).toBe(true);
      const rid = rand(16);
      const env = sealed();
      const first = await relay.deposit(h, c.deposit, env, rid);
      await h.dispose();
      h = await startHome({ persist: s.dir });
      const retry = await relay.deposit(h, c.deposit, env, rid); // response lost, process restarted, exact retry
      expect(first.ok && retry.ok && equalBytes(first.value.itemId, retry.value.itemId)).toBe(true);
      const items = await drainAll(h, c.receive);
      expect(items).toHaveLength(1);
      const differ = await relay.deposit(h, c.deposit, sealed(), rid);
      expect(!differ.ok && differ.error.name).toBe("invalid_request");
    } finally { await h.dispose(); s.remove(); }
  }, 120_000);
});

describe("HTTP binding hygiene (home-api.md section 3.1)", () => {
  it("never cacheable, no referrer, no cookies, permissive CORS without credentials", async () => {
    const c = await queue();
    for (const path of ["/.well-known/om-provider-capabilities", "/v1/nonexistent"]) {
      const r = await home.fetch(path);
      expect(r.headers.get("set-cookie")).toBeNull();
      expect(r.headers.get("referrer-policy")).toBe("no-referrer");
    }
    const d = await relay.deposit(home, c.deposit, sealed());
    expect(d.headers.get("cache-control")).toBe("no-store");
    expect(d.headers.get("access-control-allow-origin")).toBe("*");
    expect(d.headers.get("access-control-allow-credentials")).toBeNull();
    const pre = await home.fetch("/v1/relay/deposit", { method: "OPTIONS" });
    expect(pre.status).toBe(204);
    expect(pre.headers.get("access-control-allow-headers")).toContain("authorization");
  });

  it("every error is a registered typed HomeError; unknown paths and bad bodies are typed too", async () => {
    const nf = await home.fetch("/v1/nope", { method: "POST", body: new Uint8Array([1]) });
    expect(nf.status).toBe(404);
    const bad = await home.fetch("/v1/relay/deposit", { method: "POST", body: new Uint8Array([0xa1, 0x00, 0x02]) });
    expect(bad.status).toBe(400);
    const get = await home.fetch("/v1/relay/deposit");
    expect(get.status).toBe(400);
  });
});
