// SPDX-License-Identifier: AGPL-3.0-or-later
// Optional feature om.recovery.store.v1 (ADR-0201): the RecoveryStore, the recovery inbox and the veto board of the reference Home,
// over raw HTTP against the real Worker bundle in workerd. Ciphertext and signatures are random bytes: the Home checks shape only
// and must not care. The time-dependent bounds (a day, an hour, retention, a full inbox) are in recovery.unit.test.ts.
import {
  ADMISSION_HEADER, admissionHash, decodeHomeError, decodeInboxDrainResponse, decodeRecordGetResponse, decodeVetoList, encode, encodeFanout, map, encodeInboxAck, encodeInboxDrain, encodeInboxRegister, encodeRecordPut, encodeVetoPost, recoveryEtag, recoveryInboxId, toBase64Url, toHex,
  type HomeErrorBody,
} from "@om/home-wire";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { scratch, startHome, type Home } from "./harness";
import { asBody, bearer } from "./kit";
import { envelope, maxEnvelope, rand, recordCaps, requestDigestOf, signedRequest, signedVeto } from "./recovery-kit";

const secret = rand(32);
const owner = { [ADMISSION_HEADER]: toBase64Url(secret) };
let home: Home;
let open: Home;
let unbound: Home;
/** A Home of its own, for the tests that spend the Home-wide post bucket (burst 10), so the order of the tests does not matter. */
const fresh = async (extra: Record<string, string> = {}): Promise<Home> => startHome({ vars: { OM_ADMISSION: await admissionHash(secret), ...extra } });
beforeAll(async () => {
  const vars = { OM_ADMISSION: await admissionHash(secret) };
  home = await startHome({ vars });
  open = await startHome(); // no admission: a shared or test Home
  unbound = await startHome({ vars, recovery: false }); // a deployment whose configuration predates the store
}, 120_000);
afterAll(async () => { await home.dispose(); await open.dispose(); await unbound.dispose(); });

interface Answer { readonly status: number; readonly headers: Headers; readonly bytes: Uint8Array; readonly error?: HomeErrorBody }
async function send(h: Home, path: string, init: RequestInit): Promise<Answer> {
  const r = await h.fetch(path, init);
  const bytes = new Uint8Array(await r.arrayBuffer());
  const base = { status: r.status, headers: r.headers, bytes };
  return r.status >= 300 ? { ...base, error: decodeHomeError(bytes) } : base;
}
const name = (a: Answer): string => a.error?.name ?? "ok";
const cbor = { "content-type": "application/cbor" };
const rec = (locator: Uint8Array): string => `/v1/recovery/records/${toBase64Url(locator)}`;
const etagHeader = (e: Uint8Array): string => `"${toBase64Url(e)}"`;

const create = (h: Home, c: { locator: Uint8Array; writeCap: Uint8Array }, env: Uint8Array, o: { admission?: Record<string, string>; requestId?: Uint8Array } = {}) =>
  send(h, rec(c.locator), { method: "PUT", headers: { ...cbor, ...bearer(c.writeCap), "if-none-match": "*", ...(o.admission ?? owner) }, body: asBody(encodeRecordPut({ requestId: o.requestId ?? rand(16), envelope: env })) });
const replace = (h: Home, c: { locator: Uint8Array; writeCap: Uint8Array }, ifMatch: Uint8Array, env: Uint8Array, requestId = rand(16)) =>
  send(h, rec(c.locator), { method: "PUT", headers: { ...cbor, ...bearer(c.writeCap), "if-match": etagHeader(ifMatch) }, body: asBody(encodeRecordPut({ requestId, envelope: env })) });
const get = (h: Home, locator: Uint8Array, cap: Uint8Array, extra: Record<string, string> = {}) => send(h, rec(locator), { headers: { ...bearer(cap), ...extra } });
const etagOf = (a: Answer): Uint8Array => {
  const m = /^"([A-Za-z0-9_-]{43})"$/.exec(a.headers.get("etag") ?? "");
  if (!m?.[1]) throw new Error("no ETag");
  return new Uint8Array(Buffer.from(m[1], "base64url"));
};

