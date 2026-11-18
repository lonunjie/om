// Action Registry (ADR-0159): semantic UI action id -> typed CoreIntent | query | presentation op | platform capability.
// It has no canonical-state authority and no generic execute(name, args) path: unknown ids are rejected.
import {
  asConversationId, asDeviceId, asGroupId, asMessageId, canonicalInviteLink, confirmationTarget, parseDeviceLink, parseInviteLink, asOperationId, asPersonId, asProfileId, isProfileExecutionContext,
} from "@om/platform-api";
import type {
  ConfirmationToken, CoreIntent, CoreQuery, GroupIntent, GroupRoomSetting, MessageId, OperationId, PrivacyPolicyLevel, ProfileExecutionContext, RecoveryCustodianCode, RecoveryScope,
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
  /** A typed step of the groups path (`client.groups.do`), not a CoreIntent: it has no OperationId, the step itself is durable. Direct (user-gesture) origin only. */
  | { readonly status: "group"; readonly intent: GroupIntent }
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
  | { readonly group: GroupIntent }
  | { readonly platform: PlatformRequest };
interface Spec { readonly destructive: boolean; readonly parse: (p: Rec, ctx: ProfileExecutionContext) => Built | null }

const ID = /^[A-Za-z0-9_.:-]{1,128}$/;
const MAX_TEXT = 65536;
/** The recovery approval link (ADR-0203): its own prefix, never an enrollment link. */
export const RECOVERY_APPROVAL_LINK = /^om:recover-approve\/[A-Za-z0-9_-]{16,200}$/;
const id = (v: unknown): string | null => (typeof v === "string" && ID.test(v) ? v : null);
const oneOf = <T extends string>(v: unknown, set: readonly T[]): T | null => set.find((s) => s === v) ?? null;
const policies: readonly PrivacyPolicyLevel[] = ["recommended", "hardened", "maximum", "custom"];
const scopes: readonly RecoveryScope[] = ["identity", "history", "provider"];
const custodians: readonly RecoveryCustodianCode[] = ["synced_passkey", "recovery_key", "trusted_contacts", "spare_device"];
/** Strict shape for security-critical actions: any key outside `allowed` (e.g. a "silent" or "skip notice" flag) rejects the request. */
/** The optional `replyTo` of a send: absent, or the id of a message. `null` is a malformed one (the send is refused, never sent without its quote). */
const replyOf = (p: Rec): MessageId | undefined | null => {
  const r = p["replyTo"];
  if (r === undefined) return undefined;
  const v = id(r);
  return v === null ? null : asMessageId(v);
};
/** A reaction key Core would accept: 1 to 64 bytes, not blank, no control character. */
const validReactionKey = (k: string): boolean => k.trim() !== "" && new TextEncoder().encode(k).length <= 64 && !/\p{Cc}/u.test(k);
const exactly = (p: Rec, allowed: readonly string[]): boolean => Object.keys(p).every((k) => allowed.includes(k));

/** One Room setting with exactly one field of the expected type. Only what a screen offers is accepted: not `open` joining, not wider history, not a timer (ADR-0189 gives no durations or widening choices). */
function parseRoomSetting(v: unknown): GroupRoomSetting | null {
  if (v === null || typeof v !== "object" || Array.isArray(v)) return null;
  const keys = Object.keys(v);
  const k = keys[0];
  if (keys.length !== 1 || k === undefined) return null;
  const x = (v as Record<string, unknown>)[k];
  switch (k) {
    case "protection": return x === 1 || x === 2 || x === 3 ? { protection: x } : null;
    case "join_rule": return x === 1 || x === 2 ? { join_rule: x } : null;
    case "new_device_policy": return x === 1 || x === 2 ? { new_device_policy: x } : null;
    case "members_may_invite": return typeof x === "boolean" ? { members_may_invite: x } : null;
    default: return null;
  }
}

const SPECS: Readonly<Record<string, Spec>> = {
  "open-conversation": { destructive: false, parse: (p, ctx) => {
    const c = id(p["conversationId"]);
    if (c === null) return null;
    return { presentation: { op: "navigate", conversationId: c }, query: { kind: "GetConversationView", ctx, conversationId: asConversationId(c) } };
  } },
  "send-message": { destructive: false, parse: (p) => {
    const c = id(p["conversationId"]); const t = p["text"]; const r = replyOf(p);
    if (!c || typeof t !== "string" || t.trim() === "" || t.length > MAX_TEXT || r === null) return null;
    return { intent: (ctx, op) => ({ kind: "SendMessage", ctx, operationId: op, conversationId: asConversationId(c), draft: { text: t, ...(r === undefined ? {} : { replyTo: r }) } }) };
  } },
  // Ordinary groups (ADR-0177). They go to `client.groups.do`, never to the generic CoreIntent path. A person's own tap is the only origin:
  // a suggestion (Omnibar, learned rank) cannot start, answer or leave a group. Leave and remove are consequential: the screen states the scope first.
  "group-create": { destructive: false, parse: (p) => {
    const name = p["name"]; const rel = p["relationshipIds"]; const level = p["protection"];
    if (!exactly(p, level === undefined ? ["name", "relationshipIds"] : ["name", "relationshipIds", "protection"]) || typeof name !== "string" || name.length > 64 || !Array.isArray(rel) || rel.length < 1 || rel.length > 31) return null;
    const ids: string[] = [];
    for (const r of rel as unknown[]) { const v = id(r); if (v === null || ids.includes(v)) return null; ids.push(v); }
    const relationshipIds = ids.map(asConversationId);
    // No level named: the default Recommended (`create`). A named level is one of the three of ADR-0189; anything else is refused here, never guessed.
    if (level === undefined) return { group: { kind: "create", name: name.trim(), relationshipIds } };
    if (level !== 1 && level !== 2 && level !== 3) return null;
    return { group: { kind: "create_with_protection", name: name.trim(), relationshipIds, protection: level } };
  } },
  // One Room setting. Tightening applies at once; a loosening carries `confirmed: true` only after the screen named what changes (Core refuses it otherwise).
  "group-room-setting": { destructive: false, parse: (p) => {
    const g = id(p["conversationId"]); const s = p["setting"]; const c = p["confirmed"];
    if (g === null || !exactly(p, c === undefined ? ["conversationId", "setting"] : ["conversationId", "setting", "confirmed"]) || (c !== undefined && typeof c !== "boolean")) return null;
    const setting = parseRoomSetting(s);
    if (setting === null) return null;
    return { group: { kind: "room_setting", conversationId: asGroupId(g), setting, ...(c === true ? { confirmed: true } : {}) } };
  } },
  "group-invite": { destructive: false, parse: (p) => {
    const g = id(p["conversationId"]); const r = id(p["relationshipId"]);
    if (!exactly(p, ["conversationId", "relationshipId"]) || g === null || r === null) return null;
    return { group: { kind: "invite", conversationId: asGroupId(g), relationshipId: asConversationId(r) } };
  } },
  "group-ask-to-rejoin": { destructive: false, parse: (p) => {
    const r = id(p["relationshipId"]); const hint = p["roomHint"];
    if (!exactly(p, ["relationshipId", "roomHint"]) || r === null || typeof hint !== "string" || hint.trim() === "" || hint.length > 256) return null;
    return { group: { kind: "ask_to_rejoin", relationshipId: asConversationId(r), roomHint: hint } };
  } },
  "group-resolve-rejoin": { destructive: false, parse: (p) => {
    const r = id(p["requestId"]); const agree = p["agree"];
    if (!exactly(p, ["requestId", "agree"]) || r === null || typeof agree !== "boolean") return null;
    return { group: { kind: "resolve_rejoin", requestId: r, agree } };
  } },
  "group-accept": { destructive: false, parse: (p) => {
    const i = id(p["inviteId"]);
    if (!exactly(p, ["inviteId"]) || i === null) return null;
    return { group: { kind: "accept", inviteId: i } };
  } },
  "group-decline": { destructive: false, parse: (p) => {
    const i = id(p["inviteId"]);
    if (!exactly(p, ["inviteId"]) || i === null) return null;
    return { group: { kind: "decline", inviteId: i } };
  } },
  "group-leave": { destructive: false, parse: (p) => {
    const g = id(p["conversationId"]);
    if (!exactly(p, ["conversationId"]) || g === null) return null;
    return { group: { kind: "leave", conversationId: asGroupId(g) } };
  } },
  "group-remove-member": { destructive: false, parse: (p) => {
    const g = id(p["conversationId"]); const a = id(p["account"]);
    if (!exactly(p, ["conversationId", "account"]) || g === null || a === null) return null;
    return { group: { kind: "remove", conversationId: asGroupId(g), account: a } };
  } },
  "group-send-text": { destructive: false, parse: (p) => {
    const g = id(p["conversationId"]); const t = p["text"];
    if (!exactly(p, ["conversationId", "text"]) || g === null || typeof t !== "string" || t.trim() === "" || t.length > MAX_TEXT) return null;
    return { group: { kind: "send_text", conversationId: asGroupId(g), text: t } };
  } },
  "group-send-reply": { destructive: false, parse: (p) => {
    const g = id(p["conversationId"]); const t = p["text"]; const r = id(p["replyTo"]);
    if (!exactly(p, ["conversationId", "text", "replyTo"]) || g === null || r === null || typeof t !== "string" || t.trim() === "" || t.length > MAX_TEXT) return null;
    return { group: { kind: "send_reply", conversationId: asGroupId(g), text: t, replyTo: r } };
  } },
  "group-react-message": { destructive: false, parse: (p) => {
    const g = id(p["conversationId"]); const m = id(p["messageId"]); const k = p["reaction"]; const present = p["present"];
    if (!exactly(p, ["conversationId", "messageId", "reaction", "present"]) || g === null || m === null || typeof present !== "boolean" || typeof k !== "string" || !validReactionKey(k)) return null;
    return { group: { kind: "set_reaction", conversationId: asGroupId(g), messageId: m, key: k, present } };
  } },
  "group-edit-message": { destructive: false, parse: (p) => {
    const g = id(p["conversationId"]); const m = id(p["messageId"]); const t = p["text"];
    if (!exactly(p, ["conversationId", "messageId", "text"]) || g === null || m === null || typeof t !== "string" || t.trim() === "" || t.length > MAX_TEXT) return null;
    return { group: { kind: "edit_message", conversationId: asGroupId(g), messageId: m, text: t } };
  } },
  "group-delete-message": { destructive: false, parse: (p) => {
    const g = id(p["conversationId"]); const m = id(p["messageId"]);
    if (!exactly(p, ["conversationId", "messageId"]) || g === null || m === null) return null;
    return { group: { kind: "delete_message", conversationId: asGroupId(g), messageId: m } };
  } },
  "send-attachments": { destructive: false, parse: (p) => {
    const c = id(p["conversationId"]); const t = p["text"]; const a = p["attachments"]; const r = replyOf(p);
    if (!exactly(p, ["conversationId", "text", "attachments", "replyTo"]) || r === null || !c || typeof t !== "string" || t.length > MAX_TEXT || !Array.isArray(a) || a.length < 1 || a.length > 8) return null;
    const items: { blobId: string; voiceNote?: { waveform?: string } }[] = [];
    for (const x of a as unknown[]) {
      if (x === null || typeof x !== "object") return null;
      const o = x as Rec;
      const blobId = o["blobId"];
      const voice = o["voiceNote"];
      if (typeof blobId !== "string" || !/^[0-9a-f]{64}$/.test(blobId)) return null;
      if (voice === undefined) { items.push({ blobId }); continue; }
      const wf = voice !== null && typeof voice === "object" ? (voice as Rec)["waveform"] : undefined;
      if (voice === null || typeof voice !== "object" || (wf !== undefined && (typeof wf !== "string" || !/^([0-9a-f]{2}){16,256}$/.test(wf)))) return null;
      items.push({ blobId, voiceNote: wf === undefined ? {} : { waveform: wf } });
    }
    if (items.filter((i) => i.voiceNote !== undefined).length > 1) return null; // one voice note per message
    return { intent: (ctx, op) => ({ kind: "SendAttachments", ctx, operationId: op, conversationId: asConversationId(c), draft: { text: t, ...(r === undefined ? {} : { replyTo: r }) }, attachments: items }) };
  } },
  "send-attachments-direct": { destructive: false, parse: (p) => {
    const c = id(p["conversationId"]); const t = p["text"]; const a = p["attachments"]; const r = replyOf(p);
    if (!exactly(p, ["conversationId", "text", "attachments", "replyTo"]) || r === null || !c || typeof t !== "string" || t.length > MAX_TEXT || !Array.isArray(a) || a.length < 1 || a.length > 8) return null;
    const items: { blobId: string; voiceNote?: { waveform?: string } }[] = [];
    for (const x of a as unknown[]) {
      if (x === null || typeof x !== "object") return null;
      const o = x as Rec;
      const blobId = o["blobId"];
      const voice = o["voiceNote"];
      if (typeof blobId !== "string" || !/^[0-9a-f]{64}$/.test(blobId)) return null;
      if (voice === undefined) { items.push({ blobId }); continue; }
      const wf = voice !== null && typeof voice === "object" ? (voice as Rec)["waveform"] : undefined;
      if (voice === null || typeof voice !== "object" || (wf !== undefined && (typeof wf !== "string" || !/^([0-9a-f]{2}){16,256}$/.test(wf)))) return null;
      items.push({ blobId, voiceNote: wf === undefined ? {} : { waveform: wf } });
    }
    if (items.filter((i) => i.voiceNote !== undefined).length > 1) return null; // one voice note per message
    return { intent: (ctx, op) => ({ kind: "SendAttachmentsDirect", ctx, operationId: op, conversationId: asConversationId(c), draft: { text: t, ...(r === undefined ? {} : { replyTo: r }) }, attachments: items }) };
  } },
  "withdraw-direct-offer": { destructive: false, parse: (p) => {
    const m = id(p["messageId"]);
    if (!exactly(p, ["messageId"]) || m === null) return null;
    return { intent: (ctx, op) => ({ kind: "WithdrawDirectOffer", ctx, operationId: op, messageId: asMessageId(m) }) };
  } },
  "send-location": { destructive: false, parse: (p) => {
    const c = id(p["conversationId"]); const lat = p["latE7"]; const lon = p["lonE7"]; const acc = p["accuracyM"];
    if (!exactly(p, ["conversationId", "latE7", "lonE7", "accuracyM", "label", "note"]) || !c) return null;
    const inRange = (v: unknown, max: number): v is number => typeof v === "number" && Number.isInteger(v) && Math.abs(v) <= max;
    if (!inRange(lat, 900_000_000) || !inRange(lon, 1_800_000_000)) return null;
    if (acc !== undefined && !(typeof acc === "number" && Number.isInteger(acc) && acc >= 1 && acc <= 1_000_000)) return null;
    const text = (v: unknown): string | undefined | null => (v === undefined ? undefined : typeof v === "string" && v.length <= 256 ? v : null);
    const label = text(p["label"]); const note = text(p["note"]);
    if (label === null || note === null) return null;
    return { intent: (ctx, op) => ({ kind: "SendLocation", ctx, operationId: op, conversationId: asConversationId(c), latE7: lat, lonE7: lon, ...(acc === undefined ? {} : { accuracyM: acc }), ...(label === undefined ? {} : { label }), ...(note === undefined ? {} : { note }) }) };
  } },
  // One reaction of ours on a message: set or removed. The key is a Unicode token; Core validates it again (1 to 64 bytes, no control characters).
  "react-message": { destructive: false, parse: (p) => {
    const c = id(p["conversationId"]); const m = id(p["messageId"]); const k = p["reaction"]; const present = p["present"];
    if (!exactly(p, ["conversationId", "messageId", "reaction", "present"]) || !c || !m || typeof k !== "string" || k.trim() === "" || new TextEncoder().encode(k).length > 64 || /\p{Cc}/u.test(k) || typeof present !== "boolean") return null;
    return { intent: (ctx, op) => ({ kind: "SetReaction", ctx, operationId: op, conversationId: asConversationId(c), messageId: asMessageId(m), reaction: k, present }) };
  } },
  // Edit and delete name one of our own messages. The scope of a delete is said on the screen before it is sent (everyone in the chat).
  "edit-message": { destructive: false, parse: (p) => {
    const c = id(p["conversationId"]); const m = id(p["messageId"]); const t = p["text"];
    if (!exactly(p, ["conversationId", "messageId", "text"]) || !c || !m || typeof t !== "string" || t.trim() === "" || t.length > MAX_TEXT) return null;
    return { intent: (ctx, op) => ({ kind: "EditMessage", ctx, operationId: op, conversationId: asConversationId(c), messageId: asMessageId(m), text: t }) };
  } },
  "delete-message": { destructive: false, parse: (p) => {
    const c = id(p["conversationId"]); const m = id(p["messageId"]);
    if (!exactly(p, ["conversationId", "messageId"]) || !c || !m) return null;
    return { intent: (ctx, op) => ({ kind: "DeleteMessage", ctx, operationId: op, conversationId: asConversationId(c), messageId: asMessageId(m) }) };
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
  "offer-controller-change": { destructive: false, parse: (p) => {
    const deviceId = id(p["deviceId"]);
    const remove = p["remove"];
    if (!exactly(p, ["deviceId", "remove"]) || deviceId === null || typeof remove !== "boolean") return null;
    return { intent: (ctx, op) => ({ kind: "OfferControllerChange", ctx, operationId: op, deviceId: asDeviceId(deviceId), remove }) };
  } },
  "join-controller-change": { destructive: false, parse: (p) => {
    const link = p["link"];
    if (!exactly(p, ["link"]) || typeof link !== "string" || !/^om:controller-change\/[01]\/[0-9a-f]{32}\/[A-Za-z0-9_-]{1,200}$/.test(link.trim())) return null;
    return { intent: (ctx, op) => ({ kind: "JoinControllerChange", ctx, operationId: op, link: link.trim() }) };
  } },
  "confirm-controller-change": { destructive: true, parse: (p) => {
    const requestId = id(p["requestId"]);
    if (!exactly(p, ["requestId"]) || requestId === null) return null;
    return { intent: (ctx, op, confirmation) => ({ kind: "ConfirmControllerChange", ctx, operationId: op, requestId, confirmation }) };
  } },
  "cancel-controller-change": { destructive: false, parse: (p) => {
    const requestId = id(p["requestId"]);
    if (!exactly(p, ["requestId"]) || requestId === null) return null;
    return { intent: (ctx, op) => ({ kind: "CancelControllerChange", ctx, operationId: op, requestId }) };
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
  // Tell a device about the devices added to the account now (ADR-0193). Local to this device; it names the device and nothing else.
  "retry-device-notice": { destructive: false, parse: (p) => {
    const d = id(p["deviceId"]);
    if (!exactly(p, ["deviceId"]) || d === null) return null;
    return { intent: (ctx, op) => ({ kind: "RetryDeviceNotice", ctx, operationId: op, deviceId: asDeviceId(d) }) };
  } },
  "set-strict-device-notice": { destructive: false, parse: (p) => {
    const e = p["enabled"];
    if (!exactly(p, ["enabled"]) || typeof e !== "boolean") return null;
    return { intent: (ctx, op) => ({ kind: "SetStrictDeviceNotice", ctx, operationId: op, enabled: e }) };
  } },
  "set-contact-nickname": { destructive: false, parse: (p) => {
    const c = id(p["conversationId"]);
    const n = p["nickname"];
    if (!exactly(p, ["conversationId", "nickname"]) || c === null || typeof n !== "string") return null;
    return { intent: (ctx, op) => ({ kind: "SetContactNickname", ctx, operationId: op, conversationId: asConversationId(c), ...(n.trim() === "" ? {} : { nickname: n }) }) };
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
  // A contact's recovery of their own account (ADR-0190 section 4). Local to this device: it names the conversation and the request,
  // nothing else, and a suggestion (Omnibar, learned rank) can never carry or obtain the confirmation.
  "confirm-peer-recovery": { destructive: true, parse: (p) => {
    const c = id(p["conversationId"]); const r = id(p["requestId"]);
    if (!exactly(p, ["conversationId", "requestId"]) || c === null || r === null) return null;
    return { intent: (ctx, op, confirmation) => ({ kind: "ConfirmPeerRecovery", ctx, operationId: op, conversationId: asConversationId(c), requestId: r, confirmation }) };
  } },
  "decline-peer-recovery": { destructive: true, parse: (p) => {
    const c = id(p["conversationId"]); const r = id(p["requestId"]);
    if (!exactly(p, ["conversationId", "requestId"]) || c === null || r === null) return null;
    return { intent: (ctx, op, confirmation) => ({ kind: "DeclinePeerRecovery", ctx, operationId: op, conversationId: asConversationId(c), requestId: r, confirmation }) };
  } },
  // The person's own level (ADR-0190 section 4, D80). Tightening and loosening are both this destructive intent with a Core-issued nonce (D130); a suggestion can never obtain one.
  "change-privacy-policy": { destructive: true, parse: (p) => {
    const policy = oneOf(p["policy"], policies);
    if (policy === null || !exactly(p, ["policy"])) return null;
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
  // Getting an account back after losing every device (ADR-0200, ADR-0202, ADR-0203). The key sheet is the person's own secret: it
  // is passed to Core and never logged or echoed. Nothing here can approve, skip a wait or choose a pin.
  "begin-recovery": { destructive: false, parse: (p) => {
    const key = p["recoveryKey"]; const place = p["place"];
    if (!exactly(p, ["recoveryKey", "place"]) || typeof key !== "string" || key.trim() === "" || key.length > 4096) return null;
    if (place !== undefined && (typeof place !== "string" || place.trim() === "" || place.length > 256)) return null;
    return { intent: (ctx, op) => ({ kind: "BeginRecovery", ctx, operationId: op, recoveryKey: key, ...(place === undefined ? {} : { place: place.trim() }) }) };
  } },
  "request-recovery": { destructive: false, parse: (p) => (exactly(p, []) ? { intent: (ctx, op) => ({ kind: "RequestRecovery", ctx, operationId: op }) } : null) },
  "cancel-recovery": { destructive: false, parse: (p) => (exactly(p, []) ? { intent: (ctx, op) => ({ kind: "CancelRecovery", ctx, operationId: op }) } : null) },
  "check-recovery-again": { destructive: false, parse: (p) => (exactly(p, []) ? { intent: (ctx, op) => ({ kind: "CheckRecoveryAgain", ctx, operationId: op }) } : null) },
  "replace-recovery-key": { destructive: false, parse: (p) => (exactly(p, []) ? { intent: (ctx, op) => ({ kind: "ReplaceRecoveryKey", ctx, operationId: op }) } : null) },
  "offer-recovery-approval": { destructive: false, parse: (p) => (exactly(p, []) ? { intent: (ctx, op) => ({ kind: "OfferRecoveryApproval", ctx, operationId: op }) } : null) },
  "join-recovery-approval": { destructive: false, parse: (p) => {
    const link = p["link"];
    if (!exactly(p, ["link"]) || typeof link !== "string" || !RECOVERY_APPROVAL_LINK.test(link.trim())) return null;
    return { intent: (ctx, op) => ({ kind: "JoinRecoveryApproval", ctx, operationId: op, link: link.trim() }) };
  } },
  // "Yes, that was me" on a recovery request this controller was shown (the exact request, after comparing the code).
  "confirm-recovery": { destructive: true, parse: (p) => {
    const r = id(p["requestId"]);
    if (!exactly(p, ["requestId"]) || r === null) return null;
    return { intent: (ctx, op, confirmation) => ({ kind: "ConfirmRecovery", ctx, operationId: op, requestId: r, confirmation }) };
  } },
  "confirm-recovery-key-stored": { destructive: false, parse: (p) => {
    const k = id(p["keyOperationId"]);
    if (k === null) return null;
    return { intent: (ctx, op) => ({ kind: "ConfirmRecoveryKeyStored", ctx, operationId: op, keyOperationId: asOperationId(k) }) };
  } },
  "reconnect-contact": {
    destructive: false,
    parse: (p) => {
      const accountId = p.accountId;
      if (!exactly(p, ["accountId"]) || typeof accountId !== "string" || !/^[0-9a-f]{64}$/.test(accountId)) return null;
      return { intent: (ctx, op) => ({ kind: "ReconnectContact", ctx, operationId: op, accountId }) };
    },
  },
  "create-invite": { destructive: false, parse: () => ({ intent: (ctx, op) => ({ kind: "CreateInvite", ctx, operationId: op }) }) },
  "accept-invitation": { destructive: false, parse: (p) => {
    const link = p["link"];
    const parsed = typeof link === "string" ? parseInviteLink(link) : null;
    if (parsed === null || !parsed.ok) return null;
    // Core is given its own canonical form whichever form the person pasted, scanned or opened.
    return { intent: (ctx, op) => ({ kind: "AcceptInvitation", ctx, operationId: op, link: canonicalInviteLink(parsed.token) }) };
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
      if ("group" in built) return req.origin === "direct" ? { status: "group", intent: built.group } : { status: "rejected", code: "suggestion_cannot_confirm" };
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
