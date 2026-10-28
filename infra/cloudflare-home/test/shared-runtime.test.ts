// SPDX-License-Identifier: MPL-2.0
// The browser shape: ONE Core instance and ONE adapter (one install) serving two profiles, with background polling.
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { asConversationId, asOperationId, asProfileId } from "@om/platform-api";
import type { HomeSummary } from "@om/platform-api";
import { createCoreRuntime, instantiateCore } from "@om/core-wasm";
import { createMemoryKv } from "@om/core-wasm/test-support";
import { createHttpProvider, createMemorySecretStore, parseProviderUrl } from "@om/provider-http";
import { startHome, type Home } from "./harness";

const WASM = join(import.meta.dirname, "../../../packages/om-core-wasm/dist/om_wasm.wasm");
const HOME: HomeSummary = { tier: "T1", observers: ["host_sees_ciphertext"], migrationAvailable: true, clientOriginChoice: ["default_origin"] };
const op = asOperationId;
let home: Home;
beforeAll(async () => { home = await startHome(); }, 120_000);
afterAll(async () => { await home.dispose(); });
const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));

describe.skipIf(!existsSync(WASM))("one Core instance, one adapter, two profiles, background polling", () => {
  it("delivers a message from one profile to the other", async () => {
    const config = parseProviderUrl(home.url, { allowLoopbackHttp: true });
    if (!config) throw new Error("url");
    const provider = createHttpProvider({ config, secrets: createMemorySecretStore() });
    const kvs = new Map([["alice", createMemoryKv()], ["bob", createMemoryKv()]]);
    const core = await instantiateCore(readFileSync(WASM));
    const rt = await createCoreRuntime({ core, provider, storage: (id) => kvs.get(id) ?? createMemoryKv(), home: HOME, pollMs: 200, retryMs: 300 });
    const a = await rt.openProfile(asProfileId("alice"));
    const b = await rt.openProfile(asProfileId("bob"));
    await rt.dispatch({ kind: "CreateIdentityProfile", ctx: a, operationId: op("a-create"), label: "Alice" });
    await rt.dispatch({ kind: "CreateIdentityProfile", ctx: b, operationId: op("b-create"), label: "Bob" });
    await rt.dispatch({ kind: "CreateInvite", ctx: a, operationId: op("a-invite") });
    await sleep(800);
    const invite = await rt.query({ kind: "GetInvite", ctx: a, operationId: op("a-invite") });
    if (!invite) throw new Error("no invite");
    await rt.dispatch({ kind: "AcceptInvitation", ctx: b, operationId: op("b-accept"), link: invite.link });
    let intro;
    for (let i = 0; i < 50 && !intro; i++) { await sleep(200); [intro] = await rt.query({ kind: "GetIntroductions", ctx: a }); }
    if (!intro) throw new Error("no introduction");
    await rt.dispatch({ kind: "AcceptIntroduction", ctx: a, operationId: op("a-intro"), requestId: intro.id });
    let conv;
    for (let i = 0; i < 50 && !conv; i++) { await sleep(200); [conv] = await rt.query({ kind: "GetConversationList", ctx: a }); }
    if (!conv) throw new Error("no conversation");
    // Like a user retrying: the first message can only be queued once the introduction has settled.
    for (let i = 0; i < 40; i++) {
      await rt.dispatch({ kind: "SendMessage", ctx: a, operationId: op(`m${String(i)}`), conversationId: asConversationId(conv.id), draft: { text: "hello bob" } });
      const o = (await rt.pendingOperations(a)).find((p) => p.operationId === `m${String(i)}`)?.outcome;
      if (o?.kind !== "Rejected") break;
      await sleep(250);
    }
    let got: string[] = [];
    for (let i = 0; i < 100 && got.length === 0; i++) {
      await sleep(200);
      const [bc] = await rt.query({ kind: "GetConversationList", ctx: b });
      if (bc) got = (await rt.query({ kind: "GetConversationView", ctx: b, conversationId: asConversationId(bc.id) })).timeline.map((t) => t.text);
    }
    expect(got).toEqual(["hello bob"]);
    rt.dispose();
  }, 60_000);
});
