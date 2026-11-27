// SPDX-License-Identifier: MPL-2.0
import { captureProfileContext, type ConversationId, type OutcomeClass, type ProfileExecutionContext, type ProfileId, type RejectReason } from "@om/platform-api";
import type {
  BridgeDirection, BridgeEntryView, BridgeEventClass, BridgeIntent, BridgeMappingState, BridgeRoomView, BridgeService,
} from "../../../../../packages/om-ui/src/bridges/bridges";
import type { BridgeListing, RoomControlEntry, ScopeEngines } from "../../../../../packages/om-core-wasm/src/scope-engines";

const bridgeCapability = "room.add_bridge";

export interface RoomAuthority {
  capabilities(ctx: ProfileExecutionContext, conversationId: ConversationId): readonly string[];
}

export const denyAll: RoomAuthority = Object.freeze({ capabilities: () => [] });

export interface PendingBridgeOperation {
  readonly id: string;
  readonly conversationId: ConversationId;
  readonly bridge: string;
  readonly intent: BridgeIntent;
}

/** The host owns durable, profile-private storage. Reserve and clear must be atomic for a Room. */
export interface PendingStore {
  load(profileId: ProfileId, conversationId: ConversationId): Promise<PendingBridgeOperation | null>;
  reserve(profileId: ProfileId, operation: PendingBridgeOperation): Promise<boolean>;
  clear(profileId: ProfileId, conversationId: ConversationId, operationId: string): Promise<void>;
}

export interface BridgeServiceOptions {
  readonly ctx: ProfileExecutionContext;
  readonly engines: Pick<ScopeEngines, "bridgeRoom" | "roomControl">;
  readonly authority?: RoomAuthority;
  readonly pending: PendingStore;
  /** Monotonic runtime time, in the same units as the scope engine's Room observations. */
  readonly now: () => number;
  /** The Room's configured bridge head freshness bound. Unknown bounds leave head freshness unverified. */
  readonly freshnessBoundMs?: number;
}

/** The scope module's bridge handles are otherwise shared across profiles. Use the same handle when opening a Room. */
export function bridgeRoomHandle(profileId: ProfileId, conversationId: ConversationId): string {
  return JSON.stringify([profileId, conversationId]);
}

const directions: Readonly<Partial<Record<number, BridgeDirection>>> = { 1: "outbound", 2: "inbound", 3: "both" };
// The event_kind registry in spec/registries/om-numeric-fields.json.
const eventClasses: readonly BridgeEventClass[] = [
  "message.create", "message.edit", "message.delete", "reaction.set", "receipt.update", "pin.set",
  "moderation.redact", "poll.create", "poll.vote", "poll.close", "topic.create", "topic.update",
  "topic.status.set", "message.topic.set", "group.invite", "group.accept", "group.decline", "group.rejoin_request",
];

const refusalReasons: Readonly<Partial<Record<string, RejectReason>>> = {
  already_installed: "conflict",
  bad_direction: "invalid",
  empty_classes: "invalid",
  malformed: "invalid",
  no_mapping: "not_found",
  not_authorized: "unauthorized",
  not_installed: "not_found",
  not_revoked: "conflict",
  quorum_not_met: "unauthorized",
  rollback: "stale_state",
  stale_target: "stale_state",
  too_early: "stale_state",
  too_many_offers: "quota",
  unknown_handle: "unavailable",
  wrong_service: "invalid",
};

const committed = { kind: "Committed" } as const;
const reconciling = { kind: "PendingReconciliation" } as const;
const rejected = (reason: RejectReason): OutcomeClass => ({ kind: "Rejected", reason });

const roomWork = new WeakMap<PendingStore, Map<string, Promise<unknown>>>();

/** A single host store serializes reads, reconciliation and changes for each Room. */
function inRoom<T>(store: PendingStore, room: string, work: () => Promise<T>): Promise<T> {
  let queue = roomWork.get(store);
  if (queue === undefined) {
    queue = new Map();
    roomWork.set(store, queue);
  }
  const own = (queue.get(room) ?? Promise.resolve()).catch(() => undefined).then(work);
  queue.set(room, own);
  const release = () => { if (queue.get(room) === own) queue.delete(room); };
  void own.then(release, release);
  return own;
}

