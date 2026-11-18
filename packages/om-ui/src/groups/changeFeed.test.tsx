// @vitest-environment jsdom
// ADR-0209: the group screens re-read when Core says something changed, not on a timer (D31 retired).
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { asConversationId, asOperationId, asPersonId, asProfileId } from "@om/platform-api";
import type { GroupApi } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import type { ConversationServices } from "../conversations/services";
import { useGroupChat, useGroups, useHeldGroupInvites } from "./hooks";

afterEach(() => { cleanup(); vi.useRealTimers(); });

const ME = asProfileId("me");
const ANA = asConversationId("c-ana");

function setup(strip = false) {
  const h = createHarness({ env: {}, clock: () => 1000 });
  h.seedProfile({ profileId: ME, label: "Me", conversations: [{ id: ANA, personId: asPersonId("p-ana"), displayName: "Ana", messages: [] }] });
  let n = 0;
  const registry = createActionRegistry({ newOperationId: () => asOperationId(`op-${String(++n)}`) });
  const base = h.groups as GroupApi;
  // A host with no feed: the same api minus `onChange`.
  const client = strip ? { ...h, groups: { ...base, onChange: undefined } } : h;
  const services = { client, registry, ctx: h.contextFor(ME) } as unknown as ConversationServices;
  return { h, services };
}

/** Counts every read of the group list, the cheapest sign of a screen re-reading. */
function countReads(h: ReturnType<typeof setup>["h"]): { readonly n: () => number } {
  const real = h.groups.list.bind(h.groups);
  let n = 0;
  h.groups.list = (ctx) => { n++; return real(ctx); };
  return { n: () => n };
}

describe("group change feed", () => {
  it("re-reads at once when Core reports a change, and never on a timer", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "setTimeout"] });
    const { h, services } = setup();
    const reads = countReads(h);
    const { result } = renderHook(() => useGroups(services));
    const first = reads.n();
    await act(async () => { await vi.advanceTimersByTimeAsync(60_000); });
    expect(reads.n()).toBe(first); // a minute of quiet: no read at all
    expect(result.current.invites).toHaveLength(0);
    act(() => { h.groups.simulateInvite(services.ctx, ANA, "Family"); });
    expect(result.current.invites).toHaveLength(1); // the change itself re-read the views
  });

  it("shows a new message in an open group without waiting", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "setTimeout"] });
    const { h, services } = setup();
    const id = h.groups.simulateInvite(services.ctx, ANA, "Family");
    await h.groups.do(services.ctx, { kind: "accept", inviteId: id });
    const gid = h.groups.list(services.ctx)[0]?.conversationId;
    if (gid === undefined) throw new Error("no group");
    const { result } = renderHook(() => useGroupChat(services, gid, 0));
    expect(result.current.messages).toHaveLength(0);
    act(() => { h.groups.simulateMember(services.ctx, gid, "acct-c-ana"); h.groups.simulateMessage(services.ctx, gid, "acct-c-ana", "hello"); });
    expect(result.current.messages.map((m) => m.text)).toEqual(["hello"]);
  });

  it("follows the held invitations too", () => {
    vi.useFakeTimers({ toFake: ["setInterval", "setTimeout"] });
    const { h, services } = setup();
    h.groups.setModuleLoaded(false);
    const { result } = renderHook(() => useHeldGroupInvites(services));
    expect(result.current.held.invitations).toHaveLength(0);
    act(() => { h.groups.simulateInvite(services.ctx, ANA, "Family"); });
    expect(result.current.held.invitations).toHaveLength(1);
  });

  it("stops listening when the screen goes away", () => {
    const { h, services } = setup();
    const off = vi.fn();
    const real = h.groups.onChange?.bind(h.groups);
    h.groups.onChange = (l) => { const u = real?.(l); return () => { off(); u?.(); }; };
    const { unmount } = renderHook(() => useGroups(services));
    unmount();
    expect(off).toHaveBeenCalled();
  });

  it("falls back to the calm interval on a host with no feed", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "setTimeout"] });
    const { h, services } = setup(true);
    const { result } = renderHook(() => useGroups(services));
    h.groups.simulateInvite(services.ctx, ANA, "Family");
    expect(result.current.invites).toHaveLength(0);
    await act(async () => { await vi.advanceTimersByTimeAsync(2600); });
    expect(result.current.invites).toHaveLength(1);
  });
});
