// SPDX-License-Identifier: MPL-2.0
// Real Core, one adapter, two profiles, and a poll timer of a full minute: with the wake channel the whole introduction and a
// message still complete in seconds, and the Home is asked far less than a 200 ms timer would ask. Without the channel the
// same timer needs minutes (the control case below uses a Home that does not offer it, at the same timer, and a short budget).
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
let off: Home;
beforeAll(async () => { home = await startHome(); off = await startHome({ vars: { OM_WAKE: "0" } }); }, 120_000);
afterAll(async () => { await home.dispose(); await off.dispose(); });
const urlOf = (i: RequestInfo | URL): string => (typeof i === "string" ? i : i instanceof URL ? i.href : i.url);
const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));

async function conversation(h: Home, pollMs: number, budgetMs: number): Promise<{ delivered: boolean; ms: number; requests: number; wakes: number }> {
  const config = parseProviderUrl(h.url, { allowLoopbackHttp: true });
  if (!config) throw new Error("url");
  let requests = 0;
  let published = false;
  const t0 = Date.now();
  const provider = createHttpProvider({ config, secrets: createMemorySecretStore(), fetch: async (i, init) => { requests++; const r = await fetch(i, init); if (init?.method === "PUT" && urlOf(i).includes("/descriptors/") && r.ok) published = true; if (process.env["WAKE_DEBUG"]) process.stderr.write(`${String(Date.now() - t0)}ms ${init?.method ?? "GET"} ${urlOf(i).replace(h.url, "").slice(0, 40)} -> ${String(r.status)}\n`); return r; } });
  provider.setWakeActive(true);
  const kvs = new Map([["alice", createMemoryKv()], ["bob", createMemoryKv()]]);
  const core = await instantiateCore(readFileSync(WASM));
  const rt = await createCoreRuntime({ core, provider, storage: (id) => kvs.get(id) ?? createMemoryKv(), home: HOME, pollMs, retryMs: 300 });
  const stage = (n: string): void => { if (process.env["WAKE_DEBUG"]) process.stderr.write(`${String(Date.now() - t0)}ms ${n} requests=${String(requests)} ${JSON.stringify(provider.wakeStats())}\n`); };
  const left = (): number => budgetMs - (Date.now() - t0);
  const wait = async <T>(f: () => Promise<T | undefined>): Promise<T | undefined> => { for (; left() > 0;) { const v = await f(); if (v !== undefined) return v; await sleep(100); } return undefined; };
  try {
    const a = await rt.openProfile(asProfileId("alice"));
    const b = await rt.openProfile(asProfileId("bob"));
    await rt.dispatch({ kind: "CreateIdentityProfile", ctx: a, operationId: op("a-create"), label: "Alice" });
    await rt.dispatch({ kind: "CreateIdentityProfile", ctx: b, operationId: op("b-create"), label: "Bob" });
    await rt.dispatch({ kind: "CreateInvite", ctx: a, operationId: op("a-invite") });
    stage("created");
    await wait(() => Promise.resolve(published ? true : undefined)); // the invitation is usable once its descriptor is at the Home
    const invite = await wait(() => rt.query({ kind: "GetInvite", ctx: a, operationId: op("a-invite") }));
    if (!invite) return { delivered: false, ms: Date.now() - t0, requests, wakes: provider.wakeStats().wakes };
    stage("invite");
    await rt.dispatch({ kind: "AcceptInvitation", ctx: b, operationId: op("b-accept"), link: invite.link });
    const intro = await wait(async () => (await rt.query({ kind: "GetIntroductions", ctx: a }))[0]);
    stage("intro?" + String(intro !== undefined));
    if (!intro) return { delivered: false, ms: Date.now() - t0, requests, wakes: provider.wakeStats().wakes };
    stage("intro");
    await rt.dispatch({ kind: "AcceptIntroduction", ctx: a, operationId: op("a-intro"), requestId: intro.id });
    const conv = await wait(async () => (await rt.query({ kind: "GetConversationList", ctx: a }))[0]);
    if (!conv) return { delivered: false, ms: Date.now() - t0, requests, wakes: provider.wakeStats().wakes };
    stage("conversation");
    for (let i = 0; i < 60 && left() > 0; i++) {
      await rt.dispatch({ kind: "SendMessage", ctx: a, operationId: op(`m${String(i)}`), conversationId: asConversationId(conv.id), draft: { text: "hello bob" } });
      if ((await rt.pendingOperations(a)).find((p) => p.operationId === `m${String(i)}`)?.outcome.kind !== "Rejected") break;
      await sleep(250);
    }
    const got = await wait(async () => {
      const [bc] = await rt.query({ kind: "GetConversationList", ctx: b });
      if (!bc) return undefined;
      const t = (await rt.query({ kind: "GetConversationView", ctx: b, conversationId: asConversationId(bc.id) })).timeline.map((x) => x.text);
      return t.length > 0 ? t : undefined;
    });
    return { delivered: got?.[0] === "hello bob", ms: Date.now() - t0, requests, wakes: provider.wakeStats().wakes };
  } finally {
    rt.dispose();
    provider.dispose();
  }
}

describe.skipIf(!existsSync(WASM))("wake channel with a one-minute timer", () => {
  it("completes invite, introduction and a message in seconds, driven by wakes", async () => {
    const r = await conversation(home, 60_000, 40_000);
    expect(r.delivered).toBe(true);
    expect(r.wakes).toBeGreaterThan(0);
    expect(r.ms).toBeLessThan(30_000);
  }, 60_000);

  it("control: a Home without the channel, same one-minute timer, does not get there in the same budget", async () => {
    const r = await conversation(off, 60_000, 12_000);
    expect(r.wakes).toBe(0);
    expect(r.delivered).toBe(false);
  }, 30_000);
});
