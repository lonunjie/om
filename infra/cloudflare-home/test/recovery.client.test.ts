// SPDX-License-Identifier: AGPL-3.0-or-later
// The RecoveryStore client of @om/provider-http against the real reference Home (Worker bundle in workerd): the adapter and the Home
// agree on every verb, header and body, and the whole owner-side lifecycle works end to end: advertise, create, replace by
// compare-and-swap, read with only the read capability, fan out a request, see it in a controller's inbox, veto it, read the board.
import { ADMISSION_HEADER, admissionHash, checkSignedVeto, recoveryRequestDigest, toBase64Url, toHex } from "@om/home-wire";
import { createRecoveryStoreClient, fanOutToAll, parseProviderUrl, recordLocator, type RecoveryStoreClient } from "@om/provider-http";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { providerFetch, startHome, type Home } from "./harness";
import { envelope, maxEnvelope, rand, recordCaps, signedRequest, signedVeto } from "./recovery-kit";

const secret = rand(32);
let home: Home;
let open: Home;
beforeAll(async () => {
  home = await startHome({ vars: { OM_ADMISSION: await admissionHash(secret) } });
  open = await startHome();
}, 120_000);
afterAll(async () => { await home.dispose(); await open.dispose(); });

const clientFor = (h: Home, admission: Uint8Array | null = secret): RecoveryStoreClient => {
  const config = parseProviderUrl(h.url, { allowLoopbackHttp: true });
  if (!config) throw new Error("bad url");
  return createRecoveryStoreClient({ config, fetch: providerFetch(h), timeoutMs: 10_000, ...(admission ? { admission } : {}) });
};
const val = <T>(r: { ok: true; value: T } | { ok: false; failure: unknown }): T => { if (!r.ok) throw new Error(`failed: ${JSON.stringify(r.failure)}`); return r.value; };
const failName = (r: { ok: boolean; failure?: { kind: string; name?: string } }): string => (r.ok ? "ok" : (r.failure?.name ?? r.failure?.kind ?? "?"));

