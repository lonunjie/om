import { describe, expect, it } from "vitest";
import { IDBFactory } from "fake-indexeddb";
import { asConversationId, asMessageId, asProfileId } from "@om/platform-api";
import { createEncryptedKv, createWebDeviceVault } from "@om/web-platform";
import type { EncryptedKv, KvGetResult, KvListResult, KvResult, VaultErrorReason } from "@om/platform-api";
import { LocalSearchIndex, type StoredConversation } from "./LocalSearchIndex";
import { createSearchStore } from "./store";

const dec = new TextDecoder();
interface FakeKv extends EncryptedKv { readonly data: Map<string, Uint8Array>; locked: boolean; failGet: Map<string, VaultErrorReason>; puts: string[]; failPutAt: number }
function fakeKv(): FakeKv {
  const data = new Map<string, Uint8Array>();
  const kv: FakeKv = {
    data, locked: false, failGet: new Map(), puts: [], failPutAt: -1,
    put(k, v): Promise<KvResult> { if (kv.locked) return Promise.resolve({ ok: false, reason: "locked" }); kv.puts.push(k); if (kv.puts.length === kv.failPutAt) return Promise.resolve({ ok: false, reason: "storage_failed" }); data.set(k, v); return Promise.resolve({ ok: true }); },
    get(k): Promise<KvGetResult> {
      if (kv.locked) return Promise.resolve({ ok: false, reason: "locked" });
      const f = kv.failGet.get(k);
      if (f !== undefined) return Promise.resolve({ ok: false, reason: f });
      return Promise.resolve({ ok: true, value: data.get(k) });
    },
    delete(k): Promise<KvResult> { data.delete(k); return Promise.resolve({ ok: true }); },
    list(): Promise<KvListResult> { return kv.locked ? Promise.resolve({ ok: false, reason: "locked" }) : Promise.resolve({ ok: true, keys: [...data.keys()] }); },
  };
  return kv;
}

const C1 = asConversationId("c1");
const C2 = asConversationId("room/2");
const rows = (from: number, to: number, word = "msg") => Array.from({ length: to - from }, (_, i) => ({ messageId: asMessageId(`m${String(from + i)}`), sentAt: (from + i) * 1000, text: `${word} ${String(from + i)}`, files: [] as string[] }));
const conv = (id: typeof C1, first: number, r: ReturnType<typeof rows>, name = "Sam"): StoredConversation => ({ id, name, first, rows: r });

