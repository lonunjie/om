import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ConversationId, EncryptedKv, ProfileId } from "@om/platform-api";
import { createDraftStore } from "./draftStore";

const alice = "p-alice" as ProfileId;
const bob = "p-bob" as ProfileId;
const chat = (n: string): ConversationId => n as ConversationId;

/** An in-memory EncryptedKv; `locked` makes every call fail like a vault that has been locked. */
function fakeKv() {
  const data = new Map<string, Uint8Array>();
  const state = { locked: false };
  const kv: EncryptedKv = {
    put: (k, v) => { if (state.locked) return Promise.resolve({ ok: false, reason: "locked" }); data.set(k, v); return Promise.resolve({ ok: true }); },
    get: (k) => Promise.resolve(state.locked ? { ok: false, reason: "locked" } : { ok: true, value: data.get(k) }),
    delete: (k) => { if (state.locked) return Promise.resolve({ ok: false, reason: "locked" }); data.delete(k); return Promise.resolve({ ok: true }); },
    list: () => Promise.resolve(state.locked ? { ok: false, reason: "locked" } : { ok: true, keys: [...data.keys()] }),
  };
  return { kv, data, state };
}

beforeEach(() => { vi.useFakeTimers(); });
afterEach(() => { vi.useRealTimers(); });

describe("draft store", () => {
  it("saves a draft shortly after typing stops, sealed under a key of its own, and restores it for a new session", async () => {
    const { kv, data } = fakeKv();
    const first = createDraftStore(() => kv);
    first.set(alice, chat("c1"), "see you at");
    first.set(alice, chat("c1"), "see you at six");
    expect(data.size).toBe(0);
    await vi.advanceTimersByTimeAsync(700);
    expect([...data.keys()]).toEqual(["ui/draft/c1"]);
    expect(new TextDecoder().decode(data.get("ui/draft/c1"))).toBe("see you at six");

    const second = createDraftStore(() => kv); // a reload
    expect((await second.load(alice)).get(chat("c1"))).toBe("see you at six");
  });

  it("a send (empty text) removes the saved draft", async () => {
    const { kv, data } = fakeKv();
    const s = createDraftStore(() => kv);
    s.set(alice, chat("c1"), "hello");
    await s.flush();
    expect(data.size).toBe(1);
    s.set(alice, chat("c1"), "");
    await s.flush();
    expect(data.size).toBe(0);
    expect((await createDraftStore(() => kv).load(alice)).size).toBe(0);
  });

  it("flush before locking keeps the last keystrokes; forget drops them from memory; unlock brings them back", async () => {
    const { kv, state } = fakeKv();
    const s = createDraftStore(() => kv);
    s.set(alice, chat("c1"), "half a thought");
    await s.flush(); // what Lock now does before the vault locks
    state.locked = true;
    s.forget(alice);
    state.locked = false; // unlocked again
    expect((await s.load(alice)).get(chat("c1"))).toBe("half a thought");
  });

  it("text typed while the vault is locked stays pending and is written once it opens", async () => {
    const { kv, data, state } = fakeKv();
    const s = createDraftStore(() => kv);
    state.locked = true;
    s.set(alice, chat("c1"), "later");
    await s.flush();
    expect(data.size).toBe(0);
    state.locked = false;
    await s.flush();
    expect(new TextDecoder().decode(data.get("ui/draft/c1"))).toBe("later");
  });

  it("keeps identities apart and prefers what was typed since opening over what was saved", async () => {
    const a = fakeKv();
    const b = fakeKv();
    const s = createDraftStore((p) => (p === alice ? a.kv : b.kv));
    a.data.set("ui/draft/c1", new TextEncoder().encode("old"));
    a.data.set("core/not-a-draft", new Uint8Array([1]));
    s.set(alice, chat("c1"), "new");
    expect((await s.load(alice)).get(chat("c1"))).toBe("new");
    expect((await s.load(bob)).size).toBe(0);
    await s.flush();
    expect(b.data.size).toBe(0);
    expect(a.data.has("core/not-a-draft")).toBe(true);
  });

  it("ignores a stored value that is not text", async () => {
    const { kv, data } = fakeKv();
    data.set("ui/draft/c1", new Uint8Array([0xff, 0xfe, 0xfd]));
    expect((await createDraftStore(() => kv).load(alice)).size).toBe(0);
  });
});
