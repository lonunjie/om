import { describe, expect, it } from "vitest";
import { asOperationId, asPersonId, asProfileId } from "@om/platform-api";
import type { ConfirmationToken, CoreIntent } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { confirmWithCore } from "./confirm";
import { createActionRegistry } from "./registry";

const ME = asProfileId("me");
const PERSON = asPersonId("bob-person");

function setup() {
  const h = createHarness({ env: {}, clock: () => 1000 });
  h.seedProfile({ profileId: ME, label: "Me", conversations: [{ id: "c1" as never, personId: PERSON, displayName: "Bob" }] });
  let n = 0;
  const registry = createActionRegistry({ newOperationId: () => asOperationId(`op-${String(++n)}`) });
  return { h, registry, ctx: h.contextFor(ME) };
}
const params = { personId: PERSON, scope: "local" } as const;

describe("confirmWithCore (UI-11, ADR-0181 section 8)", () => {
  it("obtains a nonce from Core and the resulting token confirms the matching action exactly once", async () => {
    const { h, registry, ctx } = setup();
    const issued = await confirmWithCore({ client: h, registry, ctx }, "block-person", params);
    if (issued.status !== "issued") throw new Error("not issued");
    expect(issued.token.nonce).toMatch(/^[0-9a-f]{32}$/);
    const r = registry.resolve({ actionId: "block-person", params, ctx, origin: "direct", confirmation: issued.token });
    if (r.status !== "intent") throw new Error("expected intent");
    const handle = await h.dispatch(r.intent);
    expect((await h.pendingOperations(ctx)).find((o) => o.operationId === handle.operationId)?.outcome).toEqual({ kind: "Committed" });
    // The registry spent its binding; Core spent the nonce: replaying the intent is refused by Core itself.
    expect(registry.resolve({ actionId: "block-person", params, ctx, origin: "direct", confirmation: issued.token })).toEqual({ status: "rejected", code: "invalid_confirmation" });
    const replay = await h.dispatch({ ...r.intent, operationId: asOperationId("replay") });
    expect((await h.pendingOperations(ctx)).find((o) => o.operationId === replay.operationId)?.outcome).toEqual({ kind: "Rejected", reason: "unauthorized" });
  });

  it("a token minted for one target cannot confirm another, even if the registry binding is bypassed", async () => {
    const { h, registry, ctx } = setup();
    const issued = await confirmWithCore({ client: h, registry, ctx }, "block-person", { personId: "someone-else", scope: "local" });
    if (issued.status !== "issued") throw new Error("not issued");
    const forged: CoreIntent = { kind: "BlockPerson", ctx, operationId: asOperationId("forged"), personId: PERSON, scope: "local", confirmation: issued.token };
    const handle = await h.dispatch(forged);
    expect((await h.pendingOperations(ctx)).find((o) => o.operationId === handle.operationId)?.outcome).toEqual({ kind: "Rejected", reason: "unauthorized" });
  });

  it("a token Core never issued is unauthorized whatever the UI says", async () => {
    const { h, ctx } = setup();
    const made = Object.freeze({ nonce: "00".repeat(16) }) as ConfirmationToken;
    const handle = await h.dispatch({ kind: "BlockPerson", ctx, operationId: asOperationId("made-up"), personId: PERSON, scope: "local", confirmation: made });
    expect((await h.pendingOperations(ctx)).find((o) => o.operationId === handle.operationId)?.outcome).toEqual({ kind: "Rejected", reason: "unauthorized" });
  });

  it("asks Core for nothing when the action is not a direct destructive one", async () => {
    const { h, registry, ctx } = setup();
    expect(await confirmWithCore({ client: h, registry, ctx }, "send-message", { conversationId: "c1", text: "x" })).toEqual({ status: "rejected", code: "invalid_params" });
    expect(await h.pendingOperations(ctx)).toEqual([]);
  });
});
