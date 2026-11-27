// SPDX-License-Identifier: MPL-2.0
// Test support for the sealed LAN signalling suites: two parties connected through the loopback store on the real
// engineering module (`pnpm --filter @om/core-wasm build:wasm:scope`).
import { readFileSync } from "node:fs";
import { asOperationId, asProfileId } from "@om/platform-api";
import type { HomeSummary, ProfileExecutionContext, ProfileId } from "@om/platform-api";
import { createCoreRuntime, instantiateCore } from "./index";
import type { WasmCore } from "./wasm";
import { createScopeEngines } from "./scope-engines";
import type { ScopeEngines } from "./scope-engines";
import { createMemoryKv, createMemoryProvider } from "./test-support";
import type { MemoryProvider } from "./test-support";

export const SCOPE_WASM = new URL("../dist/om_wasm_scope.wasm", import.meta.url);
const HOME: HomeSummary = { tier: "T1", observers: ["host_sees_ciphertext"], migrationAvailable: true, clientOriginChoice: ["default_origin"] };
const op = asOperationId;

export interface Party { core: WasmCore; engines: ScopeEngines; flush: () => Promise<void>; rt: Awaited<ReturnType<typeof createCoreRuntime>>; ctx: ProfileExecutionContext; id: ProfileId }

async function boot(provider: MemoryProvider, id: ProfileId): Promise<Party> {
  const core = await instantiateCore(readFileSync(SCOPE_WASM));
  const kv = createMemoryKv();
  const rt = await createCoreRuntime({ core, provider, storage: () => kv, home: HOME, autoDrain: false, retryMs: 0 });
  const ctx = await rt.openProfile(id);
  return { core, engines: createScopeEngines(core, ctx), flush: () => rt.flush(id), rt, ctx, id };
}

export async function settle(...ps: Party[]): Promise<void> {
  for (let i = 0; i < 20; i++) for (const p of ps) await p.flush();
  for (const p of ps) await p.rt.idle();
}

/** Alice invites, Bob accepts, Alice accepts the introduction: an active relationship, plus a stranger with none. */
export async function connectPair(): Promise<{ alice: Party; bob: Party; carol: Party; aConv: string; bConv: string }> {
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
  return { alice, bob, carol, aConv: ac.id, bConv: bc.id };
}
