// SPDX-License-Identifier: MPL-2.0
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, it } from "vitest";
import { asOperationId, asProfileId } from "@om/platform-api";
import { createCoreRuntime, instantiateCore } from "./index";
import { createMemoryKv, createMemoryProvider } from "./test-support";
import type { HomeSummary } from "@om/platform-api";

const wasm = join(import.meta.dirname, "../dist/om_wasm.wasm");
const home: HomeSummary = { tier: "T1", observers: [], migrationAvailable: true, clientOriginChoice: [] };
const op = asOperationId;

it("passes authenticated call descriptions and one-shot directional keys through the wasm driver", async () => {
  const provider = createMemoryProvider();
  async function boot(name: string) {
    const id = asProfileId(name);
    const kv = createMemoryKv();
    const core = await instantiateCore(readFileSync(wasm));
    const rt = await createCoreRuntime({ core, provider, storage: () => kv, home, calls: true, autoDrain: false });
    const ctx = await rt.openProfile(id);
    await rt.dispatch({ kind: "CreateIdentityProfile", ctx, operationId: op("create"), label: name });
    return { rt, ctx, id };
  }
  const a = await boot("Alice");
  const b = await boot("Bob");
  const settle = async (): Promise<void> => {
    for (let i = 0; i < 8; i++) { await a.rt.flush(a.id); await b.rt.flush(b.id); }
  };
  try {
    await a.rt.dispatch({ kind: "CreateInvite", ctx: a.ctx, operationId: op("invite") });
    await settle();
    const invite = await a.rt.query({ kind: "GetInvite", ctx: a.ctx, operationId: op("invite") });
    if (!invite) throw new Error("no invite");
    await b.rt.dispatch({ kind: "AcceptInvitation", ctx: b.ctx, operationId: op("accept"), link: invite.link });
    await settle();
    const [intro] = await a.rt.query({ kind: "GetIntroductions", ctx: a.ctx });
    if (!intro) throw new Error("no introduction");
    await a.rt.dispatch({ kind: "AcceptIntroduction", ctx: a.ctx, operationId: op("intro"), requestId: intro.id });
    await settle();
    const [contact] = await a.rt.query({ kind: "GetConversationList", ctx: a.ctx });
    if (!contact || !a.rt.calls || !b.rt.calls) throw new Error("no contact or call driver");

    const av = { kind: "GetCallView", ctx: a.ctx } as const;
    const bv = { kind: "GetCallView", ctx: b.ctx } as const;
    await a.rt.dispatch({ kind: "StartCall", ctx: a.ctx, operationId: op("call"), target: contact.personId, media: "audio", sdp: [1, 2] });
    await settle();
    const incoming = await b.rt.query(bv);
    if (incoming === null) throw new Error("no incoming call");
    const { callSessionId } = incoming;
    expect(incoming.state).toBe("incoming");
    expect(await a.rt.query(av)).toMatchObject({ state: "starting" });
    expect(b.rt.calls.description(b.ctx, callSessionId)).toEqual({ kind: "offer", seq: 0, sdp: [1, 2] });
    expect(await b.rt.calls.takeMediaKeys(b.ctx, callSessionId)).toBeNull();
    await b.rt.dispatch({ kind: "AcceptCall", ctx: b.ctx, operationId: op("answer"), callSessionId, sdp: [3, 4], mediaFlags: 1 });
    await settle();
    expect(a.rt.calls.description(a.ctx, callSessionId)).toEqual({ kind: "answer", seq: 0, sdp: [3, 4] });
    const caller = await a.rt.calls.takeMediaKeys(a.ctx, callSessionId);
    const callee = await b.rt.calls.takeMediaKeys(b.ctx, callSessionId);
    if (caller === null || callee === null) throw new Error("no media keys");
    try {
      expect(caller.generation).toBe(0);
      expect(caller.send.every((v, i) => v === callee.receive[i])).toBe(true);
      expect(callee.send.every((v, i) => v === caller.receive[i])).toBe(true);
      expect(caller.send.some((v, i) => v !== caller.receive[i])).toBe(true);
    } finally {
      caller.send.fill(0);
      caller.receive.fill(0);
      callee.send.fill(0);
      callee.receive.fill(0);
    }
    expect(await a.rt.calls.takeMediaKeys(a.ctx, callSessionId)).toBeNull();
    await a.rt.calls.mediaState(a.ctx, callSessionId, "active");
    await a.rt.calls.routeFacts(a.ctx, callSessionId, "relay");
    expect(await a.rt.query(av)).toMatchObject({ state: "active", route: "relay" });
    await a.rt.dispatch({ kind: "UpdateCallIce", ctx: a.ctx, operationId: op("ice"), callSessionId, sdp: [5, 6] });
    await settle();
    expect(b.rt.calls.description(b.ctx, callSessionId, true)).toEqual({ kind: "ice_update", seq: 1, sdp: [5, 6] });
    await a.rt.dispatch({ kind: "EndCall", ctx: a.ctx, operationId: op("hangup"), callSessionId });
    await settle();
    expect(await b.rt.query(bv)).toMatchObject({ state: "ended", reason: "hangup" });
  } finally {
    a.rt.dispose();
    b.rt.dispose();
  }
});