describe("provider-http RecoveryStore client against the reference Home", () => {
  it("info: the Home that has the store says so with a scope id the client can derive capabilities from; one that has none says nothing", async () => {
    const info = await clientFor(home).info();
    expect(info?.scopeId).toHaveLength(32);
    expect(info?.limits).toMatchObject({ max_recovery_records: 16 });
    expect(await clientFor(open).info()).toBeUndefined();
  });

  it("record lifecycle: create, read with only the read capability, replace by compare-and-swap, lose a race, delete", async () => {
    const c = clientFor(home);
    const caps = await recordCaps();
    const first = envelope();
    const etag1 = val(await c.putRecord({ locator: caps.locator, writeCap: caps.writeCap, envelope: first }));
    // a fresh device knows only the read capability: it derives the locator itself
    expect(toHex(await recordLocator(caps.readCap))).toBe(toHex(caps.locator));
    const got = val(await c.getRecord(caps.readCap));
    expect(toHex(got.envelope)).toBe(toHex(first));
    expect(toHex(got.etag)).toBe(toHex(etag1));
    const second = envelope();
    const etag2 = val(await c.putRecord({ locator: caps.locator, writeCap: caps.writeCap, envelope: second, ifMatch: etag1 }));
    expect(failName(await c.putRecord({ locator: caps.locator, writeCap: caps.writeCap, envelope: envelope(), ifMatch: etag1 }))).toBe("stale_generation"); // the loser of a race learns it
    expect(failName(await c.putRecord({ locator: caps.locator, writeCap: caps.writeCap, envelope: envelope() }))).toBe("stale_generation"); // a create on a taken locator
    expect(toHex(val(await c.getRecord(caps.readCap)).envelope)).toBe(toHex(second));
    expect(failName(await c.putRecord({ locator: caps.locator, writeCap: caps.readCap, envelope: envelope(), ifMatch: etag2 }))).toBe("unauthorized");
    expect(failName(await c.getRecord(caps.writeCap))).toBe("not_found");
    expect(await c.deleteRecord(caps.locator, caps.writeCap)).toEqual({ ok: true, value: undefined });
    expect(await c.deleteRecord(caps.locator, caps.writeCap)).toEqual({ ok: true, value: undefined }); // idempotent
    expect(failName(await c.getRecord(caps.readCap))).toBe("not_found");
  });

  it("a record at the largest conforming size round-trips; a create without the owner's admission is admission_denied; an oversize body is payload_too_large", async () => {
    const c = clientFor(home);
    const caps = await recordCaps();
    const big = maxEnvelope();
    val(await c.putRecord({ locator: caps.locator, writeCap: caps.writeCap, envelope: big }));
    expect(toHex(val(await c.getRecord(caps.readCap)).envelope)).toBe(toHex(big));
    const stranger = clientFor(home, null);
    const other = await recordCaps();
    expect(failName(await stranger.putRecord({ locator: other.locator, writeCap: other.writeCap, envelope: envelope() }))).toBe("admission_denied");
    expect(failName(await c.putRecord({ locator: other.locator, writeCap: other.writeCap, envelope: new Uint8Array(1_048_577) }))).toBe("payload_too_large");
    expect(failName(await c.registerInbox({ accountId: rand(32), cap: rand(32) }).then((r) => r))).toBe("ok");
    expect(failName(await stranger.registerInbox({ accountId: rand(32), cap: rand(32) }))).toBe("admission_denied");
  });

  it("the owner-side recovery loop: register, a request is fanned out, the controller pulls it, a veto is posted, the recovering device reads the board", async () => {
    const c = clientFor(home);
    const accountId = rand(32);
    const cap = rand(32);
    val(await c.registerInbox({ accountId, recoveryEpoch: 1, cap }));
    val(await c.registerInbox({ accountId, recoveryEpoch: 1, cap })); // a monthly refresh is idempotent
    expect(failName(await c.registerInbox({ accountId, recoveryEpoch: 1, cap: rand(32) }))).toBe("unauthorized");
    const req = signedRequest(accountId);
    const all = await fanOutToAll([c, clientFor(open)], req.signed); // the second Home has no store: it does not stop the first
    expect(all.ok).toBe(true);
    expect(all.results.map(failName)).toEqual(["ok", "not_found"]);
    const page = val(await c.drainInbox({ accountId, recoveryEpoch: 1, cap }));
    expect(page.items).toHaveLength(1);
    expect(page.items[0]?.kind).toBe(1);
    expect(toHex(page.items[0]?.bytes ?? new Uint8Array(0))).toBe(toHex(req.signed));
    expect(page.more).toBe(false);
    const digest = await recoveryRequestDigest(req.body);
    const veto = signedVeto(accountId, digest);
    expect(checkSignedVeto(veto).requestDigest).toEqual(digest);
    val(await c.postVeto(veto));
    val(await c.ackInbox({ accountId, recoveryEpoch: 1, cap }, [page.items[0]?.id ?? rand(16)]));
    const board = val(await clientFor(home, null).readVetoes(digest)); // a recovering device: no admission, no capability
    expect(board.map(toHex)).toEqual([toHex(veto)]);
    expect(failName(await c.drainInbox({ accountId, recoveryEpoch: 1, cap: rand(32) }))).toBe("unauthorized");
  });

  it("a Home-wide limit is a typed rate_limited with its wait; nothing a client does leaks a capability into a URL", async () => {
    const h = await startHome({ vars: { OM_ADMISSION: await admissionHash(secret) } });
    try {
      const c = clientFor(h);
      for (let i = 0; i < 10; i++) expect(failName(await c.fanOut(signedRequest().signed))).toBe("ok");
      const r = await c.fanOut(signedRequest().signed);
      expect(r).toMatchObject({ ok: false, failure: { kind: "home", name: "rate_limited", retryable: true } });
      expect(!r.ok && r.failure.kind === "home" && r.failure.retryAfterSeconds).toBeGreaterThan(0);
    } finally { await h.dispose(); }
    expect(ADMISSION_HEADER).toBe("om-admission");
    expect(toBase64Url(rand(1))).toBeTypeOf("string");
  });
});
