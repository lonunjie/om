// DEV/TEST ONLY in-memory CoreClient. No real crypto, no persistence beyond the process. See README.md.
import {
  asMessageId, parseInviteLink, asProfileGeneration, captureProfileContext, isProfileExecutionContext, isTerminalOutcome,
} from "@om/platform-api";
import type {
  ConversationId, ConversationSummary, ConversationTimelineItem, ConversationView, CoreClient, CoreIntent, CoreQuery,
  DeliveryState, HomeSummary, IntroductionRequest, InviteView, MessageId, OperationHandle, OperationId, OutcomeClass, PendingOperationView,
  PersonId, ProfileExecutionContext, ProfileId, QueryResult, RejectReason, TerminalOutcome, Unsubscribe,
} from "@om/platform-api";

export const HARNESS_MARKER = "OM-DEV-HARNESS-NOT-FOR-PRODUCTION";

export class HarnessError extends Error {}
/** Thrown when a captured context reaches state owned by another profile, or is stale/forged. Fail closed. */
export class ProfileAccessError extends HarnessError {}
export class OperationIdConflictError extends HarnessError {}

export interface HarnessEnv { readonly PROD?: boolean }

export function assertNotProduction(env: HarnessEnv | undefined): void {
  if (env?.PROD === true) throw new HarnessError(`${HARNESS_MARKER}: refusing to run in a production build`);
}

const viteEnv = (): HarnessEnv | undefined => (import.meta as unknown as { readonly env?: HarnessEnv }).env;

export interface SeedConversation {
  readonly id: ConversationId;
  readonly personId: PersonId;
  readonly displayName: string;
  readonly messages?: readonly { readonly text: string; readonly direction: "incoming" | "outgoing" }[];
}
export interface SeedProfile {
  readonly profileId: ProfileId;
  readonly label: string;
  readonly conversations?: readonly SeedConversation[];
  readonly introductions?: readonly (IntroductionRequest & { readonly personId: PersonId })[];
  readonly home?: HomeSummary;
}

export interface Harness extends CoreClient {
  readonly marker: typeof HARNESS_MARKER;
  /** Registers a profile (generation fixed at seed) and returns its captured context. */
  seedProfile(seed: SeedProfile): ProfileExecutionContext;
  /** Fresh context for a seeded profile at its current generation (what a restarted UI would capture). */
  contextFor(profileId: ProfileId): ProfileExecutionContext;
  /** Arms a fault: when this operation is sent, the ack times out and it ends PendingReconciliation. Arm before dispatch. */
  injectNetworkTimeoutAfterSend(operationId: OperationId): void;
  /** Drops all in-memory observers/subscribers; the durable operation log is kept. */
  simulateRestart(): void;
  resolveReconciliation(operationId: OperationId, outcome: TerminalOutcome): void;
}

export interface HarnessOptions { readonly clock?: () => number; readonly env?: HarnessEnv | undefined }

interface MutableMessage {
  messageId: MessageId;
  operationId: OperationId | null;
  direction: "incoming" | "outgoing";
  text: string;
  sentAt: number;
  delivery: DeliveryState;
}
interface ConversationState { id: ConversationId; personId: PersonId; displayName: string; messages: MutableMessage[] }
interface ProfileState {
  label: string;
  generation: ReturnType<typeof asProfileGeneration>;
  conversations: Map<ConversationId, ConversationState>;
  persons: Map<PersonId, { id: PersonId; displayName: string; blocked: boolean }>;
  introductions: Map<string, IntroductionRequest & { personId: PersonId }>;
  identityCreated: boolean;
  /** Display-once fake recovery keys by creating operation; entries are deleted when confirmed. */
  recoveryKeys: Map<OperationId, readonly string[]>;
  invites: Map<OperationId, InviteView>;
  acceptedInvites: Set<string>;
  home: HomeSummary;
}
interface OpRecord { operationId: OperationId; profileId: ProfileId; intent: CoreIntent; outcome: OutcomeClass; messageId: MessageId | null }

const DEFAULT_HOME: HomeSummary = {
  tier: "T1", observers: ["host_sees_metadata", "host_sees_ciphertext"], migrationAvailable: true, clientOriginChoice: ["default_origin", "independent_origin_available"],
};
const INVITE_TTL_MS = 7 * 24 * 60 * 60 * 1000;
const B32 = "ABCDEFGHIJKLMNPQRSTUVWXYZ23456789";
const B64URL = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
const randomChars = (alphabet: string, n: number): string =>
  Array.from(globalThis.crypto.getRandomValues(new Uint8Array(n)), (b) => alphabet.charAt(b % alphabet.length)).join("");