async function operationId(profileId: ProfileId, conversationId: ConversationId, intent: BridgeIntent): Promise<string> {
  const input = new TextEncoder().encode(JSON.stringify([profileId, conversationId, intent.kind, intent.target]));
  const digest = await globalThis.crypto.subtle.digest("SHA-256", input);
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function mappingState(
  bridge: BridgeListing, head: RoomControlEntry | undefined, freshnessBoundMs: number | undefined,
): BridgeMappingState {
  if (bridge.mapping?.active !== true) return "revoked";
  if (head?.frozen) return "endpoint_stopped";
  if (!reviewSupported(bridge.mapping) || head?.headVersion == null || freshnessBoundMs === undefined ||
      !Number.isFinite(freshnessBoundMs) || head.observedAgeMs > freshnessBoundMs ||
      head.headVersion !== bridge.heldVersion) return "stale";
  const installedEndpoints = bridge.endpoints ?? [];
  return head.endpoints.some((endpoint) => endpoint.usable && installedEndpoints.includes(endpoint.id))
    ? "active" : "endpoint_stopped";
}

function matchesEffect(bridges: readonly BridgeListing[], operation: PendingBridgeOperation): boolean {
  const membership = operation.intent.kind === "admit" ? "member" : "removed";
  return bridges.some((bridge) => bridge.bridge === operation.bridge &&
    bridge.target === operation.intent.target && bridge.membership === membership);
}

function reviewSupported(mapping: BridgeListing["mapping"]): boolean {
  return mapping != null && directions[mapping.direction] !== undefined &&
    mapping.allowedClasses.every((code) => eventClasses[code - 1] !== undefined);
}

function validIntentKind(kind: string): boolean {
  return kind === "admit" || kind === "remove";
}

export function createBridgeService({
  ctx: inputCtx, engines, authority = denyAll, pending, now, freshnessBoundMs,
}: BridgeServiceOptions): BridgeService {
  const ctx = captureProfileContext(inputCtx.profileId, inputCtx.generation);

  function read(conversationId: ConversationId) {
    // This contextual read validates the generation before touching the unscoped handle API.
    const { entries } = engines.roomControl.view(ctx, conversationId);
    const listed = engines.bridgeRoom.list(bridgeRoomHandle(ctx.profileId, conversationId));
    if ("refused" in listed) throw new Error("Bridge Room view unavailable");
    return { bridges: listed.bridges, heads: new Map(entries.filter((entry) => entry.kind === "bridge").map((entry) => [entry.id, entry])) };
  }
  type Room = ReturnType<typeof read>;

  async function loadPending(conversationId: ConversationId): Promise<PendingBridgeOperation | null> {
    const operation = await pending.load(ctx.profileId, conversationId);
    if (operation !== null && (operation.conversationId !== conversationId ||
        !validIntentKind(operation.intent.kind) ||
        operation.id !== await operationId(ctx.profileId, conversationId, operation.intent))) {
      throw new Error("Bridge pending operation does not belong to this Room");
    }
    return operation;
  }

  function view(conversationId: ConversationId, room: Room, outcome: OutcomeClass | null): BridgeRoomView {
    const allowed = authority.capabilities(ctx, conversationId).includes(bridgeCapability);
    const bridges = room.bridges.map((bridge): BridgeEntryView => {
      const { membership, target, mapping } = bridge;
      const principal = { id: bridge.bridge, name: bridge.name || bridge.bridge };
      const kind = membership === "proposed" ? "admit" : "remove";
      const action = membership === "removed" || target === null ? null
        : !allowed ? { availability: "blocked", kind, reason: "unauthorized" } as const
        : outcome?.kind === "PendingReconciliation" ? null
        : kind === "admit" && !reviewSupported(mapping) ? { availability: "blocked", kind, reason: "unsupported" } as const
        : { availability: "ready", intent: { kind, target } } as const;
      const direction = mapping == null ? undefined : directions[mapping.direction];
      const state = mappingState(bridge, room.heads.get(bridge.bridge), freshnessBoundMs);
      const mappings = mapping == null || direction === undefined || target === null ? [] : [{
        // The scoped listing currently carries only the network namespace.
        id: target, network: mapping.namespace, scope: "—", direction,
        exportedClasses: direction === "inbound" || (membership === "member" && state !== "active") ? []
          : mapping.allowedClasses.flatMap((code) => {
            const name = eventClasses[code - 1];
            return name === undefined ? [] : [name];
          }),
        externalClaim: bridge.externalClaim ?? null,
      }];
      return membership === "member"
        ? { principal, membership, action, mappings: mappings.map((item) => ({ ...item, state })) }
        : { principal, membership, action, mappings };
    });
    return { bridges, outcome };
  }

  async function reconcile(conversationId: ConversationId, room: Room, operation: PendingBridgeOperation) {
    let outcome: OutcomeClass = reconciling;
    if (matchesEffect(room.bridges, operation)) {
      outcome = committed;
    } else {
      const handle = bridgeRoomHandle(ctx.profileId, conversationId);
      const caps = authority.capabilities(ctx, conversationId);
      const input = { bridge: operation.bridge, target: operation.intent.target };
      try {
        const result = operation.intent.kind === "admit"
          ? engines.bridgeRoom.admitOffer(handle, caps, { ...input, now: now() })
          : engines.bridgeRoom.removeMember(handle, caps, input);
        if ("refused" in result) outcome = rejected(refusalReasons[result.refused] ?? "unavailable");
      } catch {
        // The command may have applied before the response was lost.
      }
      try {
        room = read(conversationId);
      } catch {
        return view(conversationId, room, reconciling);
      }
      if (matchesEffect(room.bridges, operation)) outcome = committed;
    }
    if (outcome.kind !== "PendingReconciliation") {
      try {
        await pending.clear(ctx.profileId, conversationId, operation.id);
      } catch {
        outcome = reconciling;
      }
    }
    return view(conversationId, room, outcome);
  }

  return {
    state: (conversationId) => inRoom(pending, bridgeRoomHandle(ctx.profileId, conversationId), async () => {
      const operation = await loadPending(conversationId);
      const room = read(conversationId);
      return operation === null ? view(conversationId, room, null) : reconcile(conversationId, room, operation);
    }),
    change(conversationId, { kind, target }) {
      const intent: BridgeIntent = { kind, target };
      return inRoom(pending, bridgeRoomHandle(ctx.profileId, conversationId), async () => {
        const existing = await loadPending(conversationId);
        let room = read(conversationId);
        if (existing !== null) return reconcile(conversationId, room, existing);

        if (!validIntentKind(kind)) return view(conversationId, room, rejected("invalid"));
        const membership = kind === "admit" ? "proposed" : "member";
        const bridge = room.bridges.find((entry) => entry.target === target && entry.membership === membership);
        if (bridge === undefined) return view(conversationId, room, rejected("stale_state"));
        if (!authority.capabilities(ctx, conversationId).includes(bridgeCapability)) {
          return view(conversationId, room, rejected("unauthorized"));
        }
        if (kind === "admit" && !reviewSupported(bridge.mapping)) {
          return view(conversationId, room, rejected("unsupported"));
        }

        const id = await operationId(ctx.profileId, conversationId, intent);
        const operation = { id, conversationId, bridge: bridge.bridge, intent };
        const owned = await pending.reserve(ctx.profileId, operation);
        try {
          room = read(conversationId);
        } catch {
          return view(conversationId, room, reconciling);
        }
        // A different host may have reserved the Room while the digest or store write was pending.
        if (!owned) return view(conversationId, room, reconciling);
        return reconcile(conversationId, room, operation);
      });
    },
  };
}
