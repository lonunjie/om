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
      "offer-device-link": {}, "request-device-link": { link: "om:link/AAAAAAAAAAAAAAAAAAAAAA" }, "reject-device-link": { requestId: "lr-1" },
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
    expect(DESTRUCTIVE_INTENT_KINDS).toEqual(["BlockPerson", "SuspendDevice", "ChangePrivacyPolicy", "ApproveDeviceLink", "RevokeDevice", "StartRecovery", "VetoRecovery"]);
    const actions: [string, unknown][] = [
      ["block-person", block], ["suspend-device", { deviceId: "d1" }], ["change-privacy-policy", { policy: "maximum" }],
      ["approve-device-link", { requestId: "lr-1" }], ["finalize-device-revocation", { deviceId: "d1" }],
      ["start-recovery", { scope: "identity", custodian: "recovery_key" }], ["veto-recovery", { requestId: "rr-1" }],
    ];
    for (const [actionId, params] of actions) {
      expect(registry.resolve({ actionId, params, ctx, origin: "direct" })).toEqual({ status: "needs_confirmation", actionId });
      const issue = registry.issueConfirmation({ actionId, params, ctx, origin: "direct", nonce: "n-1" });
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
    const issue = registry.issueConfirmation({ actionId: "block-person", params: block, ctx, origin: "direct", nonce: "n-1" });
    if (issue.status !== "issued") throw new Error("not issued");
    expect(registry.resolve({ actionId: "block-person", params: { ...block, personId: "person-2" }, ctx, origin: "direct", confirmation: issue.token })).toMatchObject({ status: "rejected" });
    const other = captureProfileContext(asProfileId("p2"), asProfileGeneration("g1"));
    expect(registry.resolve({ actionId: "block-person", params: block, ctx: other, origin: "direct", confirmation: issue.token })).toMatchObject({ status: "rejected" });
    expect(registry.resolve({ actionId: "send-message", params: { conversationId: "c1", text: "a" }, ctx, origin: "direct", confirmation: issue.token })).toMatchObject({ status: "rejected" });
  });

  it("UI-11: a suggestion origin can neither obtain nor use a confirmation", () => {
    expect(registry.issueConfirmation({ actionId: "block-person", params: block, ctx, origin: "suggestion", nonce: "n-1" })).toEqual({ status: "rejected", code: "suggestion_cannot_confirm" });
    const issue = registry.issueConfirmation({ actionId: "block-person", params: block, ctx, origin: "direct", nonce: "n-1" });
    if (issue.status !== "issued") throw new Error("not issued");
    expect(registry.resolve({ actionId: "block-person", params: block, ctx, origin: "suggestion", confirmation: issue.token })).toEqual({ status: "rejected", code: "suggestion_cannot_confirm" });
    expect(registry.resolve({ actionId: "block-person", params: block, ctx, origin: "suggestion" })).toEqual({ status: "needs_confirmation", actionId: "block-person" });
  });

  it("NoSilentLink: link actions accept no extra flags, and approval cannot come from a suggestion", () => {
    for (const params of [{ requestId: "lr-1", notify: false }, { requestId: "lr-1", silent: true }, { requestId: "../x" }, {}]) {
      expect(registry.resolve({ actionId: "approve-device-link", params, ctx, origin: "direct" })).toEqual({ status: "rejected", code: "invalid_params" });
    }
    for (const link of ["om:invite/AAAAAAAAAAAAAAAAAAAAAA", "https://x.example/om:link/AAAAAAAAAAAAAAAAAAAAAA", "om:link/short", ""]) {
      expect(registry.resolve({ actionId: "request-device-link", params: { link }, ctx, origin: "direct" })).toEqual({ status: "rejected", code: "invalid_params" });
    }
    expect(registry.resolve({ actionId: "request-device-link", params: { link: "om:link/AAAAAAAAAAAAAAAAAAAAAA", autoApprove: true }, ctx, origin: "direct" }))
      .toEqual({ status: "rejected", code: "invalid_params" });
    expect(registry.issueConfirmation({ actionId: "approve-device-link", params: { requestId: "lr-1" }, ctx, origin: "suggestion", nonce: "n-1" }))
      .toEqual({ status: "rejected", code: "suggestion_cannot_confirm" });
  });

  it("suspend and permanent revoke are distinct actions; a suspend token cannot revoke", () => {
    const params = { deviceId: "d1" };
    const issue = registry.issueConfirmation({ actionId: "suspend-device", params, ctx, origin: "direct", nonce: "n-1" });
    if (issue.status !== "issued") throw new Error("not issued");
    expect(registry.resolve({ actionId: "finalize-device-revocation", params, ctx, origin: "direct", confirmation: issue.token })).toEqual({ status: "rejected", code: "invalid_confirmation" });
    expect(intentOf(registry.resolve({ actionId: "suspend-device", params, ctx, origin: "direct", confirmation: issue.token })).kind).toBe("SuspendDevice");
  });

  it("recovery scopes and custodians are closed sets; settings search only opens the dedicated screens", () => {
    for (const params of [{ scope: "everything", custodian: "recovery_key" }, { scope: "identity", custodian: "email" }, { scope: "identity" }]) {
      expect(registry.resolve({ actionId: "start-recovery", params, ctx, origin: "direct" })).toEqual({ status: "rejected", code: "invalid_params" });
    }
    expect(registry.resolve({ actionId: "view-recovery", params: {}, ctx, origin: "suggestion" }))
      .toMatchObject({ status: "presentation", op: { op: "open-settings", section: "recovery" }, query: { kind: "GetRecoveryStatus" } });
    expect(registry.resolve({ actionId: "start-recovery", params: { scope: "identity", custodian: "recovery_key" }, ctx, origin: "suggestion" }))
      .toEqual({ status: "needs_confirmation", actionId: "start-recovery" });
  });

  it("platform actions map to capabilities, not intents", () => {
    expect(registry.resolve({ actionId: "copy-text", params: { sensitive: true }, ctx, origin: "direct" }))
      .toEqual({ status: "platform", request: { capability: "Clipboard", purpose: "write", sensitive: true } });
  });
});

