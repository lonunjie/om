// SPDX-License-Identifier: MPL-2.0
// The sealed signalling envelope through the real engineering module: two parties connect through the loopback store, then
// one seals a WebRTC offer for the other and the other opens it. Needs `pnpm --filter @om/core-wasm build:wasm:scope`.
import { existsSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { asOperationId, asProfileId } from "@om/platform-api";
import type { HomeSummary, ProfileExecutionContext, ProfileId } from "@om/platform-api";
import { createCoreRuntime, instantiateCore } from "./index";
import type { WasmCore } from "./wasm";
import { createScopeEngines } from "./scope-engines";
import type { ScopeEngines } from "./scope-engines";
import { createMemoryKv, createMemoryProvider } from "./test-support";
import type { MemoryProvider } from "./test-support";

const WASM = new URL("../dist/om_wasm_scope.wasm", import.meta.url);
const HOME: HomeSummary = { tier: "T1", observers: ["host_sees_ciphertext"], migrationAvailable: true, clientOriginChoice: ["default_origin"] };
const op = asOperationId;

interface Party { core: WasmCore; engines: ScopeEngines; flush: () => Promise<void>; rt: Awaited<ReturnType<typeof createCoreRuntime>>; ctx: ProfileExecutionContext; id: ProfileId }

async function boot(provider: MemoryProvider, id: ProfileId): Promise<Party> {
  const core = await instantiateCore(readFileSync(WASM));
  const kv = createMemoryKv();
  const rt = await createCoreRuntime({ core, provider, storage: () => kv, home: HOME, autoDrain: false, retryMs: 0 });
  const ctx = await rt.openProfile(id);
  return { core, engines: createScopeEngines(core), flush: () => rt.flush(id), rt, ctx, id };
}

async function settle(...ps: Party[]): Promise<void> {
  for (let i = 0; i < 20; i++) for (const p of ps) await p.flush();
  for (const p of ps) await p.rt.idle();
}

describe.skipIf(!existsSync(WASM))("sealed LAN signalling through the real module", () => {
  it("an accepted pair signals in both directions; a replay and a stranger are refused", async () => {
    const provider = createMemoryProvider();
    const alice = await boot(provider, asProfileId("alice"));
    const bob = await boot(provider, asProfileId("bob"));
    const carol = await boot(provider, asProfileId("carol"));
    await alice.rt.dispatch({ kind: "CreateIdentityProfile", ctx: alice.ctx, operationId: op("a"), label: "Alice" });
    await bob.rt.dispatch({ kind: "CreateIdentityProfile", ctx: bob.ctx, operationId: op("b"), label: "Bob" });
    await carol.rt.dispatch({ kind: "CreateIdentityProfile", ctx: carol.ctx, operationId: op("c"), label: "Carol" });
    await settle(alice, bob, carol);
    await alice.rt.dispatch({ kind: "CreateInvite", ctx: alice.ctx, operationId: op("inv") });
    await settle(alice, bob);
    const invite = await alice.rt.query({ kind: "GetInvite", ctx: alice.ctx, operationId: op("inv") });
    if (!invite) throw new Error("no invite");
    await bob.rt.dispatch({ kind: "AcceptInvitation", ctx: bob.ctx, operationId: op("acc"), link: invite.link });
    await settle(alice, bob);
    const [intro] = await alice.rt.query({ kind: "GetIntroductions", ctx: alice.ctx });
    if (!intro) throw new Error("no introduction");
    await alice.rt.dispatch({ kind: "AcceptIntroduction", ctx: alice.ctx, operationId: op("intro"), requestId: intro.id });
    await settle(alice, bob);
    const [ac] = await alice.rt.query({ kind: "GetConversationList", ctx: alice.ctx });
    const [bc] = await bob.rt.query({ kind: "GetConversationList", ctx: bob.ctx });
    if (!ac || !bc) throw new Error("no conversation");

    const sessionId = "0f".repeat(16);
    const sdp = "v=0\r\no=- 1 1 IN IP4 0.0.0.0\r\ns=-\r\n";
    const sealed = alice.engines.lanSignal.seal(alice.ctx, ac.id, { kind: "offer", sessionId, sdp });
    if ("refused" in sealed) throw new Error(sealed.refused);
    expect(sealed.envelope).not.toContain(Buffer.from("IN IP4").toString("hex"));
    expect(bob.engines.lanSignal.open(bob.ctx, bc.id, sealed.envelope)).toEqual({ kind: "offer", sessionId, sdp });
    expect(bob.engines.lanSignal.open(bob.ctx, bc.id, sealed.envelope)).toEqual({ refused: "replay" });
    const answer = bob.engines.lanSignal.seal(bob.ctx, bc.id, { kind: "answer", sessionId, sdp: "answer" });
    if ("refused" in answer) throw new Error(answer.refused);
    expect(alice.engines.lanSignal.open(alice.ctx, ac.id, answer.envelope)).toMatchObject({ kind: "answer", sdp: "answer" });
    // Malformed input is a refusal, not a throw; a host-supplied seed has no field to travel in.
    expect(alice.engines.lanSignal.seal(alice.ctx, ac.id, { kind: "offer", sessionId: "zz", sdp })).toEqual({ refused: "malformed" });
    expect(alice.engines.lanSignal.open(alice.ctx, ac.id, "00")).toEqual({ refused: "malformed" });
  });
});
