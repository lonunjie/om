// @vitest-environment jsdom
// The persisted, encrypted-at-rest search index (ADR-0208) through the hook: survives a reload, covers groups, reconciles, stays per identity.
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { asConversationId, asGroupId, asOperationId, asPersonId, asProfileId } from "@om/platform-api";
import type { EncryptedKv, KvGetResult, KvListResult, KvResult } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import type { ConversationServices } from "../conversations/services";
import { createSearchStore } from "./store";
import { useLocalSearch } from "./useLocalSearch";

afterEach(() => { cleanup(); vi.useRealTimers(); });

const ME = asProfileId("me");
const ANA = asConversationId("c-ana");

function memoryKv(): EncryptedKv & { readonly data: Map<string, Uint8Array> } {
  const data = new Map<string, Uint8Array>();
  return {
    data,
    put: (k, v): Promise<KvResult> => { data.set(k, v); return Promise.resolve({ ok: true }); },
    get: (k): Promise<KvGetResult> => Promise.resolve({ ok: true, value: data.get(k) }),
    delete: (k): Promise<KvResult> => { data.delete(k); return Promise.resolve({ ok: true }); },
    list: (): Promise<KvListResult> => Promise.resolve({ ok: true, keys: [...data.keys()] }),
  };
}

function world(chat = true) {
  const h = createHarness({ env: {}, clock: () => 1000 });
  h.seedProfile({ profileId: ME, label: "Me", conversations: chat ? [{ id: ANA, personId: asPersonId("p-ana"), displayName: "Ana", messages: [{ text: "lunch at the café", direction: "incoming" }] }] : [] });
  let n = 0;
  const registry = createActionRegistry({ newOperationId: () => asOperationId(`op-${String(++n)}`) });
  return { h, services: (over: Partial<ConversationServices["client"]> = {}): ConversationServices => ({ client: Object.assign(Object.create(h) as object, over) as ConversationServices["client"], registry, ctx: h.contextFor(ME) }) };
}

async function makeGroup(h: ReturnType<typeof world>["h"], ctx: ConversationServices["ctx"], text: string): Promise<ReturnType<typeof asGroupId>> {
  const id = h.groups.simulateInvite(ctx, ANA, "Book club");
  await h.groups.do(ctx, { kind: "accept", inviteId: id });
  const gid = h.groups.list(ctx)[0]?.conversationId ?? asGroupId("");
  h.groups.simulateMember(ctx, gid, "acct-c-ana");
  h.groups.simulateMessage(ctx, gid, "acct-c-ana", text);
  return gid;
}

describe("persisted search", () => {
  it("finds chat and group text after a reload before anything is re-read, and writes only ciphertext-bound keys", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "setInterval"] });
    const { h, services } = world();
    const kv = memoryKv();
    const first = (() => { const sv = services(); const st = createSearchStore(kv); return renderHook(() => useLocalSearch(sv, st)); })();
    await act(async () => { await vi.advanceTimersByTimeAsync(50); });
    await makeGroup(h, services().ctx, "quarterly budget review");
    await act(async () => { await vi.advanceTimersByTimeAsync(2000); });
    expect(first.result.current.search("budget")[0]).toMatchObject({ displayName: "Book club" });
    first.unmount();
    expect([...kv.data.keys()].every((k) => k.startsWith("ui/search/v1/"))).toBe(true);
    // A new page: Core answers nothing yet, so only the stored copy can answer.
    const never = new Promise<never>(() => undefined);
    const second = (() => { const sv = services({ query: () => never as never, subscribe: () => () => undefined }); const st = createSearchStore(kv); return renderHook(() => useLocalSearch(sv, st)); })();
    await act(async () => { await vi.advanceTimersByTimeAsync(50); });
    expect(second.result.current.search("CAFE").map((x) => x.displayName)).toEqual(["Ana"]);
    expect(second.result.current.search("quarterly")).toHaveLength(1);
    second.unmount();
  });

  it("forgets what vanished while the app was closed, once the lists are known", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "setInterval"] });
    const a = world();
    const kv = memoryKv();
    const first = (() => { const sv = a.services(); const st = createSearchStore(kv); return renderHook(() => useLocalSearch(sv, st)); })();
    await act(async () => { await vi.advanceTimersByTimeAsync(2000); });
    expect(first.result.current.search("lunch")).toHaveLength(1);
    first.unmount();
    expect([...kv.data.keys()].some((k) => k.includes("c-ana"))).toBe(true);
    const b = world(false); // the chat is gone; the group module is loaded and empty
    const second = (() => { const sv = b.services(); const st = createSearchStore(kv); return renderHook(() => useLocalSearch(sv, st)); })();
    await act(async () => { await vi.advanceTimersByTimeAsync(2000); });
    expect(second.result.current.search("lunch")).toEqual([]);
    expect([...kv.data.keys()].some((k) => k.includes("c-ana"))).toBe(false);
    second.unmount();
  });

  it("keeps a stored group while the group module is not loaded", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "setInterval"] });
    const a = world();
    const kv = memoryKv();
    const first = (() => { const sv = a.services(); const st = createSearchStore(kv); return renderHook(() => useLocalSearch(sv, st)); })();
    await makeGroup(a.h, a.services().ctx, "keep me");
    await act(async () => { await vi.advanceTimersByTimeAsync(2000); });
    first.unmount();
    a.h.groups.setModuleLoaded(false);
    const second = (() => { const sv = a.services(); const st = createSearchStore(kv); return renderHook(() => useLocalSearch(sv, st)); })();
    await act(async () => { await vi.advanceTimersByTimeAsync(2000); });
    expect(second.result.current.search("keep me")).toHaveLength(1);
    second.unmount();
    expect([...kv.data.keys()].some((k) => k.includes("/m/"))).toBe(true);
  });

  it("a group message that arrives is searchable at once, without waiting for a timer", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "setInterval"] });
    const { h, services } = world();
    const s = services();
    const gid = await makeGroup(h, s.ctx, "first");
    const { result } = renderHook(() => useLocalSearch(s));
    await act(async () => { await vi.advanceTimersByTimeAsync(10); });
    expect(result.current.search("zebra")).toEqual([]);
    act(() => { h.groups.simulateMessage(s.ctx, gid, "acct-c-ana", "a zebra crossing"); });
    expect(result.current.search("zebra")).toHaveLength(1);
  });

  it("writes only after the batching delay and not for an unchanged re-read", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "setInterval"] });
    const { services } = world();
    const kv = memoryKv();
    const put = vi.spyOn(kv, "put");
    const { unmount } = (() => { const sv = services(); const st = createSearchStore(kv); return renderHook(() => useLocalSearch(sv, st)); })();
    await act(async () => { await vi.advanceTimersByTimeAsync(300); });
    expect(put).not.toHaveBeenCalled();
    await act(async () => { await vi.advanceTimersByTimeAsync(1500); });
    const writes = put.mock.calls.length;
    expect(writes).toBeGreaterThan(0);
    await act(async () => { await vi.advanceTimersByTimeAsync(5000); });
    expect(put.mock.calls.length).toBe(writes);
    unmount();
  });
});