describe("Core-issued confirmation, step 1 (UI-11, ADR-0181 section 8)", () => {
  const ids = ["op-ic"];
  const reg = () => { let n = 0; return createActionRegistry({ newOperationId: () => asOperationId(ids[n++] ?? `op-${String(n)}`) }); };

  it("asks Core for a nonce bound to the intent kind and the documented target", () => {
    const r = reg();
    const cases: [string, unknown, string, string][] = [
      ["block-person", block, "BlockPerson", "person-1"], ["suspend-device", { deviceId: "d1" }, "SuspendDevice", "d1"],
      ["finalize-device-revocation", { deviceId: "d1" }, "RevokeDevice", "d1"], ["approve-device-link", { requestId: "lr-1" }, "ApproveDeviceLink", "lr-1"],
      ["veto-recovery", { requestId: "rr-1" }, "VetoRecovery", "rr-1"], ["change-privacy-policy", { policy: "maximum" }, "ChangePrivacyPolicy", "maximum"],
      ["start-recovery", { scope: "history", custodian: "recovery_key" }, "StartRecovery", "history:recovery_key"],
    ];
    for (const [actionId, params, intentKind, target] of cases) {
      const prep = r.prepareConfirmation({ actionId, params, ctx, origin: "direct" });
      expect(prep).toMatchObject({ status: "intent", intent: { kind: "IssueConfirmation", intentKind, target } });
    }
  });

  it("a suggestion, an unknown action or a non-destructive action never reaches Core for a nonce", () => {
    const r = reg();
    expect(r.prepareConfirmation({ actionId: "block-person", params: block, ctx, origin: "suggestion" })).toEqual({ status: "rejected", code: "suggestion_cannot_confirm" });
    expect(r.prepareConfirmation({ actionId: "nope", params: {}, ctx, origin: "direct" })).toEqual({ status: "rejected", code: "unknown_action" });
    expect(r.prepareConfirmation({ actionId: "send-message", params: { conversationId: "c1", text: "a" }, ctx, origin: "direct" })).toEqual({ status: "rejected", code: "invalid_params" });
    expect(r.prepareConfirmation({ actionId: "block-person", params: { personId: "../x" }, ctx, origin: "direct" })).toEqual({ status: "rejected", code: "invalid_params" });
    // An empty nonce is not a confirmation.
    expect(r.issueConfirmation({ actionId: "block-person", params: block, ctx, origin: "direct", nonce: "" })).toEqual({ status: "rejected", code: "invalid_params" });
  });
});