/** Obviously fake: the first groups spell OM-DEV-FAKE so it can never be mistaken for a real key. */
const fakeRecoveryGroups = (): readonly string[] => ["OM", "DEV", "FAKE", ...Array.from({ length: 6 }, () => randomChars(B32, 4))];

const RETRYABLE: ReadonlySet<RejectReason> = new Set<RejectReason>(["unavailable", "rate_limited", "quota"]);

export function createHarness(options: HarnessOptions = {}): Harness {
  assertNotProduction("env" in options ? options.env : viteEnv());
  const clock = options.clock ?? ((): number => Date.now());

  const profiles = new Map<ProfileId, ProfileState>();
  const conversationOwner = new Map<ConversationId, ProfileId>();
  const messageOwner = new Map<MessageId, ProfileId>();
  const opLog = new Map<OperationId, OpRecord>(); // durable: survives simulateRestart
  const armedTimeouts = new Set<OperationId>();
  let observers = new Map<OperationId, Set<(o: OutcomeClass) => void>>();
  let subscribers = new Set<{ readonly selector: CoreQuery; readonly listener: (r: never) => void }>();
  let messageSeq = 0;

  const settle = <T>(fn: () => T): Promise<T> => {
    try { return Promise.resolve(fn()); } catch (e) { return Promise.reject(e instanceof Error ? e : new HarnessError("unknown failure")); }
  };

  function stateFor(ctx: ProfileExecutionContext): ProfileState {
    if (!isProfileExecutionContext(ctx)) throw new ProfileAccessError("invalid or unfrozen profile context");
    const st = profiles.get(ctx.profileId);
    if (!st) throw new ProfileAccessError("unknown profile");
    if (st.generation !== ctx.generation) throw new ProfileAccessError("stale profile context");
    return st;
  }

  function conversationFor(ctx: ProfileExecutionContext, st: ProfileState, id: ConversationId): ConversationState | null {
    const conv = st.conversations.get(id);
    if (conv) return conv;
    if (conversationOwner.has(id)) throw new ProfileAccessError("conversation belongs to another profile");
    return null;
  }

  const handleOf = (rec: OpRecord): OperationHandle => ({ operationId: rec.operationId, profileId: rec.profileId });

  function view(m: MutableMessage): ConversationTimelineItem {
    return m.direction === "outgoing" && m.operationId !== null
      ? { direction: "outgoing", messageId: m.messageId, operationId: m.operationId, text: m.text, sentAt: m.sentAt, delivery: m.delivery }
      : { direction: "incoming", messageId: m.messageId, text: m.text, sentAt: m.sentAt };
  }
  function summary(c: ConversationState): ConversationSummary {
    return { id: c.id, personId: c.personId, displayName: c.displayName, lastMessageAt: c.messages.at(-1)?.sentAt ?? null, notices: [] };
  }

  function runQuery<Q extends CoreQuery>(q: Q): QueryResult<Q> {
    const out = ((): QueryResult<CoreQuery> => {
      switch (q.kind) {
        case "GetProfileList":
          return [...profiles].map(([id, s]) => ({ id, label: s.label }));
        case "GetConversationList":
          return [...stateFor(q.ctx).conversations.values()].map(summary);
        case "GetIntroductions":
          return [...stateFor(q.ctx).introductions.values()].map(({ id, from, displayName }) => ({ id, from, displayName }));
        case "GetRecoveryKey": {
          const groups = stateFor(q.ctx).recoveryKeys.get(q.keyOperationId);
          return groups ? { keyOperationId: q.keyOperationId, groups: [...groups] } : null;
        }
        case "GetInvite":
          return stateFor(q.ctx).invites.get(q.operationId) ?? null;
        case "GetHomeSummary":
          return stateFor(q.ctx).home;
        case "GetConversationView": {
          const st = stateFor(q.ctx);
          const conv = conversationFor(q.ctx, st, q.conversationId);
          if (!conv) throw new HarnessError("conversation not found");
          const v: ConversationView = { summary: summary(conv), timeline: conv.messages.map(view) };
          return v;
        }
      }
    })();
    return out as QueryResult<Q>;
  }

  function notify(profileId: ProfileId, operationId?: OperationId): void {
    if (operationId) {
      const rec = opLog.get(operationId);
      if (rec) for (const l of [...(observers.get(operationId) ?? [])]) l(rec.outcome);
    }
    for (const s of [...subscribers]) {
      if (s.selector.kind === "GetProfileList" || s.selector.ctx.profileId === profileId) (s.listener as (r: unknown) => void)(runQuery(s.selector));
    }
  }

  function record(intent: CoreIntent, outcome: OutcomeClass, messageId: MessageId | null = null): OpRecord {
    const rec: OpRecord = { operationId: intent.operationId, profileId: intent.ctx.profileId, intent, outcome, messageId };
    opLog.set(rec.operationId, rec);
    return rec;
  }

  function apply(intent: CoreIntent, st: ProfileState): OpRecord {
    const rejected = (reason: RejectReason): OpRecord => record(intent, { kind: "Rejected", reason });
    switch (intent.kind) {
      case "SendMessage": {
        const conv = conversationFor(intent.ctx, st, intent.conversationId);
        if (!conv) return rejected("not_found");
        if (intent.draft.text.trim() === "") return rejected("invalid");
        const armed = armedTimeouts.delete(intent.operationId);
        const messageId = asMessageId(`msg-${String(++messageSeq)}`);
        conv.messages.push({
          messageId, operationId: intent.operationId, direction: "outgoing", text: intent.draft.text, sentAt: clock(),
          delivery: armed ? { state: "pending_reconciliation" } : { state: "sent" },
        });
        messageOwner.set(messageId, intent.ctx.profileId);
        return record(intent, armed ? { kind: "PendingReconciliation" } : { kind: "Committed" }, messageId);
      }
      case "RetryDelivery": {
        const owner = messageOwner.get(intent.messageId);
        if (owner !== undefined && owner !== intent.ctx.profileId) throw new ProfileAccessError("message belongs to another profile");
        const msg = [...st.conversations.values()].flatMap((c) => c.messages).find((m) => m.messageId === intent.messageId);
        if (!msg || msg.operationId === null) return rejected("not_found");
        const original = opLog.get(msg.operationId);
        if (original && !isTerminalOutcome(original.outcome)) return original; // same logical operation; no second one
        if (msg.delivery.state === "rejected" && RETRYABLE.has(msg.delivery.reason)) {
          msg.delivery = { state: "sent" };
          return record(intent, { kind: "Committed" }, msg.messageId);
        }
        return msg.delivery.state === "rejected" ? rejected("conflict") : record(intent, { kind: "Committed" }, msg.messageId);
      }
      case "SwitchIdentityProfile":
        return profiles.has(intent.targetProfileId) ? record(intent, { kind: "Committed" }) : rejected("not_found");
      case "AcceptIntroduction": {
        const intro = st.introductions.get(intent.requestId);
        if (!intro) return rejected("not_found");
        st.introductions.delete(intro.id);
        st.persons.set(intro.personId, { id: intro.personId, displayName: intro.displayName, blocked: false });
        return record(intent, { kind: "Committed" });
      }
      case "CreateIdentityProfile": {
        const label = intent.label.trim();
        if (label === "" || label.length > 64) return rejected("invalid");
        if (st.identityCreated) return rejected("conflict");
        st.identityCreated = true;
        st.label = label;
        st.recoveryKeys.set(intent.operationId, fakeRecoveryGroups());
        return record(intent, { kind: "Committed" });
      }
      case "ConfirmRecoveryKeyStored": {
        if (!st.recoveryKeys.delete(intent.keyOperationId)) return rejected("not_found");
        return record(intent, { kind: "Committed" });
      }
      case "CreateInvite": {
        st.invites.set(intent.operationId, {
          operationId: intent.operationId, link: `om:invite/${randomChars(B64URL, 22)}`, expiresAt: clock() + INVITE_TTL_MS,
        });
        return record(intent, { kind: "Committed" });
      }
      case "AcceptInvitation": {
        const parsed = parseInviteLink(intent.link);
        if (!parsed.ok) return rejected("invalid");
        if ([...st.invites.values()].some((i) => i.link === intent.link.trim())) return rejected("invalid"); // own invite
        if (st.acceptedInvites.has(parsed.token)) return rejected("conflict");
        st.acceptedInvites.add(parsed.token);
        return record(intent, { kind: "Committed" });
      }
      case "BlockPerson": {
        const p = st.persons.get(intent.personId);
        if (!p) return rejected("not_found");
        p.blocked = true;
        return record(intent, { kind: "Committed" });
      }
      default:
        return record(intent, { kind: "Committed" });
    }
  }

  return {
    marker: HARNESS_MARKER,

    seedProfile(seed) {
      if (profiles.has(seed.profileId)) throw new HarnessError("profile already seeded");
      const st: ProfileState = {
        label: seed.label, generation: asProfileGeneration(`gen-${String(profiles.size + 1)}`),
        conversations: new Map(), persons: new Map(), introductions: new Map(),
        identityCreated: false, recoveryKeys: new Map(), invites: new Map(), acceptedInvites: new Set(), home: seed.home ?? DEFAULT_HOME,
      };
      for (const c of seed.conversations ?? []) {
        if (conversationOwner.has(c.id)) throw new HarnessError("conversation id already seeded");
        conversationOwner.set(c.id, seed.profileId);
        st.persons.set(c.personId, { id: c.personId, displayName: c.displayName, blocked: false });
        const messages: MutableMessage[] = (c.messages ?? []).map((m) => {
          const messageId = asMessageId(`msg-${String(++messageSeq)}`);
          messageOwner.set(messageId, seed.profileId);
          return { messageId, operationId: null, direction: m.direction, text: m.text, sentAt: clock(), delivery: { state: "delivered" } };
        });
        st.conversations.set(c.id, { id: c.id, personId: c.personId, displayName: c.displayName, messages });
      }
      for (const i of seed.introductions ?? []) st.introductions.set(i.id, i);
      profiles.set(seed.profileId, st);
      return captureProfileContext(seed.profileId, st.generation);
    },

    contextFor(profileId) {
      const st = profiles.get(profileId);
      if (!st) throw new ProfileAccessError("unknown profile");
      return captureProfileContext(profileId, st.generation);
    },

    injectNetworkTimeoutAfterSend(operationId) {
      if (opLog.has(operationId)) throw new HarnessError("fault must be armed before the operation is dispatched");
      armedTimeouts.add(operationId);
    },

    simulateRestart() {
      observers = new Map();
      subscribers = new Set();
    },

    resolveReconciliation(operationId, outcome) {
      const rec = opLog.get(operationId);
      if (rec?.outcome.kind !== "PendingReconciliation") throw new HarnessError("operation is not pending reconciliation");
      rec.outcome = outcome;
      if (rec.messageId !== null) {
        const msg = [...(profiles.get(rec.profileId)?.conversations.values() ?? [])].flatMap((c) => c.messages).find((m) => m.messageId === rec.messageId);
        if (msg) msg.delivery = outcome.kind === "Committed" ? { state: "sent" } : { state: "rejected", reason: outcome.reason };
      }
      notify(rec.profileId, operationId);
    },

    dispatch(intent) {
      return settle(() => {
        const st = stateFor(intent.ctx);
        const existing = opLog.get(intent.operationId);
        if (existing) {
          if (existing.profileId !== intent.ctx.profileId || existing.intent.kind !== intent.kind) throw new OperationIdConflictError("operation id reused for a different intent");
          return handleOf(existing);
        }
        const before = opLog.size;
        const rec = apply(intent, st);
        if (opLog.size !== before) notify(rec.profileId, rec.operationId);
        return handleOf(rec);
      });
    },

    observe(ctx, operationId, listener): Unsubscribe {
      stateFor(ctx);
      const rec = opLog.get(operationId);
      if (rec && rec.profileId !== ctx.profileId) throw new ProfileAccessError("operation belongs to another profile");
      const set = observers.get(operationId) ?? new Set();
      set.add(listener);
      observers.set(operationId, set);
      if (rec) listener(rec.outcome);
      return () => { set.delete(listener); };
    },

    query: <Q extends CoreQuery>(q: Q) => settle(() => runQuery(q)),

    subscribe<Q extends CoreQuery>(selector: Q, listener: (r: QueryResult<Q>) => void): Unsubscribe {
      if (selector.kind !== "GetProfileList") stateFor(selector.ctx);
      const entry = { selector, listener: listener as (r: never) => void };
      subscribers.add(entry);
      return () => { subscribers.delete(entry); };
    },

    pendingOperations: (ctx) => settle((): readonly PendingOperationView[] => {
      stateFor(ctx);
      return [...opLog.values()]
        .filter((r) => r.profileId === ctx.profileId)
        .map((r) => ({ operationId: r.operationId, intentKind: r.intent.kind, outcome: r.outcome }));
    }),
  };
}
