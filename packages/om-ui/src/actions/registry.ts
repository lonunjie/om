// Action Registry (ADR-0159): semantic UI action id -> typed CoreIntent | query | presentation op | platform capability.
// It has no canonical-state authority and no generic execute(name, args) path: unknown ids are rejected.
import {
  asConversationId, asDeviceId, asMessageId, confirmationTarget, parseDeviceLink, parseInviteLink, asOperationId, asPersonId, asProfileId, isProfileExecutionContext,
} from "@om/platform-api";
import type {
  ConfirmationToken, CoreIntent, CoreQuery, OperationId, PrivacyPolicyLevel, ProfileExecutionContext, RecoveryCustodianCode, RecoveryScope,
} from "@om/platform-api";

export type PresentationOp =
  | { readonly op: "navigate"; readonly conversationId: string }
  | { readonly op: "set-theme"; readonly themeId: string }
  | { readonly op: "open-settings"; readonly section: "devices" | "recovery" };
export type PlatformRequest =
  | { readonly capability: "Files"; readonly purpose: "attach-media" }
  | { readonly capability: "Clipboard"; readonly purpose: "write"; readonly sensitive: boolean };

export type RejectCode = "unknown_action" | "invalid_params" | "invalid_context" | "invalid_confirmation" | "suggestion_cannot_confirm";

export type ActionResult =
  | { readonly status: "intent"; readonly intent: CoreIntent }
  | { readonly status: "presentation"; readonly op: PresentationOp; readonly query?: CoreQuery }
  | { readonly status: "platform"; readonly request: PlatformRequest }
  | { readonly status: "needs_confirmation"; readonly actionId: string }
  | { readonly status: "rejected"; readonly code: RejectCode };

/** `suggestion` = learned/ranked/Omnibar-autocompleted origin; it can never carry or obtain a confirmation. */
export interface ActionRequest {
  readonly actionId: string;
  readonly params: unknown;
  readonly ctx: ProfileExecutionContext;
  readonly origin: "direct" | "suggestion";
  readonly confirmation?: ConfirmationToken;
}

export type ConfirmationIssue =
  | { readonly status: "issued"; readonly token: ConfirmationToken }
  | { readonly status: "rejected"; readonly code: RejectCode };

export type ConfirmationPrep =
  | { readonly status: "intent"; readonly intent: CoreIntent }
  | { readonly status: "rejected"; readonly code: RejectCode };

export interface ActionRegistry {
  readonly actionIds: readonly string[];
  resolve(req: ActionRequest): ActionResult;
  /**
   * Step 1 of a destructive action (UI-11, ADR-0181): the typed `IssueConfirmation` intent that asks Core for a
   * single-use nonce bound to this action's intent kind and target. Direct (user-gesture) origin only.
   */
  prepareConfirmation(req: Omit<ActionRequest, "confirmation">): ConfirmationPrep;
  /**
   * Step 2: wrap the nonce Core issued (read with `GetConfirmation`) in a single-use token bound to this exact
   * action, params and profile context. Core still checks the nonce itself; this binding is the UI-side guard.
   */
  issueConfirmation(req: Omit<ActionRequest, "confirmation"> & { readonly nonce: string }): ConfirmationIssue;
}

export interface RegistryOptions { readonly newOperationId?: () => OperationId }

type Rec = Readonly<Record<string, unknown>>;
type Built = { readonly intent: (ctx: ProfileExecutionContext, op: OperationId, confirmation: ConfirmationToken) => CoreIntent }
  | { readonly presentation: PresentationOp; readonly query?: CoreQuery }
  | { readonly platform: PlatformRequest };
interface Spec { readonly destructive: boolean; readonly parse: (p: Rec, ctx: ProfileExecutionContext) => Built | null }