const inboxPath = (id: Uint8Array, sub = ""): string => `/v1/recovery/inbox/${toBase64Url(id)}${sub}`;
async function newInbox(h: Home) {
  const accountId = rand(32);
  const inboxId = await recoveryInboxId(accountId, 1);
  const cap = rand(32);
  const r = await send(h, inboxPath(inboxId), { method: "PUT", headers: { ...cbor, ...owner }, body: asBody(encodeInboxRegister({ requestId: rand(16), cap })) });
  expect(name(r)).toBe("ok");
  return { accountId, inboxId, cap };
}
const fanout = (h: Home, signed: Uint8Array, requestId = rand(16)) => send(h, "/v1/recovery/requests", { method: "POST", headers: cbor, body: asBody(encodeFanout({ requestId, signed })) });
const postVeto = (h: Home, signed: Uint8Array, cap?: Uint8Array) => send(h, "/v1/recovery/vetoes", { method: "POST", headers: { ...cbor, ...(cap === undefined ? {} : bearer(cap)) }, body: asBody(encodeVetoPost({ requestId: rand(16), signed })) });
const drain = (h: Home, i: { inboxId: Uint8Array; cap: Uint8Array }, pageSize?: number) =>
  send(h, inboxPath(i.inboxId, "/drain"), { method: "POST", headers: cbor, body: asBody(encodeInboxDrain({ requestId: rand(16), cap: i.cap, ...(pageSize === undefined ? {} : { pageSize }) })) });
const ack = (h: Home, i: { inboxId: Uint8Array; cap: Uint8Array }, ids: Uint8Array[]) =>
  send(h, inboxPath(i.inboxId, "/ack"), { method: "POST", headers: cbor, body: asBody(encodeInboxAck({ requestId: rand(16), cap: i.cap, itemIds: ids })) });
const board = (h: Home, digest: Uint8Array) => send(h, `/v1/recovery/requests/${toBase64Url(digest)}/vetoes`, {});

