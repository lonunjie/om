import { describe, expect, it } from "vitest";
import { asConversationId, asOperationId, asPersonId, asProfileId, captureProfileContext } from "@om/platform-api";
import type { CoreIntent, OperationId, OutcomeClass, ProfileExecutionContext } from "@om/platform-api";
import { HARNESS_MARKER, ProfileAccessError, assertNotProduction, createHarness } from "./harness";

const A = asProfileId("profile-a");
const B = asProfileId("profile-b");
const CA = asConversationId("conv-a1");
const CB = asConversationId("conv-b1");

function setup() {
  const h = createHarness({ clock: () => 1000 });
  const a = h.seedProfile({ profileId: A, label: "A", conversations: [{ id: CA, personId: asPersonId("pa"), displayName: "Ann", messages: [{ text: "hello", direction: "incoming" }] }] });
  const b = h.seedProfile({ profileId: B, label: "B", conversations: [{ id: CB, personId: asPersonId("pb"), displayName: "Bo" }] });
  return { h, a, b };
}
const send = (ctx: ProfileExecutionContext, op: string, conversationId = CA): CoreIntent =>
  ({ kind: "SendMessage", ctx, operationId: asOperationId(op), conversationId, draft: { text: "hi" } });

describe("harness guard", () => {
  it("exports the marker and refuses to run in a production build", () => {
    expect(HARNESS_MARKER).toBe("OM-DEV-HARNESS-NOT-FOR-PRODUCTION");
    expect(() => createHarness({ env: { PROD: true } })).toThrow(HARNESS_MARKER);
    expect(() => { assertNotProduction({ PROD: true }); }).toThrow();
    expect(() => { assertNotProduction({ PROD: false }); }).not.toThrow();
    expect(createHarness().marker).toBe(HARNESS_MARKER);
  });
});