const ID = /^[A-Za-z0-9_.:-]{1,128}$/;
const MAX_TEXT = 65536;
const id = (v: unknown): string | null => (typeof v === "string" && ID.test(v) ? v : null);
const oneOf = <T extends string>(v: unknown, set: readonly T[]): T | null => set.find((s) => s === v) ?? null;
const policies: readonly PrivacyPolicyLevel[] = ["recommended", "hardened", "maximum", "custom"];
const scopes: readonly RecoveryScope[] = ["identity", "history", "provider"];
const custodians: readonly RecoveryCustodianCode[] = ["synced_passkey", "recovery_key", "trusted_contacts", "spare_device"];
/** Strict shape for security-critical actions: any key outside `allowed` (e.g. a "silent" or "skip notice" flag) rejects the request. */
const exactly = (p: Rec, allowed: readonly string[]): boolean => Object.keys(p).every((k) => allowed.includes(k));

const SPECS: Readonly<Record<string, Spec>> = {
  "open-conversation": { destructive: false, parse: (p, ctx) => {
    const c = id(p["conversationId"]);
    if (c === null) return null;
    return { presentation: { op: "navigate", conversationId: c }, query: { kind: "GetConversationView", ctx, conversationId: asConversationId(c) } };
  } },
  "send-message": { destructive: false, parse: (p) => {
    const c = id(p["conversationId"]); const t = p["text"];
    if (!c || typeof t !== "string" || t.trim() === "" || t.length > MAX_TEXT) return null;
    return { intent: (ctx, op) => ({ kind: "SendMessage", ctx, operationId: op, conversationId: asConversationId(c), draft: { text: t } }) };
  } },
  "retry-message": { destructive: false, parse: (p) => {
    const m = id(p["messageId"]);
    if (m === null) return null;
    return { intent: (ctx, op) => ({ kind: "RetryDelivery", ctx, operationId: op, messageId: asMessageId(m) }) };
  } },
  "switch-profile": { destructive: false, parse: (p) => {
    const t = id(p["profileId"]);
    if (t === null) return null;
    return { intent: (ctx, op) => ({ kind: "SwitchIdentityProfile", ctx, operationId: op, targetProfileId: asProfileId(t) }) };
  } },
  "accept-introduction": { destructive: false, parse: (p) => {
    const r = id(p["requestId"]);
    if (r === null) return null;
    return { intent: (ctx, op) => ({ kind: "AcceptIntroduction", ctx, operationId: op, requestId: r }) };
  } },
  "share-contact": { destructive: false, parse: (p) => {
    const person = id(p["personId"]); const scope = oneOf(p["scope"], ["name_only", "full"]);
    if (person === null || scope === null) return null;
    return { intent: (ctx, op) => ({ kind: "ShareContact", ctx, operationId: op, personId: asPersonId(person), scope }) };
  } },
  "clear-adaptive-navigation": { destructive: false, parse: () => ({ intent: (ctx, op) => ({ kind: "ClearAdaptiveNavigation", ctx, operationId: op }) }) },
  "open-omnibar-result": { destructive: false, parse: (p) => {
    const r = id(p["resultId"]);
    if (r === null) return null;
    return { intent: (ctx, op) => ({ kind: "OpenOmnibarResult", ctx, operationId: op, resultId: r }) };
  } },
  "block-person": { destructive: true, parse: (p) => {
    const person = id(p["personId"]); const scope = oneOf(p["scope"], ["local", "provider"]);
    if (person === null || scope === null) return null;
    return { intent: (ctx, op, confirmation) => ({ kind: "BlockPerson", ctx, operationId: op, personId: asPersonId(person), scope, confirmation }) };
  } },
  "suspend-device": { destructive: true, parse: (p) => {
    const d = id(p["deviceId"]);
    if (!exactly(p, ["deviceId"]) || d === null) return null;
    return { intent: (ctx, op, confirmation) => ({ kind: "SuspendDevice", ctx, operationId: op, deviceId: asDeviceId(d), confirmation }) };
  } },
  "offer-device-link": { destructive: false, parse: (p) => (exactly(p, []) ? { intent: (ctx, op) => ({ kind: "OfferDeviceLink", ctx, operationId: op }) } : null) },
  "request-device-link": { destructive: false, parse: (p) => {
    const link = p["link"];
    if (!exactly(p, ["link"]) || typeof link !== "string" || !parseDeviceLink(link).ok) return null;
    return { intent: (ctx, op) => ({ kind: "RequestDeviceLink", ctx, operationId: op, link: link.trim() }) };
  } },
  "approve-device-link": { destructive: true, parse: (p) => {
    const r = id(p["requestId"]);
    if (!exactly(p, ["requestId"]) || r === null) return null;
    return { intent: (ctx, op, confirmation) => ({ kind: "ApproveDeviceLink", ctx, operationId: op, requestId: r, confirmation }) };
  } },
  "reject-device-link": { destructive: false, parse: (p) => {
    const r = id(p["requestId"]);
    if (!exactly(p, ["requestId"]) || r === null) return null;
    return { intent: (ctx, op) => ({ kind: "RejectDeviceLink", ctx, operationId: op, requestId: r }) };
  } },
  "finalize-device-revocation": { destructive: true, parse: (p) => {
    const d = id(p["deviceId"]);
    if (!exactly(p, ["deviceId"]) || d === null) return null;
    return { intent: (ctx, op, confirmation) => ({ kind: "RevokeDevice", ctx, operationId: op, deviceId: asDeviceId(d), confirmation }) };
  } },
  "view-devices": { destructive: false, parse: (p, ctx) => (exactly(p, []) ? { presentation: { op: "open-settings", section: "devices" }, query: { kind: "GetDevices", ctx } } : null) },
  "view-recovery": { destructive: false, parse: (p, ctx) => (exactly(p, []) ? { presentation: { op: "open-settings", section: "recovery" }, query: { kind: "GetRecoveryStatus", ctx } } : null) },
  "start-recovery": { destructive: true, parse: (p) => {
    const scope = oneOf(p["scope"], scopes); const custodian = oneOf(p["custodian"], custodians);
    if (!exactly(p, ["scope", "custodian"]) || scope === null || custodian === null) return null;
    return { intent: (ctx, op, confirmation) => ({ kind: "StartRecovery", ctx, operationId: op, scope, custodian, confirmation }) };
  } },
  "veto-recovery": { destructive: true, parse: (p) => {
    const r = id(p["requestId"]);
    if (!exactly(p, ["requestId"]) || r === null) return null;
    return { intent: (ctx, op, confirmation) => ({ kind: "VetoRecovery", ctx, operationId: op, requestId: r, confirmation }) };
  } },
  "change-privacy-policy": { destructive: true, parse: (p) => {
    const policy = oneOf(p["policy"], policies);
    if (policy === null) return null;
    return { intent: (ctx, op, confirmation) => ({ kind: "ChangePrivacyPolicy", ctx, operationId: op, policy, confirmation }) };
  } },
  "set-theme": { destructive: false, parse: (p) => {
    const t = id(p["themeId"]);
    if (t === null) return null;
    return { presentation: { op: "set-theme", themeId: t } };
  } },
  "create-profile": { destructive: false, parse: (p) => {
    const label = p["label"];
    if (typeof label !== "string" || label.trim() === "" || label.length > 64) return null;
    return { intent: (ctx, op) => ({ kind: "CreateIdentityProfile", ctx, operationId: op, label }) };
  } },
  "confirm-recovery-key-stored": { destructive: false, parse: (p) => {
    const k = id(p["keyOperationId"]);
    if (k === null) return null;
    return { intent: (ctx, op) => ({ kind: "ConfirmRecoveryKeyStored", ctx, operationId: op, keyOperationId: asOperationId(k) }) };
  } },
  "create-invite": { destructive: false, parse: () => ({ intent: (ctx, op) => ({ kind: "CreateInvite", ctx, operationId: op }) }) },
  "accept-invitation": { destructive: false, parse: (p) => {
    const link = p["link"];
    if (typeof link !== "string" || !parseInviteLink(link).ok) return null;
    return { intent: (ctx, op) => ({ kind: "AcceptInvitation", ctx, operationId: op, link: link.trim() }) };
  } },
  "attach-media": { destructive: false, parse: () => ({ platform: { capability: "Files", purpose: "attach-media" } }) },
  "copy-text": { destructive: false, parse: (p) => {
    const sensitive = p["sensitive"];
    return typeof sensitive === "boolean" ? { platform: { capability: "Clipboard", purpose: "write", sensitive } } : null;
  } },
};