describe("PC-R recovery store (om.recovery.store.v1)", () => {
  it("PC-R01 is advertised, with its scope id, kind and limits, only where bound, not switched off and with owner admission", async () => {
    const caps = async (h: Home) => (await (await h.fetch("/.well-known/om-provider-capabilities")).json()) as { features: string[]; provider_kinds: string[]; limits: Record<string, unknown>; recovery_scope_id?: string };
    const on = await caps(home);
    expect(on.features).toContain("om.recovery.store.v1");
    expect(on.provider_kinds).toContain("recovery");
    expect(on.recovery_scope_id).toMatch(/^[A-Za-z0-9_-]{43}$/);
    expect(Buffer.from(on.recovery_scope_id ?? "", "base64url")).toHaveLength(32);
    expect(on.limits).toMatchObject({
      max_recovery_record_bytes: 1_048_576, max_recovery_records: 16, max_recovery_inboxes: 16, recovery_put_rate: { burst: 6, refill_seconds: 600 }, recovery_get_rate: { burst: 10, refill_seconds: 30 },
      recovery_failed_get_rate: { burst: 30, refill_seconds: 10 }, recovery_requests_per_day: 3, recovery_pending_requests: 8, recovery_pending_vetoes: 32, recovery_pending_vetoes_per_request: 16,
      recovery_vetoes_per_hour: 8, recovery_post_rate: { burst: 10, refill_seconds: 60 }, recovery_drain_rate: { burst: 20, refill_seconds: 5 }, recovery_veto_read_rate: { burst: 10, refill_seconds: 30 },
      max_recovery_object_bytes: 16_384, recovery_item_retention_seconds: 14 * 86400, recovery_record_retention_seconds: 400 * 86400, recovery_inbox_retention_seconds: 400 * 86400,
    });
    expect((await caps(home)).recovery_scope_id).toBe(on.recovery_scope_id); // stable
    for (const h of [open, unbound]) {
      const c = await caps(h);
      expect(c.features).not.toContain("om.recovery.store.v1");
      expect(c.provider_kinds).not.toContain("recovery");
      expect(c).not.toHaveProperty("recovery_scope_id");
      expect(c.limits).not.toHaveProperty("max_recovery_records");
    }
    const off = await startHome({ vars: { OM_ADMISSION: await admissionHash(secret), OM_RECOVERY_STORE: "0" } });
    try { expect((await (await off.fetch("/.well-known/om-provider-capabilities")).json() as { features: string[] }).features).not.toContain("om.recovery.store.v1"); } finally { await off.dispose(); }
    const personal = await startHome({ vars: { OM_ADMISSION: await admissionHash(secret), OM_PROFILE: "personal" } });
    try { expect((await (await personal.fetch("/.well-known/om-provider-capabilities")).json() as { limits: Record<string, number> }).limits).toMatchObject({ max_recovery_records: 4, max_recovery_inboxes: 4 }); } finally { await personal.dispose(); }
  });

  it("PC-R02 a Home that does not offer the store answers every recovery path not_found and its other features are untouched", async () => {
    const c = await recordCaps();
    for (const h of [open, unbound]) {
      expect(name(await create(h, c, envelope()))).toBe("not_found");
      expect(name(await get(h, c.locator, c.readCap))).toBe("not_found");
      expect(name(await fanout(h, signedRequest().signed))).toBe("not_found");
      expect(name(await board(h, rand(32)))).toBe("not_found");
      expect((await h.fetch("/healthz")).status).toBe(200);
    }
  });

  it("PC-R03 creating a record or registering an inbox needs the owner's admission: missing, wrong or short is admission_denied, identically, and stores nothing", async () => {
    const c = await recordCaps();
    for (const admission of [{}, { [ADMISSION_HEADER]: toBase64Url(rand(32)) }, { [ADMISSION_HEADER]: toBase64Url(rand(16)) }]) {
      const r = await create(home, c, envelope(), { admission });
      expect(name(r)).toBe("admission_denied");
      expect(r.status).toBe(403);
      expect(name(await get(home, c.locator, c.readCap))).toBe("not_found");
      const id = await recoveryInboxId(rand(32), 1);
      const reg = await send(home, inboxPath(id), { method: "PUT", headers: { ...cbor, ...admission }, body: asBody(encodeInboxRegister({ requestId: rand(16), cap: rand(32) })) });
      expect(name(reg)).toBe("admission_denied");
    }
    // a replace needs no admission: it is proved by the bound write capability
    const made = await create(home, c, envelope());
    expect(name(made)).toBe("ok");
    expect(name(await replace(home, c, etagOf(made), envelope()))).toBe("ok");
  });

  it("PC-R04 a 1 MiB record (the largest conforming envelope) round-trips byte for byte; the ETag is the hash of the stored bytes; no response carries a modification time", async () => {
    const c = await recordCaps();
    const big = maxEnvelope();
    const made = await create(home, c, big);
    expect(made.status).toBe(204);
    expect(toHex(etagOf(made))).toBe(toHex(await recoveryEtag(big)));
    const got = await get(home, c.locator, c.readCap, { "om-request-id": toBase64Url(rand(16)) });
    expect(got.status).toBe(200);
    expect(Buffer.from(decodeRecordGetResponse(got.bytes)).equals(Buffer.from(big))).toBe(true);
    expect(toHex(etagOf(got))).toBe(toHex(etagOf(made)));
    for (const a of [made, got, await board(home, rand(32))]) {
      expect(a.headers.get("cache-control")).toBe("no-store");
      expect(a.headers.get("referrer-policy")).toBe("no-referrer");
      expect(a.headers.get("last-modified")).toBeNull();
      expect(a.headers.get("set-cookie")).toBeNull();
    }
    expect(got.headers.get("access-control-expose-headers")).toContain("ETag");
    expect(name(await get(home, c.locator, c.readCap, { "om-request-id": "not-an-id" }))).toBe("invalid_request");
  });

  it("PC-R05 the capability is only in the Authorization header: not in the path, and a capability in a query or without the header is just a wrong one", async () => {
    const c = await recordCaps();
    expect(name(await create(home, c, envelope()))).toBe("ok");
    expect(rec(c.locator)).not.toContain(toBase64Url(c.readCap));
    expect(rec(c.locator)).not.toContain(toBase64Url(c.writeCap));
    expect(name(await send(home, `${rec(c.locator)}?cap=${toBase64Url(c.readCap)}`, {}))).toBe("not_found");
    expect(name(await send(home, rec(c.locator), { headers: { authorization: `Bearer ${toBase64Url(rand(8))}` } }))).toBe("not_found"); // too short
    expect(name(await send(home, rec(c.locator), { headers: { authorization: `Basic ${toBase64Url(c.readCap)}` } }))).toBe("not_found");
    expect(name(await get(home, c.locator, c.readCap))).toBe("ok");
    // the locator is the one the read capability derives, not just any hash of it
    expect(name(await get(home, await recordCaps().then((x) => x.locator), c.readCap))).toBe("not_found");
  });

  it("PC-R06 compare-and-swap: a stale If-Match is stale_generation (409), a create on an existing locator too; the preconditions are required, exactly one", async () => {
    const c = await recordCaps();
    const made = await create(home, c, envelope());
    const second = await replace(home, c, etagOf(made), envelope());
    expect(name(second)).toBe("ok");
    const stale = await replace(home, c, etagOf(made), envelope());
    expect([stale.status, name(stale)]).toEqual([409, "stale_generation"]);
    expect(name(await create(home, c, envelope()))).toBe("stale_generation");
    const body = asBody(encodeRecordPut({ requestId: rand(16), envelope: envelope() }));
    expect(name(await send(home, rec(c.locator), { method: "PUT", headers: { ...cbor, ...bearer(c.writeCap), ...owner }, body }))).toBe("invalid_request"); // neither
    expect(name(await send(home, rec(c.locator), { method: "PUT", headers: { ...cbor, ...bearer(c.writeCap), ...owner, "if-match": etagHeader(etagOf(second)), "if-none-match": "*" }, body }))).toBe("invalid_request"); // both
    expect(name(await send(home, rec(c.locator), { method: "PUT", headers: { ...cbor, ...bearer(c.writeCap), "if-match": "*" }, body }))).toBe("invalid_request");
    expect(name(await send(home, rec(c.locator), { method: "PUT", headers: { ...cbor, ...bearer(c.writeCap), "if-none-match": "\"x\"", ...owner }, body }))).toBe("invalid_request");
    const got = await get(home, c.locator, c.readCap);
    expect(toHex(etagOf(got))).toBe(toHex(etagOf(second))); // the refused writes changed nothing
  });

  it("PC-R07 a wrong capability and an absent record are indistinguishable: same status, same body, for get, replace and delete", async () => {
    const c = await recordCaps();
    const made = await create(home, c, envelope());
    const ghost = await recordCaps();
    const g1 = await get(home, c.locator, rand(32));
    const g2 = await get(home, ghost.locator, ghost.readCap);
    expect([g1.status, toHex(g1.bytes)]).toEqual([g2.status, toHex(g2.bytes)]);
    expect(name(g1)).toBe("not_found");
    const p1 = await replace(home, { locator: c.locator, writeCap: rand(32) }, etagOf(made), envelope());
    const p2 = await replace(home, ghost, rand(32), envelope());
    expect([p1.status, toHex(p1.bytes)]).toEqual([p2.status, toHex(p2.bytes)]);
    expect(name(p1)).toBe("unauthorized");
    const d1 = await send(home, rec(c.locator), { method: "DELETE", headers: bearer(rand(32)) });
    const d2 = await send(home, rec(ghost.locator), { method: "DELETE", headers: bearer(ghost.writeCap) });
    expect([d1.status, toHex(d1.bytes)]).toEqual([d2.status, toHex(d2.bytes)]);
    expect(name(await get(home, c.locator, c.readCap))).toBe("ok"); // none of it touched the record
    expect(name(await send(home, rec(c.locator), { method: "DELETE", headers: bearer(c.readCap) }))).toBe("not_found"); // the read capability cannot delete
    expect(name(await send(home, rec(c.locator), { method: "DELETE", headers: bearer(c.writeCap) }))).toBe("ok");
    expect(name(await send(home, rec(c.locator), { method: "DELETE", headers: bearer(c.writeCap) }))).toBe("not_found"); // idempotent: a client counts it as done
    expect(name(await get(home, c.locator, c.readCap))).toBe("not_found");
  });

  it("PC-R08 oversize and malformed records: one byte over 1,048,576 is payload_too_large (413), a wrong shape is invalid_request, and neither is stored", async () => {
    const c = await recordCaps();
    const tooBig = new Uint8Array(1_048_577);
    const r = await create(home, c, tooBig);
    expect([r.status, name(r)]).toEqual([413, "payload_too_large"]);
    const huge = await send(home, rec(c.locator), { method: "PUT", headers: { ...cbor, ...bearer(c.writeCap), "if-none-match": "*", ...owner }, body: asBody(new Uint8Array(1_048_576 + 5000)) });
    expect(name(huge)).toBe("payload_too_large");
    for (const bad of [
      new Uint8Array([1, 2, 3]), // not CBOR we know
      envelope(16_399), // below the smallest bucket plus tag
      envelope(16_400, { 24: new Uint8Array(1) }), // an unknown top-level key
      (() => { const e = envelope(); e[3] = 0x35; return e; })(), // not type 35
    ]) expect(name(await create(home, c, bad))).toBe("invalid_request");
    expect(name(await get(home, c.locator, c.readCap))).toBe("not_found");
    expect(name(await create(home, c, envelope()))).toBe("ok"); // the locator was never taken
  });

  it("PC-R09 put is idempotent by request id, also across a process restart; other bytes under that id are invalid_request", async () => {
    const state = scratch();
    let h = await startHome({ vars: { OM_ADMISSION: await admissionHash(secret) }, persist: state.dir });
    try {
      const c = await recordCaps();
      const rid = rand(16);
      const env = envelope();
      const made = await create(h, c, env, { requestId: rid });
      expect(name(made)).toBe("ok");
      const again = await create(h, c, env, { requestId: rid });
      expect([name(again), toHex(etagOf(again))]).toEqual(["ok", toHex(etagOf(made))]);
      await h.dispose();
      h = await startHome({ vars: { OM_ADMISSION: await admissionHash(secret) }, persist: state.dir });
      const afterRestart = await create(h, c, env, { requestId: rid });
      expect([name(afterRestart), toHex(etagOf(afterRestart))]).toEqual(["ok", toHex(etagOf(made))]);
      expect(name(await create(h, c, envelope(), { requestId: rid }))).toBe("invalid_request");
      expect(name(await get(h, c.locator, c.readCap))).toBe("ok"); // the record and its ETag survived
      const fresh = await replace(h, c, etagOf(made), envelope());
      expect(name(fresh)).toBe("ok");
    } finally { await h.dispose(); state.remove(); }
  });

  it("PC-R10 put rate: burst 6 per write capability (the create counts), then rate_limited (429) with Retry-After; another record is unaffected", async () => {
    const c = await recordCaps();
    let etag = etagOf(await create(home, c, envelope()));
    for (let i = 0; i < 5; i++) etag = etagOf(await replace(home, c, etag, envelope()));
    const limited = await replace(home, c, etag, envelope());
    expect([limited.status, name(limited)]).toEqual([429, "rate_limited"]);
    expect(Number(limited.headers.get("retry-after"))).toBeGreaterThan(0);
    const other = await recordCaps();
    expect(name(await create(home, other, envelope()))).toBe("ok");
  });

  it("PC-R11 the personal profile holds 4 records and 4 inboxes; the next create or register is quota (429), and an existing one is never evicted", async () => {
    const p = await startHome({ vars: { OM_ADMISSION: await admissionHash(secret), OM_PROFILE: "personal" } });
    try {
      const rs = [];
      for (let i = 0; i < 4; i++) { const c = await recordCaps(); expect(name(await create(p, c, envelope()))).toBe("ok"); rs.push(c); }
      const fifth = await create(p, await recordCaps(), envelope());
      expect([fifth.status, name(fifth)]).toEqual([429, "quota"]);
      for (const c of rs) expect(name(await get(p, c.locator, c.readCap))).toBe("ok");
      for (let i = 0; i < 4; i++) await newInbox(p);
      const reg = await send(p, inboxPath(await recoveryInboxId(rand(32), 1)), { method: "PUT", headers: { ...cbor, ...owner }, body: asBody(encodeInboxRegister({ requestId: rand(16), cap: rand(32) })) });
      expect(name(reg)).toBe("quota");
    } finally { await p.dispose(); }
  });

  describe("with a Home of their own (they spend the Home-wide post bucket)", () => {
    let home: Home;
    beforeEach(async () => { home = await fresh(); }, 60_000);
    afterEach(async () => { await home.dispose(); });

    it("PC-R20 the inbox routes by account and recovery epoch", async () => {
      const old = await newInbox(home);
      const current = { inboxId: await recoveryInboxId(old.accountId, 2), cap: rand(32) };
      const made = await send(home, inboxPath(current.inboxId), {
        method: "PUT",
        headers: { ...cbor, ...owner },
        body: asBody(encodeInboxRegister({ requestId: rand(16), cap: current.cap })),
      });
      expect(name(made)).toBe("ok");
      expect(name(await fanout(home, signedRequest(old.accountId, 2).signed))).toBe("ok");
      expect(decodeInboxDrainResponse((await drain(home, old)).bytes).items).toHaveLength(0);
      expect(decodeInboxDrainResponse((await drain(home, current)).bytes).items).toHaveLength(1);
      expect(name(await drain(home, { ...current, cap: old.cap }))).toBe("unauthorized");
    });

    it("PC-R21 controller vetoes precede public vetoes and post replies are identical", async () => {
      const i = await newInbox(home);
      const req = signedRequest(i.accountId);
      expect(name(await fanout(home, req.signed))).toBe("ok");
      const digest = await requestDigestOf(req);
      const publicVeto = signedVeto(i.accountId, digest, 1);
      const controllerVeto = signedVeto(i.accountId, digest, 2);
      const publicAnswer = await postVeto(home, publicVeto, rand(32));
      const controllerAnswer = await postVeto(home, controllerVeto, i.cap);
      expect([publicAnswer.status, toHex(publicAnswer.bytes)]).toEqual([204, ""]);
      expect([controllerAnswer.status, toHex(controllerAnswer.bytes)]).toEqual([204, ""]);
      expect(decodeVetoList((await board(home, digest)).bytes)).toEqual([controllerVeto, publicVeto]);
    });

    for (const present of [true, false]) {
      it(`PC-R22 board pacing at the burst boundary for a ${present ? "present" : "missing"} digest`, async () => {
        const i = await newInbox(home);
        const req = signedRequest(i.accountId);
        if (present) expect(name(await fanout(home, req.signed))).toBe("ok");
        const digest = await requestDigestOf(req);
        for (let n = 0; n < 10; n++) {
          const read = await board(home, digest);
          expect(read.status).toBe(200);
          expect(decodeVetoList(read.bytes)).toEqual([]);
        }
        const denied = await board(home, digest);
        expect([denied.status, name(denied)]).toEqual([429, "rate_limited"]);
      });
    }

    it("PC-R12 fan-out reaches a registered inbox; the controller drains {id, kind, bytes} without any time and acknowledges; a duplicate changes nothing", async () => {
      const i = await newInbox(home);
      const req = signedRequest(i.accountId);
      expect(name(await fanout(home, req.signed))).toBe("ok");
      expect(name(await fanout(home, req.signed))).toBe("ok");
      const d = await drain(home, i);
      const page = decodeInboxDrainResponse(d.bytes);
      expect(page.items).toHaveLength(1);
      expect(Object.keys(page.items[0] ?? {}).sort()).toEqual(["bytes", "id", "kind"]);
      expect(page.items[0]?.kind).toBe(1);
      expect(Buffer.from(page.items[0]?.bytes ?? new Uint8Array(0)).equals(Buffer.from(req.signed))).toBe(true);
      expect(name(await ack(home, i, [page.items[0]?.id ?? rand(16), rand(16)]))).toBe("ok");
      expect(decodeInboxDrainResponse((await drain(home, i)).bytes).items).toHaveLength(0);
    });

    it("PC-R13 posting to an inbox that exists and to one that does not is answered identically (status, headers that matter, body bytes)", async () => {
      const i = await newInbox(home);
      const a = await fanout(home, signedRequest(i.accountId).signed);
      const b = await fanout(home, signedRequest(rand(32)).signed);
      expect([a.status, toHex(a.bytes)]).toEqual([b.status, toHex(b.bytes)]);
      expect(a.status).toBe(204);
      // the same for a veto that names a request the inbox holds and one it does not
      const held = signedRequest(i.accountId);
      await fanout(home, held.signed);
      const v1 = await postVeto(home, signedVeto(i.accountId, await requestDigestOf(held)));
      const v2 = await postVeto(home, signedVeto(i.accountId, rand(32)));
      const v3 = await postVeto(home, signedVeto(rand(32), rand(32)));
      for (const v of [v2, v3]) expect([v.status, toHex(v.bytes)]).toEqual([v1.status, toHex(v1.bytes)]);
    });

    it("PC-R14 drain and ack: a wrong capability and an absent inbox are one unauthorized", async () => {
      const i = await newInbox(home);
      const ghost = { inboxId: await recoveryInboxId(rand(32), 1), cap: rand(32) };
      const a = await drain(home, { inboxId: i.inboxId, cap: rand(32) });
      const b = await drain(home, ghost);
      expect([a.status, toHex(a.bytes)]).toEqual([b.status, toHex(b.bytes)]);
      expect(name(a)).toBe("unauthorized");
      const c = await ack(home, { inboxId: i.inboxId, cap: rand(32) }, [rand(16)]);
      const d = await ack(home, ghost, [rand(16)]);
      expect([c.status, toHex(c.bytes)]).toEqual([d.status, toHex(d.bytes)]);
      expect(name(await send(home, inboxPath(i.inboxId, "/drain"), { method: "POST", headers: cbor, body: asBody(encodeInboxDrain({ requestId: rand(16), cap: rand(16) })) }))).toBe("unauthorized"); // not 256 bits
      // registering again with another capability does not take the inbox over
      const take = await send(home, inboxPath(i.inboxId), { method: "PUT", headers: { ...cbor, ...owner }, body: asBody(encodeInboxRegister({ requestId: rand(16), cap: rand(32) })) });
      expect(name(take)).toBe("unauthorized");
      expect(name(await send(home, inboxPath(i.inboxId), { method: "PUT", headers: { ...cbor, ...owner }, body: asBody(encodeInboxRegister({ requestId: rand(16), cap: i.cap })) }))).toBe("ok"); // the same one refreshes
    });

    it("PC-R15 the veto board: a veto is stored only for a request the inbox holds, read by anyone who knows the digest, and an ack does not remove it", async () => {
      const i = await newInbox(home);
      const req = signedRequest(i.accountId);
      const digest = await requestDigestOf(req);
      await postVeto(home, signedVeto(i.accountId, digest)); // before the request: dropped
      expect(decodeVetoList((await board(home, digest)).bytes)).toHaveLength(0);
      await fanout(home, req.signed);
      const v = signedVeto(i.accountId, digest);
      expect(name(await postVeto(home, v))).toBe("ok");
      const read = await board(home, digest); // public read: no Authorization
      expect(read.status).toBe(200);
      expect(decodeVetoList(read.bytes).map((b) => toHex(b))).toEqual([toHex(v)]);
      const items = decodeInboxDrainResponse((await drain(home, i)).bytes).items;
      expect(items.map((x) => x.kind)).toEqual([1, 2]);
      await ack(home, i, items.map((x) => x.id));
      expect(decodeVetoList((await board(home, digest)).bytes)).toHaveLength(1);
      expect(decodeVetoList((await board(home, rand(32))).bytes)).toHaveLength(0); // unknown digest: an empty list, like a known one without vetoes
    });

    it("PC-R16 posts are shape-checked: garbage, the wrong object type, an unknown key and oversize are refused; the Home checks no signature", async () => {
      const i = await newInbox(home);
      const good = signedRequest(i.accountId);
      expect(name(await fanout(home, new Uint8Array([1, 2, 3])))).toBe("invalid_request");
      expect(name(await fanout(home, signedVeto(i.accountId, rand(32))))).toBe("invalid_request"); // a veto is not a request
      expect(name(await postVeto(home, good.signed))).toBe("invalid_request"); // and a request is not a veto
      const noSig = encode(map([[1, good.body], [2, []]]));
      expect(name(await fanout(home, noSig))).toBe("invalid_request");
      const big = await send(home, "/v1/recovery/requests", { method: "POST", headers: cbor, body: asBody(new Uint8Array(17_000)) });
      expect([big.status, name(big)]).toEqual([413, "payload_too_large"]);
      // random bytes where the signature belongs are accepted as to shape: the Home never judges a signature
      expect(name(await fanout(home, good.signed))).toBe("ok");
    });
  });

  it("PC-R17 the Home-wide post bucket is the one limit a poster sees: burst 10, then rate_limited (429) with Retry-After", async () => {
    const p = await startHome({ vars: { OM_ADMISSION: await admissionHash(secret) } });
    try {
      for (let n = 0; n < 10; n++) expect(name(await fanout(p, signedRequest(rand(32)).signed))).toBe("ok");
      const limited = await fanout(p, signedRequest(rand(32)).signed);
      expect([limited.status, name(limited)]).toEqual([429, "rate_limited"]);
      expect(Number(limited.headers.get("retry-after"))).toBeGreaterThan(0);
      expect(name(await postVeto(p, signedVeto(rand(32), rand(32))))).toBe("rate_limited");
    } finally { await p.dispose(); }
  });

  it("PC-R18 failed gets share one Home-wide bucket (30, refill 1 per 10 s) and the owner's own valid get is not blocked by them", async () => {
    const p = await startHome({ vars: { OM_ADMISSION: await admissionHash(secret) } });
    try {
      const c = await recordCaps();
      await create(p, c, envelope());
      for (let n = 0; n < 30; n++) expect(name(await get(p, (await recordCaps()).locator, rand(32)))).toBe("not_found");
      const limited = await get(p, c.locator, rand(32));
      expect([limited.status, name(limited)]).toEqual([429, "rate_limited"]);
      expect(name(await get(p, (await recordCaps()).locator, rand(32)))).toBe("rate_limited"); // an unknown locator: the same
      expect(name(await get(p, c.locator, c.readCap))).toBe("ok");
    } finally { await p.dispose(); }
  });

  it("PC-R19 records and inboxes survive a restart; the scope id does too", async () => {
    const state = scratch();
    const vars = { OM_ADMISSION: await admissionHash(secret) };
    let h = await startHome({ vars, persist: state.dir });
    try {
      const c = await recordCaps();
      await create(h, c, envelope());
      const i = await newInbox(h);
      const req = signedRequest(i.accountId);
      await fanout(h, req.signed);
      const scope = ((await (await h.fetch("/.well-known/om-provider-capabilities")).json()) as { recovery_scope_id: string }).recovery_scope_id;
      await h.dispose();
      h = await startHome({ vars, persist: state.dir });
      expect(name(await get(h, c.locator, c.readCap))).toBe("ok");
      expect(decodeInboxDrainResponse((await drain(h, i)).bytes).items).toHaveLength(1);
      expect(((await (await h.fetch("/.well-known/om-provider-capabilities")).json()) as { recovery_scope_id: string }).recovery_scope_id).toBe(scope);
    } finally { await h.dispose(); state.remove(); }
  });
});
