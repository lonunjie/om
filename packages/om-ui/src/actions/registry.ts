// Action Registry (ADR-0159): semantic UI action id -> typed CoreIntent | query | presentation op | platform capability.
// It has no canonical-state authority and no generic execute(name, args) path: unknown ids are rejected.
import {
  asConversationId, asMessageId, parseInviteLink, asOperationId, asPersonId, asProfileId, isProfileExecutionContext,
} from "@om/platform-api";
import type {
  ConfirmationToken, CoreIntent, CoreQuery, OperationId, PrivacyPolicyLevel, ProfileExecutionContext,
} from "@om/platform-api";

export type PresentationOp =
  | { readonly op: "navigate"; readonly conversationId: string }
  | { readonly op: "set-theme"; readonly themeId: string };
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

export interface ActionRegistry {
  readonly actionIds: readonly string[];
  resolve(req: ActionRequest): ActionResult;
  /** Mint a single-use token bound to this exact action, params and profile context. Direct (user-gesture) origin only. */
  issueConfirmation(req: Omit<ActionRequest, "confirmation">): ConfirmationIssue;
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
    if (d === null) return null;
    return { intent: (ctx, op, confirmation) => ({ kind: "SuspendDevice", ctx, operationId: op, deviceId: d, confirmation }) };
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
      return { status: "intent", intent: built.intent(req.ctx, newOperationId(), token ?? (Object.freeze({}) as ConfirmationToken)) };
    },

    issueConfirmation(req) {
      if (req.origin !== "direct") return { status: "rejected", code: "suggestion_cannot_confirm" };
      const p = prepare(req);
      if (typeof p === "string") return { status: "rejected", code: p };
      if (!p.spec.destructive) return { status: "rejected", code: "invalid_params" };
      const token = Object.freeze({}) as ConfirmationToken;
      issued.set(token, bindingOf(req.actionId, p.params, req.ctx));
      return { status: "issued", token };
    },
  };
}