const isRec = (v: unknown): v is Rec => typeof v === "object" && v !== null && !Array.isArray(v);
const defaultOpId = (): OperationId => asOperationId(globalThis.crypto.randomUUID());

export function createActionRegistry(options: RegistryOptions = {}): ActionRegistry {
  const newOperationId = options.newOperationId ?? defaultOpId;
  const issued = new WeakMap<ConfirmationToken, string>(); // token -> binding; deleted on use

  const bindingOf = (actionId: string, params: Rec, ctx: ProfileExecutionContext): string =>
    JSON.stringify([actionId, params, ctx.profileId, ctx.generation]);

  function prepare(req: Omit<ActionRequest, "confirmation">): { spec: Spec; built: Built; params: Rec } | RejectCode {
    if (!isProfileExecutionContext(req.ctx)) return "invalid_context";
    if (!Object.hasOwn(SPECS, req.actionId)) return "unknown_action";
    const spec = SPECS[req.actionId];
    if (!spec || !isRec(req.params)) return "invalid_params";
    const built = spec.parse(req.params, req.ctx);
    return built ? { spec, built, params: req.params } : "invalid_params";
  }

  return {
    actionIds: Object.keys(SPECS),

    resolve(req) {
      const p = prepare(req);
      if (typeof p === "string") return { status: "rejected", code: p };
      const { spec, built } = p;
      if ("presentation" in built) return built.query ? { status: "presentation", op: built.presentation, query: built.query } : { status: "presentation", op: built.presentation };
      if ("platform" in built) return { status: "platform", request: built.platform };
      let token: ConfirmationToken | undefined;
      if (spec.destructive) {
        if (req.origin === "suggestion") return req.confirmation ? { status: "rejected", code: "suggestion_cannot_confirm" } : { status: "needs_confirmation", actionId: req.actionId };
        if (!req.confirmation) return { status: "needs_confirmation", actionId: req.actionId };
        if (issued.get(req.confirmation) !== bindingOf(req.actionId, p.params, req.ctx)) return { status: "rejected", code: "invalid_confirmation" };
        issued.delete(req.confirmation);
        token = req.confirmation;
      } else if (req.confirmation) {
        return { status: "rejected", code: "invalid_confirmation" };
      }
      return { status: "intent", intent: built.intent(req.ctx, newOperationId(), token ?? (Object.freeze({ nonce: "" }) as ConfirmationToken)) };
    },

    prepareConfirmation(req) {
      if (req.origin !== "direct") return { status: "rejected", code: "suggestion_cannot_confirm" };
      const p = prepare(req);
      if (typeof p === "string") return { status: "rejected", code: p };
      if (!p.spec.destructive || !("intent" in p.built)) return { status: "rejected", code: "invalid_params" };
      // Build the destructive intent with a placeholder token only to learn its kind and target.
      const probe = p.built.intent(req.ctx, asOperationId("probe"), Object.freeze({ nonce: "" }) as ConfirmationToken);
      if (!("confirmation" in probe)) return { status: "rejected", code: "invalid_params" };
      return { status: "intent", intent: { kind: "IssueConfirmation", ctx: req.ctx, operationId: newOperationId(), intentKind: probe.kind, target: confirmationTarget(probe) } };
    },

    issueConfirmation(req) {
      if (req.origin !== "direct") return { status: "rejected", code: "suggestion_cannot_confirm" };
      const p = prepare(req);
      if (typeof p === "string") return { status: "rejected", code: p };
      if (!p.spec.destructive || req.nonce === "") return { status: "rejected", code: "invalid_params" };
      const token = Object.freeze({ nonce: req.nonce }) as ConfirmationToken;
      issued.set(token, bindingOf(req.actionId, p.params, req.ctx));
      return { status: "issued", token };
    },
  };
}