describe("Core boundary via harness", () => {
  it("UI-01: a typed SendMessage commits and appears in the timeline", async () => {
    const { h, a } = setup();
    const handle = await h.dispatch(send(a, "op-1"));
    const seen: OutcomeClass[] = [];
    h.observe(a, handle.operationId, (o) => seen.push(o));
    expect(seen).toEqual([{ kind: "Committed" }]);
    const v = await h.query({ kind: "GetConversationView", ctx: a, conversationId: CA });
    expect(v.timeline.map((t) => t.direction)).toEqual(["incoming", "outgoing"]);
  });

  it("re-dispatching the same OperationId creates no second operation; reuse for another intent throws", async () => {
    const { h, a } = setup();
    await h.dispatch(send(a, "op-1"));
    await h.dispatch(send(a, "op-1"));
    expect(await h.pendingOperations(a)).toHaveLength(1);
    await expect(h.dispatch({ kind: "ClearAdaptiveNavigation", ctx: a, operationId: asOperationId("op-1") })).rejects.toThrow();
  });

  it("UI-12: timeout after send => PendingReconciliation, same id, never Rejected, no second operation", async () => {
    const { h, a } = setup();
    const op = asOperationId("op-t");
    h.injectNetworkTimeoutAfterSend(op);
    const handle = await h.dispatch(send(a, "op-t"));
    expect(handle.operationId).toBe(op);
    const seen: OutcomeClass[] = [];
    h.observe(a, op, (o) => seen.push(o));
    h.observe(a, op, (o) => seen.push(o)); // re-observing is not a new operation
    expect(seen).toEqual([{ kind: "PendingReconciliation" }, { kind: "PendingReconciliation" }]);
    const view = await h.query({ kind: "GetConversationView", ctx: a, conversationId: CA });
    const last = view.timeline.at(-1);
    expect(last).toMatchObject({ direction: "outgoing", delivery: { state: "pending_reconciliation" } });
    // retrying delivery while reconciling returns the original operation, no new one
    if (last === undefined) throw new Error("no message");
    const retry = await h.dispatch({ kind: "RetryDelivery", ctx: a, operationId: asOperationId("op-retry"), messageId: last.messageId });
    expect(retry.operationId).toBe(op);
    expect(await h.pendingOperations(a)).toHaveLength(1);
    // authenticated outcome arrives later
    h.resolveReconciliation(op, { kind: "Committed" });
    expect(seen.at(-1)).toEqual({ kind: "Committed" });
    expect(() => { h.resolveReconciliation(op, { kind: "Committed" }); }).toThrow();
  });

  it("UI-13: restart drops observers but pendingOperations reconstructs all states; nothing inferred failed", async () => {
    const { h, a } = setup();
    h.injectNetworkTimeoutAfterSend(asOperationId("op-p"));
    await h.dispatch(send(a, "op-p"));
    await h.dispatch(send(a, "op-ok"));
    await h.dispatch(send(a, "op-bad", asConversationId("missing")));
    let calls = 0;
    h.observe(a, asOperationId("op-p"), () => { calls++; });
    h.subscribe({ kind: "GetConversationList", ctx: a }, () => { calls++; });
    const before = calls;
    h.simulateRestart();
    const ctx2 = h.contextFor(A);
    const ops = await h.pendingOperations(ctx2);
    const byId = Object.fromEntries(ops.map((o) => [o.operationId, o.outcome.kind]));
    expect(byId).toEqual({ "op-p": "PendingReconciliation", "op-ok": "Committed", "op-bad": "Rejected" });
    h.resolveReconciliation(asOperationId("op-p"), { kind: "Committed" });
    expect(calls).toBe(before); // old observers/subscribers were dropped
    expect((await h.pendingOperations(ctx2)).find((o) => o.operationId === ("op-p" as OperationId))?.outcome).toEqual({ kind: "Committed" });
  });

  it("resolveReconciliation can end rejected with a provider-neutral reason", async () => {
    const { h, a } = setup();
    h.injectNetworkTimeoutAfterSend(asOperationId("op-r"));
    await h.dispatch(send(a, "op-r"));
    h.resolveReconciliation(asOperationId("op-r"), { kind: "Rejected", reason: "unavailable" });
    const v = await h.query({ kind: "GetConversationView", ctx: a, conversationId: CA });
    expect(v.timeline.at(-1)).toMatchObject({ delivery: { state: "rejected", reason: "unavailable" } });
  });

  it("UI-06 data layer: profiles are namespaced and cross-profile access throws", async () => {
    const { h, a, b } = setup();
    expect((await h.query({ kind: "GetConversationList", ctx: a })).map((c) => c.id)).toEqual([CA]);
    expect((await h.query({ kind: "GetConversationList", ctx: b })).map((c) => c.id)).toEqual([CB]);
    await expect(h.query({ kind: "GetConversationView", ctx: a, conversationId: CB })).rejects.toThrow(ProfileAccessError);
    await expect(h.dispatch(send(a, "op-x", CB))).rejects.toThrow(ProfileAccessError);
    h.injectNetworkTimeoutAfterSend(asOperationId("op-b"));
    await h.dispatch(send(b, "op-b", CB));
    expect(() => h.observe(a, asOperationId("op-b"), () => undefined)).toThrow(ProfileAccessError);
    expect(await h.pendingOperations(a)).toEqual([]);
  });

  it("UI-03: forged, unfrozen or stale contexts fail closed", async () => {
    const { h, a } = setup();
    const unfrozen = { profileId: a.profileId, generation: a.generation };
    await expect(h.dispatch(send(unfrozen, "op-u"))).rejects.toThrow(ProfileAccessError);
    const stale = captureProfileContext(A, "gen-99" as typeof a.generation);
    await expect(h.query({ kind: "GetConversationList", ctx: stale })).rejects.toThrow(ProfileAccessError);
    const unknown = captureProfileContext(asProfileId("nope"), a.generation);
    await expect(h.pendingOperations(unknown)).rejects.toThrow(ProfileAccessError);
  });

  it("subscribers see changes for their profile only", async () => {
    const { h, a, b } = setup();
    let hitsA = 0;
    h.subscribe({ kind: "GetConversationList", ctx: a }, () => { hitsA++; });
    await h.dispatch(send(b, "op-b", CB));
    expect(hitsA).toBe(0);
    await h.dispatch(send(a, "op-a"));
    expect(hitsA).toBe(1);
  });
});