describe("SearchStore", () => {
  it("stores only ciphertext in the production vault and binds it to purpose and record key", async () => {
    const indexedDB = new IDBFactory();
    const profileId = asProfileId("search-identity");
    const vault = createWebDeviceVault({ crypto, indexedDB, rpId: "om.test", rpName: "OM", hasFocus: () => true,
      now: Date.now, sleep: () => Promise.resolve(),
    });
    expect(await vault.createProfileVault(profileId)).toMatchObject({ ok: true });
    const kv = createEncryptedKv(vault, { indexedDB }, profileId, "search-index");
    const word = "search content known only to this profile";
    const conversation = conv(C1, 0, rows(0, 2, word));
    const s = createSearchStore(kv);
    await s.load();
    await s.save(conversation);
    expect(await createSearchStore(kv).load()).toEqual([conversation]);

    const idb = <T>(request: IDBRequest<T>): Promise<T> => new Promise((resolve, reject) => {
      request.onsuccess = () => { resolve(request.result); };
      request.onerror = () => { reject(request.error ?? new Error("IndexedDB request failed")); };
    });
    const db = await idb(indexedDB.open(`om-vault-${profileId}`));
    try {
      const store = db.transaction("kv").objectStore("kv");
      const records = await idb(store.getAll()) as Uint8Array[];
      expect(records).toHaveLength(2);
      expect(records.every((record) => !dec.decode(record).includes(word))).toBe(true);
      const found = await kv.list();
      if (!found.ok) throw new Error(found.reason);
      const segment = found.keys.find((key) => key.includes("/s/"));
      if (segment === undefined) throw new Error("segment missing");
      const raw = await idb(db.transaction("kv").objectStore("kv").get(segment)) as Uint8Array;
      const wrongPurpose = createEncryptedKv(vault, { indexedDB }, profileId, "local-db");
      expect(await wrongPurpose.get(segment)).toEqual({ ok: false, reason: "wrong_purpose" });
      const otherKey = `${segment}-other`;
      await idb(db.transaction("kv", "readwrite").objectStore("kv").put(raw, otherKey));
      expect(await kv.get(otherKey)).toEqual({ ok: false, reason: "integrity_failure" });
    } finally { db.close(); }
  });

  it("round-trips across a restart under the search prefix", async () => {
    const kv = fakeKv();
    const a = createSearchStore(kv, { segment: 4 });
    await a.load();
    await a.save(conv(C1, 0, rows(0, 10, "needle")));
    await a.save(conv(C2, 5, rows(5, 7), "Room"));
    await a.idle();
    const b = createSearchStore(kv, { segment: 4 });
    const got = await b.load();
    expect(got.map((c) => c.id).sort()).toEqual([C1, C2].sort());
    const c1 = got.find((c) => c.id === C1);
    expect(c1?.rows).toHaveLength(10);
    expect(c1?.rows[3]).toMatchObject({ messageId: "m3", text: "needle 3" });
    expect(got.find((c) => c.id === C2)).toMatchObject({ name: "Room", first: 5 });
    expect([...kv.data.keys()].every((k) => k.startsWith("ui/search/v2/"))).toBe(true);
  });

  it("reopens the last committed rows when any segment or manifest write is interrupted", async () => {
    for (const failAt of [1, 2, 3]) {
      const kv = fakeKv();
      const before = conv(C1, 0, rows(0, 4, "original"));
      const first = createSearchStore(kv, { segment: 2 });
      await first.load();
      await first.save(before);
      kv.puts.length = 0;
      kv.failPutAt = failAt;
      await first.save(conv(C1, 0, rows(0, 4, "updated")));
      expect(await createSearchStore(kv, { segment: 2 }).load()).toEqual([before]);
      expect(kv.data.size).toBe(3);
    }
  });

  it("does not write a conversation whose text alone exceeds the budget", async () => {
    const kv = fakeKv();
    const s = createSearchStore(kv, { segment: 2, maxBytes: 40 });
    await s.load();
    await s.save(conv(C1, 0, rows(0, 4)));
    expect(kv.puts).toEqual([]);
  });

  it("appending a message rewrites only the newest segment and the manifest", async () => {
    const kv = fakeKv();
    const s = createSearchStore(kv, { segment: 4 });
    await s.load();
    await s.save(conv(C1, 0, rows(0, 10)));
    kv.puts.length = 0;
    await s.save(conv(C1, 0, rows(0, 11)));
    expect(kv.puts.filter((k) => k.includes("/s/"))).toHaveLength(1);
    expect(kv.puts.some((k) => k.includes("/m/"))).toBe(true);
    kv.puts.length = 0;
    await s.save(conv(C1, 0, rows(0, 11)));
    expect(kv.puts).toEqual([]); // nothing changed: nothing written
  });

  it("drops segments that fell out of the window and sweeps crash leftovers and orphans", async () => {
    const kv = fakeKv();
    const s = createSearchStore(kv, { segment: 4 });
    await s.load();
    await s.save(conv(C1, 0, rows(0, 12)));
    await s.save(conv(C1, 8, rows(8, 12)));
    expect([...kv.data.keys()].filter((k) => k.includes("/s/"))).toHaveLength(1);
    // a crashed write left a segment no manifest names, and a chat with no manifest
    kv.data.set("ui/search/v2/s/c1/99", new Uint8Array([91, 93]));
    kv.data.set("ui/search/v2/s/ghost/0", new Uint8Array([91, 93]));
    const got = await createSearchStore(kv, { segment: 4 }).load();
    expect(got).toHaveLength(1);
    expect([...kv.data.keys()].filter((k) => k.includes("/s/"))).toEqual([expect.stringMatching(/^ui\/search\/v2\/s\/c1\/2\/[a-f0-9]{64}$/)]);
  });

  it("evicts the least recently active chats over budget and does not write them back this visit", async () => {
    const kv = fakeKv();
    const one = JSON.stringify(rows(0, 20).map((r) => [r.messageId, r.sentAt, r.text, r.files])).length;
    const s = createSearchStore(kv, { segment: 64, maxBytes: Math.floor(one * 2.5) });
    await s.load();
    await s.save(conv(asConversationId("old"), 0, rows(0, 20)));
    await s.save(conv(asConversationId("mid"), 0, rows(100, 120)));
    await s.save(conv(asConversationId("new"), 0, rows(200, 220)));
    expect(s.bytes()).toBeLessThanOrEqual(Math.floor(one * 2.5));
    const left = (await createSearchStore(kv, { segment: 64 }).load()).map((c) => c.id).sort();
    expect(left).toEqual(["mid", "new"]);
    kv.puts.length = 0;
    await s.save(conv(asConversationId("old"), 0, rows(0, 20)));
    expect(kv.puts).toEqual([]);
  });

  it("purges an older format, leaves a newer one, and rebuilds from damaged records", async () => {
    const kv = fakeKv();
    const s = createSearchStore(kv, { segment: 4 });
    await s.load();
    await s.save(conv(C1, 0, rows(0, 6)));
    await s.save(conv(C2, 0, rows(0, 6)));
    kv.data.set("ui/search/v1/x", new Uint8Array([1]));
    kv.data.set("ui/search/v7/x", new Uint8Array([1]));
    const damaged = [...kv.data.keys()].find((k) => k.startsWith("ui/search/v2/s/c1/0/"));
    if (damaged === undefined) throw new Error("segment missing");
    kv.failGet.set(damaged, "integrity_failure");
    const got = await createSearchStore(kv, { segment: 4 }).load();
    expect(got.map((c) => c.id)).toEqual([C2]);
    expect(kv.data.has("ui/search/v1/x")).toBe(false);
    expect(kv.data.has("ui/search/v7/x")).toBe(true);
    expect([...kv.data.keys()].some((k) => k.includes("/c1"))).toBe(false);
    // an unknown manifest payload is dropped too
    kv.data.set("ui/search/v2/m/c9", new TextEncoder().encode(JSON.stringify({ v: 2, name: "x" })));
    const again = await createSearchStore(kv, { segment: 4 }).load();
    expect(again.map((c) => c.id)).toEqual([C2]);
    expect(kv.data.has("ui/search/v2/m/c9")).toBe(false);
  });

  it("a locked vault loads nothing and deletes nothing; writes are skipped; the next unlock reads everything", async () => {
    const kv = fakeKv();
    const s = createSearchStore(kv, { segment: 4 });
    await s.load();
    await s.save(conv(C1, 0, rows(0, 6)));
    const n = kv.data.size;
    kv.locked = true;
    const locked = createSearchStore(kv, { segment: 4 });
    expect(await locked.load()).toEqual([]);
    await locked.save(conv(C1, 0, rows(0, 9)));
    await locked.remove(C1);
    expect(kv.data.size).toBe(n);
    kv.locked = false;
    expect((await createSearchStore(kv, { segment: 4 }).load())[0]?.rows).toHaveLength(6);
  });

  it("remove and wipe forget at rest", async () => {
    const kv = fakeKv();
    const s = createSearchStore(kv, { segment: 4 });
    await s.load();
    await s.save(conv(C1, 0, rows(0, 6)));
    await s.save(conv(C2, 0, rows(0, 6)));
    await s.remove(C1);
    expect([...kv.data.keys()].some((k) => k.endsWith("/c1") || k.includes("/c1/"))).toBe(false);
    await s.wipe();
    expect(kv.data.size).toBe(0);
    expect(dec.decode(new Uint8Array())).toBe("");
  });

  it("works with the index: snapshot, restore and bounded restore", async () => {
    const kv = fakeKv();
    const ix = new LocalSearchIndex();
    const items = rows(0, 8, "café").map((r) => ({ messageId: r.messageId, sentAt: r.sentAt, text: r.text }));
    expect(ix.replace(C1, "Sam", items)).toBe(true);
    expect(ix.replace(C1, "Sam", items)).toBe(false); // unchanged
    const s = createSearchStore(kv, { segment: 4 });
    await s.load();
    const snap = ix.snapshot(C1);
    if (snap === undefined) throw new Error("no snapshot");
    await s.save(snap);
    const fresh = new LocalSearchIndex({ perConversation: 3, conversations: 5 });
    for (const c of await createSearchStore(kv, { segment: 4 }).load()) fresh.restore(c);
    expect(fresh.search("CAFE").map((h) => h.messageId)).toEqual(["m7", "m6", "m5"]);
    expect(fresh.snapshot(C1)?.first).toBe(5);
  });
});
