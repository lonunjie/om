import { describe, expect, it } from "vitest";
import { DESTRUCTIVE_INTENT_KINDS, INTENT_KINDS, asOperationId, asProfileGeneration, asProfileId, captureProfileContext } from "@om/platform-api";
import type { ConfirmationToken, CoreIntent } from "@om/platform-api";
import type { ActionResult } from "./registry";
import { createActionRegistry } from "./registry";

const ctx = captureProfileContext(asProfileId("p1"), asProfileGeneration("g1"));
let n = 0;
const registry = createActionRegistry({ newOperationId: () => asOperationId(`op-${String(++n)}`) });
const intentOf = (r: ActionResult): CoreIntent => {
  if (r.status !== "intent") throw new Error(`expected intent, got ${r.status}`);
  return r.intent;
};
const block = { personId: "person-1", scope: "local" };

describe("Action Registry", () => {
  it("UI-01: send-message yields a typed SendMessage CoreIntent bound to the captured context", () => {
    const intent = intentOf(registry.resolve({ actionId: "send-message", params: { conversationId: "c1", text: "hi" }, ctx, origin: "direct" }));
    expect(INTENT_KINDS).toContain(intent.kind);
    expect(intent).toMatchObject({ kind: "SendMessage", conversationId: "c1", draft: { text: "hi" } });
    expect(intent.ctx).toBe(ctx);
  });

  it("every intent-producing action emits a known CoreIntent kind", () => {
    const samples: Record<string, unknown> = {
      "send-message": { conversationId: "c1", text: "x" }, "retry-message": { messageId: "m1" }, "switch-profile": { profileId: "p2" },
      "accept-introduction": { requestId: "r1" }, "share-contact": { personId: "u1", scope: "full" }, "clear-adaptive-navigation": {},
      "open-omnibar-result": { resultId: "x1" }, "create-profile": { label: "Me" }, "confirm-recovery-key-stored": { keyOperationId: "op-9" },
      "create-invite": {}, "accept-invitation": { link: "om:invite/AAAAAAAAAAAAAAAAAAAAAA" },
    };
    for (const [actionId, params] of Object.entries(samples)) {
      expect(INTENT_KINDS).toContain(intentOf(registry.resolve({ actionId, params, ctx, origin: "direct" })).kind);
    }
  });

  it("onboarding actions validate params; accept-invitation rejects anything but a strict om: link", () => {
    for (const link of ["https://x.example/a", "om:invite/short", "om:other/AAAAAAAAAAAAAAAAAAAAAA", "", 5]) {
      expect(registry.resolve({ actionId: "accept-invitation", params: { link }, ctx, origin: "direct" })).toEqual({ status: "rejected", code: "invalid_params" });
    }
    expect(registry.resolve({ actionId: "create-profile", params: { label: " " }, ctx, origin: "direct" })).toEqual({ status: "rejected", code: "invalid_params" });
    expect(registry.resolve({ actionId: "confirm-recovery-key-stored", params: {}, ctx, origin: "direct" })).toEqual({ status: "rejected", code: "invalid_params" });
  });

  it("open-conversation is presentation plus a read-only query, no intent", () => {
    const r = registry.resolve({ actionId: "open-conversation", params: { conversationId: "c1" }, ctx, origin: "direct" });
    expect(r).toMatchObject({ status: "presentation", op: { op: "navigate" }, query: { kind: "GetConversationView" } });
  });

  it("UI-03: unknown ids, prototype keys and hostile params synthesize no intent", () => {
    for (const actionId of ["execute", "revoke-device", "__proto__", "constructor", "toString", ""]) {
      expect(registry.resolve({ actionId, params: {}, ctx, origin: "direct" })).toEqual({ status: "rejected", code: "unknown_action" });
    }
    for (const params of [null, "x", [], { conversationId: "../x", text: "a" }, { conversationId: "c1", text: "   " }, { conversationId: "c1", text: 5 }]) {
      expect(registry.resolve({ actionId: "send-message", params, ctx, origin: "direct" })).toEqual({ status: "rejected", code: "invalid_params" });
    }
  });

  it("UI-03: unfrozen or malformed profile context is rejected", () => {
    const forged = { profileId: asProfileId("p1"), generation: asProfileGeneration("g1") };
    expect(registry.resolve({ actionId: "send-message", params: { conversationId: "c1", text: "a" }, ctx: forged, origin: "direct" }))
      .toEqual({ status: "rejected", code: "invalid_context" });
  });

  it("destructive actions need a registry-issued confirmation bound to action, params and context (UI-11)", () => {
    expect(DESTRUCTIVE_INTENT_KINDS).toEqual(["BlockPerson", "SuspendDevice", "ChangePrivacyPolicy"]);
    const actions: [string, unknown][] = [["block-person", block], ["suspend-device", { deviceId: "d1" }], ["change-privacy-policy", { policy: "maximum" }]];
    for (const [actionId, params] of actions) {
      expect(registry.resolve({ actionId, params, ctx, origin: "direct" })).toEqual({ status: "needs_confirmation", actionId });
      const issue = registry.issueConfirmation({ actionId, params, ctx, origin: "direct" });
      if (issue.status !== "issued") throw new Error("not issued");
      const intent = intentOf(registry.resolve({ actionId, params, ctx, origin: "direct", confirmation: issue.token }));
      expect(DESTRUCTIVE_INTENT_KINDS).toContain(intent.kind);
      // single use
      expect(registry.resolve({ actionId, params, ctx, origin: "direct", confirmation: issue.token })).toEqual({ status: "rejected", code: "invalid_confirmation" });
    }
  });

  it("UI-03/11: forged, replayed-for-other-params and cross-context tokens are rejected", () => {
    const forged = Object.freeze({}) as ConfirmationToken;
    expect(registry.resolve({ actionId: "block-person", params: block, ctx, origin: "direct", confirmation: forged })).toEqual({ status: "rejected", code: "invalid_confirmation" });
    const issue = registry.issueConfirmation({ actionId: "block-person", params: block, ctx, origin: "direct" });
    if (issue.status !== "issued") throw new Error("not issued");
    expect(registry.resolve({ actionId: "block-person", params: { ...block, personId: "person-2" }, ctx, origin: "direct", confirmation: issue.token })).toMatchObject({ status: "rejected" });
    const other = captureProfileContext(asProfileId("p2"), asProfileGeneration("g1"));
    expect(registry.resolve({ actionId: "block-person", params: block, ctx: other, origin: "direct", confirmation: issue.token })).toMatchObject({ status: "rejected" });
    expect(registry.resolve({ actionId: "send-message", params: { conversationId: "c1", text: "a" }, ctx, origin: "direct", confirmation: issue.token })).toMatchObject({ status: "rejected" });
  });

  it("UI-11: a suggestion origin can neither obtain nor use a confirmation", () => {
    expect(registry.issueConfirmation({ actionId: "block-person", params: block, ctx, origin: "suggestion" })).toEqual({ status: "rejected", code: "suggestion_cannot_confirm" });
    const issue = registry.issueConfirmation({ actionId: "block-person", params: block, ctx, origin: "direct" });
    if (issue.status !== "issued") throw new Error("not issued");
    expect(registry.resolve({ actionId: "block-person", params: block, ctx, origin: "suggestion", confirmation: issue.token })).toEqual({ status: "rejected", code: "suggestion_cannot_confirm" });
    expect(registry.resolve({ actionId: "block-person", params: block, ctx, origin: "suggestion" })).toEqual({ status: "needs_confirmation", actionId: "block-person" });
  });

  it("platform actions map to capabilities, not intents", () => {
    expect(registry.resolve({ actionId: "copy-text", params: { sensitive: true }, ctx, origin: "direct" }))
      .toEqual({ status: "platform", request: { capability: "Clipboard", purpose: "write", sensitive: true } });
  });
});