describe("onboarding intents", () => {
  const op = (s: string): OperationId => asOperationId(s);

  it("recovery key is shown until confirmed, then never again", async () => {
    const { h, a } = setup();
    await h.dispatch({ kind: "CreateIdentityProfile", ctx: a, operationId: op("create-1"), label: "Me" });
    const key = await h.query({ kind: "GetRecoveryKey", ctx: a, keyOperationId: op("create-1") });
    expect(key?.groups.join("-")).toMatch(/^OM-DEV-FAKE-/);
    await h.dispatch({ kind: "ConfirmRecoveryKeyStored", ctx: a, operationId: op("confirm-1"), keyOperationId: op("create-1") });
    expect(await h.query({ kind: "GetRecoveryKey", ctx: a, keyOperationId: op("create-1") })).toBeNull();
    const again = await h.dispatch({ kind: "ConfirmRecoveryKeyStored", ctx: a, operationId: op("confirm-2"), keyOperationId: op("create-1") });
    const ops = await h.pendingOperations(a);
    expect(ops.find((o) => o.operationId === again.operationId)?.outcome).toEqual({ kind: "Rejected", reason: "not_found" });
  });

  it("a second identity creation is rejected and the key of another profile is not readable", async () => {
    const { h, a, b } = setup();
    await h.dispatch({ kind: "CreateIdentityProfile", ctx: a, operationId: op("c1"), label: "Me" });
    const second = await h.dispatch({ kind: "CreateIdentityProfile", ctx: a, operationId: op("c2"), label: "Me" });
    expect((await h.pendingOperations(a)).find((o) => o.operationId === second.operationId)?.outcome).toEqual({ kind: "Rejected", reason: "conflict" });
    expect(await h.query({ kind: "GetRecoveryKey", ctx: b, keyOperationId: op("c1") })).toBeNull();
  });

  it("invites carry an om: link and expiry; acceptance validates strictly and rejects own invites", async () => {
    const { h, a } = setup();
    await h.dispatch({ kind: "CreateInvite", ctx: a, operationId: op("inv-1") });
    const inv = await h.query({ kind: "GetInvite", ctx: a, operationId: op("inv-1") });
    expect(inv?.link).toMatch(/^om:invite\/[A-Za-z0-9_-]{22}$/);
    expect(inv?.expiresAt).toBeGreaterThan(1000);
    const outcome = async (link: string, id: string): Promise<OutcomeClass | undefined> => {
      await h.dispatch({ kind: "AcceptInvitation", ctx: a, operationId: op(id), link });
      return (await h.pendingOperations(a)).find((o) => o.operationId === op(id))?.outcome;
    };
    expect(await outcome("https://evil.example/x", "acc-1")).toEqual({ kind: "Rejected", reason: "invalid" });
    expect(await outcome(inv?.link ?? "", "acc-2")).toEqual({ kind: "Rejected", reason: "invalid" });
    expect(await outcome("om:invite/AAAAAAAAAAAAAAAAAAAAAA", "acc-3")).toEqual({ kind: "Committed" });
  });

  it("home summary is a machine-code view model", async () => {
    const { h, a } = setup();
    const home = await h.query({ kind: "GetHomeSummary", ctx: a });
    expect(home.tier).toMatch(/^T\d$/);
    for (const c of [...home.observers, ...home.clientOriginChoice]) expect(c).toMatch(/^[a-z_]+$/);
  });
});
