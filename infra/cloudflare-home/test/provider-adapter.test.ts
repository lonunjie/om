// SPDX-License-Identifier: MPL-2.0
// The HTTP provider adapter against the real reference Home (Worker bundle in workerd).
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { decode, encode, map, routeBinding, toHex, type CborValue } from "@om/home-wire";
import { createHttpProvider, createMemorySecretStore, parseProviderUrl, type HttpProvider } from "@om/provider-http";
import { startHome, type Home } from "./harness";

const rand = (n: number): Uint8Array => crypto.getRandomValues(new Uint8Array(n));

describe("against the real reference Home", () => {
  let home: Home;
  beforeAll(async () => { home = await startHome(); }, 120_000);
  afterAll(async () => { await home.dispose(); });
  const mkReal = (secrets = createMemorySecretStore()): HttpProvider => {
    const config = parseProviderUrl(home.url, { allowLoopbackHttp: true });
    if (!config) throw new Error("bad url");
    return createHttpProvider({ config, secrets, timeoutMs: 10_000 });
  };
  const envelope = (): string => {
    // A structurally valid sealed wrapper (home-wire builds the same shape the Home checks).
    const e = new Map<number, unknown>([[0, 23], [1, 1], [2, 1], [3, rand(16)], [4, rand(16)], [5, rand(1120)], [6, rand(1040)], [7, Math.floor(Date.now() / 1000) + 3600]]);
    return toHex(encodeMap(e));
  };

  it("deposit, poll, cumulative ack; after a restart (empty id cache) the ack still frees the Home's storage", async () => {
    const secrets = createMemorySecretStore();
    const recipient = mkReal(secrets);
    const sender = mkReal();
    const mailbox = toHex(rand(16));
    expect(await recipient.poll(mailbox, 0)).toEqual([]); // first poll claims the queue
    const wrappers = [envelope(), envelope(), envelope()];
    for (const w of wrappers) expect(await sender.perform({ call: "deposit", mailbox, bytes: w })).toEqual({ result: "ok" });
    expect(await sender.perform({ call: "deposit", mailbox, bytes: wrappers[0] ?? "" })).toEqual({ result: "ok" }); // exact retry
    const got = await recipient.poll(mailbox, 0);
    expect(got.map((g) => g.bytes)).toEqual(wrappers);
    expect(new Set(got.map((g) => g.seq)).size).toBe(3);
    const last = got[2]?.seq ?? 0;
    // A restarted installation (same secret, new adapter) acknowledges without having seen the ids.
    const restarted = mkReal(secrets);
    expect(await restarted.perform({ call: "ack", mailbox, seq: last })).toEqual({ result: "ok" });
    expect(await restarted.perform({ call: "ack", mailbox, seq: last })).toEqual({ result: "ok" }); // idempotent
    expect(await restarted.poll(mailbox, 0)).toEqual([]);
    // cursors: nothing newer than `after`
    expect(await sender.perform({ call: "deposit", mailbox, bytes: envelope() })).toEqual({ result: "ok" });
    const more = await restarted.poll(mailbox, last);
    expect(more).toHaveLength(1);
  });

  it("a mailbox cannot be drained by the peer who knows its id; a second installation cannot claim it", async () => {
    const owner = mkReal();
    const mailbox = toHex(rand(16));
    await owner.poll(mailbox, 0);
    const peer = mkReal(); // knows the mailbox id (it deposits to it) but not the owner's install secret
    expect(await peer.perform({ call: "deposit", mailbox, bytes: envelope() })).toEqual({ result: "ok" });
    expect(await peer.poll(mailbox, 0)).toEqual([]);
    expect(peer.lostMailboxes()).toContain(mailbox);
    expect(await owner.poll(mailbox, 0)).toHaveLength(1);
  });

  it("a deposit to a mailbox nobody has claimed yet is retried later, not rejected", async () => {
    const sender = mkReal();
    const mailbox = toHex(rand(16));
    expect(await sender.perform({ call: "deposit", mailbox, bytes: envelope() })).toEqual({ result: "err", error: { kind: "unavailable" } });
  });

  it("key material and invites round-trip; a stranger cannot overwrite or revoke", async () => {
    const owner = mkReal();
    const other = mkReal();
    const dev = toHex(rand(16));
    const opk = toHex(rand(20));
    expect(await owner.perform({ call: "fetch_key_material", deviceId: dev })).toEqual({ result: "err", error: { kind: "not_found" } });
    expect(await owner.perform({ call: "publish_key_material", deviceId: dev, bundle: toHex(rand(60)), opks: [opk], pqopks: [] })).toEqual({ result: "ok" });
    expect(await other.perform({ call: "publish_key_material", deviceId: dev, bundle: toHex(rand(60)), opks: [], pqopks: [] })).toEqual({ result: "err", error: { kind: "rejected", reason: "unauthorized" } });
    const fetched = await other.perform({ call: "fetch_key_material", deviceId: dev });
    expect(fetched.result === "many" && fetched.items).toHaveLength(2);

    const id = toHex(rand(16));
    const descriptor = toHex(rand(200));
    expect(await owner.perform({ call: "put_invite", inviteId: id, descriptor, expiresAt: Date.now() + 3_600_000 })).toEqual({ result: "ok" });
    expect(await other.perform({ call: "get_invite", inviteId: id })).toEqual({ result: "bytes", bytes: descriptor });
    expect(await other.perform({ call: "revoke_invite", inviteId: id })).toEqual({ result: "err", error: { kind: "rejected", reason: "unauthorized" } });
    expect(await owner.perform({ call: "revoke_invite", inviteId: id })).toEqual({ result: "ok" });
    expect(await other.perform({ call: "get_invite", inviteId: id })).toEqual({ result: "err", error: { kind: "not_found" } });
  });

  it("the route object (RelayEntry) and its sealed-delivery binding follow the spec shape", async () => {
    const p = mkReal();
    const mailbox = rand(16);
    const entry = await p.routeEntry(mailbox, 1_900_000_000);
    expect(entry.relayUri).toBe(`${home.url}/`);
    expect(entry.capability).toHaveLength(32);
    expect(toHex((await routeBinding(entry.relayUri, entry.capability)).slice(0, 4))).toHaveLength(8);
    const caps = await p.capabilities();
    expect(caps?.["provider_kinds"]).toContain("relay");
    expect(decode(new Uint8Array([0xf6]))).toBeNull();
  });
});

function encodeMap(e: Map<number, unknown>): Uint8Array {
  return encode(map([...e.entries()].map(([k, v]) => [k, v as CborValue])));
}
