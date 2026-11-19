// @vitest-environment jsdom
import { cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { CallView, ProfileExecutionContext } from "@om/platform-api";
import type { CallDriver } from "@om/core-wasm";
import type { ConversationServices } from "@om/ui";
import { sessionBusy } from "../sw/busy";
import { useCalls } from "./useCalls";

afterEach(cleanup);

const view = (state: CallView["state"]): CallView => ({
  callSessionId: "call-1", conversationId: "c1" as CallView["conversationId"], media: "audio", state, outgoing: false, ringDeadlineMs: null, reason: null,
  localMediaFlags: 0, peerMediaFlags: 0, route: null, deliveryUnsure: false, restartAttempt: 0, wakeAtMs: null,
});

/** A Core client whose call view is whatever the test says; nothing else in the call path is exercised. */
function setup(current: CallView | null) {
  const client = {
    subscribe: () => () => undefined,
    query: (q: { kind: string }) => Promise.resolve(q.kind === "GetCallView" ? current : q.kind === "GetPrivacyLevel" ? "recommended" : []),
  };
  const driver = { onChange: () => () => undefined } as unknown as CallDriver;
  const services = { client, ctx: {} as ProfileExecutionContext, registry: {} } as unknown as ConversationServices;
  return { services, driver };
}

describe("a staged update and a call (sessionBusy)", () => {
  it("is not busy with no call, and busy while a call rings, connects or is live", async () => {
    const idle = setup(null);
    const a = renderHook(() => useCalls(idle.services, idle.driver));
    await new Promise((r) => setTimeout(r, 20));
    expect(sessionBusy()).toBe(false);
    a.unmount();
    for (const state of ["incoming", "ringing", "connecting", "active", "reconnecting"] as const) {
      const live = setup(view(state));
      const h = renderHook(() => useCalls(live.services, live.driver));
      await waitFor(() => { expect(h.result.current?.view?.state).toBe(state); });
      expect(sessionBusy(), state).toBe(true);
      h.unmount();
      expect(sessionBusy(), `${state} after unmount`).toBe(false);
    }
  });

  it("is not busy once the call has ended", async () => {
    const done = setup(view("ended"));
    const h = renderHook(() => useCalls(done.services, done.driver));
    await new Promise((r) => setTimeout(r, 20));
    expect(sessionBusy()).toBe(false);
    h.unmount();
  });
});
