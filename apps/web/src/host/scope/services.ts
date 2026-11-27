// SPDX-License-Identifier: MPL-2.0
import type { WasmCore } from "@om/core-wasm";
import { captureProfileContext, type ConversationId, type OutcomeClass, type ProfileExecutionContext, type ProfileId, type RejectReason } from "@om/platform-api";
import type { RoomControlEntry, ScopeEngines, ServiceListing } from "../../../../../packages/om-core-wasm/src/scope-engines";
import type {
  ConnectedServicesService, ConnectedServicesView, ServiceAction, ServiceEntryView, ServiceFlowView,
  ServiceDisclosureItem, ServiceIntent, ServiceInvocationView, ServiceReadAccess,
} from "../../../../../packages/om-ui/src/services/services";
import { denyAll, type RoomAuthority } from "./bridge";

const installCapability = "room.install_service";
type RoomServiceChange = "install" | "remove";
type ServiceChange = RoomServiceChange | "invoke";
type SelectedContext = Extract<ServiceInvocationView["disclosure"], { readonly kind: "selected_context" }>;

export interface InvocationSelection extends Omit<SelectedContext, "kind"> {
  readonly service: string;
  /** The exact typed broker tool name. */
  readonly command: string;
}

/** Reads the pending user selection, including the trigger and only the metadata selected for disclosure.
 * Keep the expiry fixed for the same service, command, destination and item list.
 */
export type SelectionSource = (ctx: ProfileExecutionContext, conversationId: ConversationId) => InvocationSelection | null;

type BrokerEgressPolicy =
  | { readonly kind: "room"; readonly allowed: boolean }
  | { readonly kind: "one_to_one"; readonly me: boolean; readonly peer: boolean };

export interface ServiceInvocationOptions {
  readonly core: Pick<WasmCore, "call">;
  /** Effective grants for this installed incarnation, from verified state. Offer capabilities are declarations. */
  readonly installedGrants?: (
    ctx: ProfileExecutionContext, conversationId: ConversationId, installation: { readonly service: string; readonly target: string },
  ) => readonly string[];
  /** Verified plaintext egress policy. Omission denies plaintext egress. */
  readonly egressPolicy?: (ctx: ProfileExecutionContext, conversationId: ConversationId) => BrokerEgressPolicy;
  /** Wall clock in Unix milliseconds, used only to check the selection's expiry. */
  readonly wallNow?: () => number;
}

export interface PendingServiceOperation {
  readonly id: string;
  readonly conversationId: ConversationId;
  readonly service: string;
  readonly intent: ServiceIntent<ServiceChange>;
}

/** The host owns durable, profile-private storage. Reserve and clear must be atomic for a Room. */
export interface PendingServiceStore {
  load(profileId: ProfileId, conversationId: ConversationId): Promise<PendingServiceOperation | null>;
  reserve(profileId: ProfileId, operation: PendingServiceOperation): Promise<boolean>;
  clear(profileId: ProfileId, conversationId: ConversationId, operationId: string): Promise<void>;
}

export interface ConnectedServicesOptions {
  readonly ctx: ProfileExecutionContext;
  readonly engines: Pick<ScopeEngines, "roomServices" | "roomControl">;
  readonly authority?: RoomAuthority;
  readonly selectionSource?: SelectionSource;
  readonly invocation?: ServiceInvocationOptions;
  readonly pending: PendingServiceStore;
  /** Monotonic runtime clock. Head observations are already timed by Core. */
  readonly now: () => number;
}

export function serviceRoomHandle(profileId: ProfileId, conversationId: ConversationId): string {
  return JSON.stringify([profileId, conversationId]);
}

const refusalReasons: Readonly<Partial<Record<string, RejectReason>>> = {
  already_installed: "conflict",
  malformed: "invalid",
  not_authorized: "unauthorized",
  not_installed: "not_found",
  quorum_not_met: "unauthorized",
  rollback: "stale_state",
  stale_target: "stale_state",
  too_many_offers: "quota",
  unknown_handle: "unavailable",
  unverified_head: "stale_state",
  wrong_service: "invalid",
};

const committed = { kind: "Committed" } as const;
const reconciling = { kind: "PendingReconciliation" } as const;
const pendingNetwork = { kind: "PendingNetwork" } as const;
const rejected = (reason: RejectReason): OutcomeClass => ({ kind: "Rejected", reason });

const brokerRefusalReasons: Readonly<Partial<Record<string, RejectReason>>> = {
  forbidden: "unauthorized",
  unknown_tool: "unsupported",
  not_granted: "unauthorized",
  tainted_egress: "policy_denied",
};

const roomWork = new WeakMap<PendingServiceStore, Map<string, Promise<unknown>>>();

function inRoom<T>(store: PendingServiceStore, room: string, work: () => Promise<T>): Promise<T> {
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

async function digestHex(input: string): Promise<string> {
  const digest = await globalThis.crypto.subtle.digest("SHA-256", new TextEncoder().encode(input));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function operationId(profileId: ProfileId, conversationId: ConversationId, intent: ServiceIntent<ServiceChange>): Promise<string> {
  return digestHex(JSON.stringify([profileId, conversationId, intent.kind, intent.target]));
}

function reviewInput({ service, command, destination, items }: InvocationSelection): string {
  return JSON.stringify([service, command, destination, items.map(({ id, kind, value }) => [id, kind, value])]);
}

function brokerVerdict(
  options: ServiceInvocationOptions | undefined, granted: readonly string[], policy: BrokerEgressPolicy, name: string,
): { readonly flow: ServiceFlowView; readonly reason: RejectReason | null } {
  // A remote selected-context review is conservatively tainted. The broker takes taint as input and returns a verdict.
  const tainted = true;
  let allowed = false;
  let refusal: string | undefined;
  try {
    const answer = options?.core.call({ cmd: "service_broker_request", granted, tainted, policy, name });
    if (typeof answer === "object" && answer !== null && "allowed" in answer) {
      allowed = answer.allowed === true && granted.includes(name);
      if (answer.allowed === false && "refusal" in answer && typeof answer.refusal === "string") refusal = answer.refusal;
    }
  } catch {
    // A missing or unavailable broker cannot authorize a review.
  }
  // The current broker exposes one egress tool and reports conversation denial as a distinct refusal.
  let egress: ServiceFlowView["egress"] = "not_requested";
  if (refusal === "tainted_egress") egress = "blocked_by_conversation";
  else if (name === "fetch_url_via_scoped_network_provider") egress = allowed ? "allowed" : "blocked_by_capability";
  const knownRefusal = refusal !== undefined && Object.hasOwn(brokerRefusalReasons, refusal)
    ? brokerRefusalReasons[refusal] : undefined;
  const reason = allowed ? null : knownRefusal ?? "unavailable";
  return { flow: { tainted, egress }, reason };
}

function matchesEffect(services: readonly ServiceListing[], operation: PendingServiceOperation): boolean {
  const membership = operation.intent.kind === "install" ? "installed" : "removed";
  return services.some((service) => service.service === operation.service &&
    service.target === operation.intent.target && service.membership === membership);
}

function matchesHead(service: ServiceListing, head: RoomControlEntry | undefined): boolean {
  const basis = service.admissionBasis;
  return basis !== undefined && head !== undefined && !head.frozen &&
    basis.serviceId === service.service && basis.version === head.headVersion && basis.hash === head.headHash &&
    basis.endpoints.length === head.endpoints.length && basis.endpoints.some((endpoint) => endpoint.state === 1) &&
    basis.endpoints.every(({ id, state }) => head.endpoints.some((endpoint) =>
      endpoint.id === id && endpoint.state === state && (state !== 1 || endpoint.usable)));
}

function installedStatus(service: ServiceListing, head: RoomControlEntry | undefined): "installed" | "revoked" {
  const headVersion = head?.headVersion;
  const { admittedVersion, heldVersion } = service;
  if (head === undefined || headVersion == null || head.frozen || admittedVersion === undefined ||
      headVersion < Math.max(admittedVersion, heldVersion ?? 0)) return "installed";
  const admitted = service.admissionBasis?.endpoints.filter((endpoint) => endpoint.state === 1).map((endpoint) => endpoint.id)
    ?? service.endpoints ?? [];
  const endpoints = new Map(head.endpoints.map((endpoint) => [endpoint.id, endpoint.state]));
  return admitted.length > 0 && admitted.every((id) =>
    endpoints.get(id) === 3 || (!endpoints.has(id) && headVersion > admittedVersion))
    ? "revoked" : "installed";
}

function readAccess(service: ServiceListing): ServiceReadAccess {
  const { endpoints, filterClasses, admittedVersion, heldVersion } = service;
  if (!service.installed || service.canReadAllContent !== true || endpoints === undefined || admittedVersion === undefined) {
    return { kind: "none" };
  }
  return {
    kind: "full_room_member",
    roster: {
      installed: true, canReadAllContent: true, endpoints, filterClasses: filterClasses ?? null,
      admittedVersion, heldVersion: heldVersion ?? null,
    },
  };
}

export function createConnectedServicesService({
  ctx: inputCtx, engines, authority = denyAll, selectionSource = () => null, invocation, pending,
}: ConnectedServicesOptions): ConnectedServicesService {
  const ctx = captureProfileContext(inputCtx.profileId, inputCtx.generation);

  function read(conversationId: ConversationId) {
    // Validate the captured context before querying the handle-based engine.
    const { entries } = engines.roomControl.view(ctx, conversationId);
    const listed = engines.roomServices.list(serviceRoomHandle(ctx.profileId, conversationId));
    if ("refused" in listed) throw new Error("Service Room view unavailable");
    return { services: listed.services, heads: new Map(entries.filter((entry) => entry.kind === "service").map((entry) => [entry.id, entry])) };
  }
  type Room = ReturnType<typeof read>;

  async function loadPending(conversationId: ConversationId): Promise<PendingServiceOperation | null> {
    const operation = await pending.load(ctx.profileId, conversationId);
    if (operation !== null && (operation.conversationId !== conversationId ||
        !["install", "remove", "invoke"].includes(operation.intent.kind) ||
        operation.id !== await operationId(ctx.profileId, conversationId, operation.intent))) {
      throw new Error("Service pending operation does not belong to this Room");
    }
    return operation;
  }

  function invocationView(
    conversationId: ConversationId, room: Room, selection: InvocationSelection, id: string, busy: boolean,
  ): ServiceInvocationView {
    const { service, command, destination, expiresAtMs, items } = selection;
    const entry = room.services.find((row) => row.service === service);
    const head = room.heads.get(service);
    const installed = entry?.membership === "installed" && entry.installed &&
      head?.frozen !== true && installedStatus(entry, head) === "installed";
    const caps = authority.capabilities(ctx, conversationId);
    const granted = installed && caps.includes(command)
      ? entry.capabilities ?? (entry.target === null ? [] :
        invocation?.installedGrants?.(ctx, conversationId, { service, target: entry.target }) ?? [])
      : [];
    const policy = invocation?.egressPolicy?.(ctx, conversationId) ?? { kind: "room", allowed: false };
    const { flow, reason: brokerReason } = brokerVerdict(invocation, granted, policy, command);
    const expired = !Number.isSafeInteger(expiresAtMs) || expiresAtMs <= (invocation?.wallNow ?? Date.now)();
    const reason = expired ? "expired" : brokerReason ?? (busy ? "conflict" : null);
    return {
      id, principal: { id: service, name: entry?.name || service }, command,
      disclosure: { kind: "selected_context", destination, expiresAtMs, items },
      flow,
      action: reason === null ? { availability: "ready", intent: { kind: "invoke", target: id } }
        : { availability: "blocked", kind: "invoke", reason },
    };
  }

  function readSelection(conversationId: ConversationId): InvocationSelection | null {
    const selection = selectionSource(ctx, conversationId);
    if (selection === null) return null;
    const selectedItems: readonly ServiceDisclosureItem[] = selection.items;
    const [first, ...rest] = selectedItems;
    if (first === undefined) return null;
    const copy = ({ id, kind, value }: ServiceDisclosureItem): ServiceDisclosureItem => ({ id, kind, value });
    return { ...selection, items: [copy(first), ...rest.map(copy)] };
  }

  async function view(
    conversationId: ConversationId, room: Room, outcome: OutcomeClass | null,
    busy = outcome?.kind === "PendingNetwork" || outcome?.kind === "PendingReconciliation",
  ): Promise<ConnectedServicesView> {
    const selection = readSelection(conversationId);
    const review = selection === null ? null : invocationView(
      conversationId, room, selection, await digestHex(reviewInput(selection)), busy,
    );
    return render(conversationId, room, outcome, busy, review);
  }

  function render(
    conversationId: ConversationId, room: Room, outcome: OutcomeClass | null, busy: boolean,
    review: ServiceInvocationView | null,
  ): ConnectedServicesView {
    const allowed = authority.capabilities(ctx, conversationId).includes(installCapability);
    const entries = room.services.map((service): ServiceEntryView => {
      const { membership, target } = service;
      const head = room.heads.get(service.service);
      const actions: ServiceAction<"install" | "revoke" | "remove">[] = [];
      const status = membership === "installed" ? installedStatus(service, head)
        : membership === "proposed" ? "offered" : "removed";
      if (membership !== "removed" && target !== null && !busy) {
        const kind = membership === "proposed" ? "install" : "remove";
        if (!allowed) actions.push({ availability: "blocked", kind, reason: "unauthorized" });
        else if (kind === "install" && !matchesHead(service, head)) {
          actions.push({ availability: "blocked", kind, reason: "stale_state" });
        } else actions.push({ availability: "ready", intent: { kind, target } });
      }
      if (status === "installed" && !busy) {
        // Revoking an action grant needs a separate authority and target from Room removal.
        actions.push({ availability: "blocked", kind: "revoke", reason: allowed ? "unsupported" : "unauthorized" });
      }
      return {
        id: target ?? service.service,
        principal: { id: service.service, name: service.name || service.service },
        status,
        scope: { kind: "room", name: "—" },
        mode: "full_room_member",
        capabilities: (service.capabilities ?? []).map((id) => ({ id, label: id })),
        readAccess: readAccess(service),
        actions,
      };
    });
    return { entries, invocation: review, outcome };
  }

  async function reconcile(conversationId: ConversationId, room: Room, operation: PendingServiceOperation) {
    if (operation.intent.kind === "invoke") return view(conversationId, room, pendingNetwork);
    let outcome: OutcomeClass = reconciling;
    if (matchesEffect(room.services, operation)) {
      outcome = committed;
    } else {
      const handle = serviceRoomHandle(ctx.profileId, conversationId);
      const caps = authority.capabilities(ctx, conversationId);
      const { kind, target } = operation.intent;
      const input = { service: operation.service, target };
      const offer = room.services.find((service) => service.service === input.service &&
        service.target === target && service.membership === "proposed");
      if (!caps.includes(installCapability)) {
        outcome = rejected("unauthorized");
      } else if (kind === "install" && offer !== undefined && !matchesHead(offer, room.heads.get(input.service))) {
        outcome = rejected("stale_state");
      } else {
        try {
          const result = kind === "install"
            ? engines.roomServices.installOffer(handle, caps, input, ctx)
            : engines.roomServices.removeMember(handle, caps, input);
          if ("refused" in result) outcome = rejected(refusalReasons[result.refused] ?? "unavailable");
        } catch {
          // The command may have applied before its result arrived.
        }
        try {
          room = read(conversationId);
        } catch {
          return view(conversationId, room, reconciling);
        }
        if (matchesEffect(room.services, operation)) outcome = committed;
      }
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

  async function invoke(conversationId: ConversationId, target: string): Promise<ConnectedServicesView> {
    const existing = await loadPending(conversationId);
    const selection = readSelection(conversationId);
    if (selection === null) {
      const reason = invocation === undefined ? "unsupported" : "stale_state";
      return render(conversationId, read(conversationId), rejected(reason), existing !== null, null);
    }
    const input = reviewInput(selection);
    const intent: ServiceIntent<"invoke"> = { kind: "invoke", target };
    const [reviewId, id] = await Promise.all([digestHex(input), operationId(ctx.profileId, conversationId, intent)]);
    // Read again after hashing so changed selections or Room facts cannot authorize the earlier target.
    const room = read(conversationId);
    const current = readSelection(conversationId);
    if (target !== reviewId || current === null || reviewInput(current) !== input || current.expiresAtMs !== selection.expiresAtMs) {
      return view(conversationId, room, rejected("stale_state"), existing !== null);
    }
    if (existing?.id === id) return view(conversationId, room, pendingNetwork);
    const review = invocationView(conversationId, room, current, reviewId, existing !== null);
    if (review.action.availability === "blocked") {
      return render(conversationId, room, rejected(review.action.reason), existing !== null, review);
    }
    // Only the operation identity is durable. There is no session through which to send the reviewed plaintext.
    if (await pending.reserve(ctx.profileId, { id, conversationId, service: current.service, intent })) {
      return view(conversationId, room, pendingNetwork);
    }
    const winner = await loadPending(conversationId);
    if (winner?.id === id) return view(conversationId, room, pendingNetwork);
    return winner === null ? view(conversationId, room, reconciling)
      : view(conversationId, room, rejected("conflict"), true);
  }

  return {
    async state(conversationId) {
      if (conversationId === undefined) return { entries: [], invocation: null, outcome: null };
      return inRoom(pending, serviceRoomHandle(ctx.profileId, conversationId), async () => {
        const operation = await loadPending(conversationId);
        const room = read(conversationId);
        return operation === null ? view(conversationId, room, null) : reconcile(conversationId, room, operation);
      });
    },
    async change(conversationId, { kind, target }) {
      if (conversationId === undefined) return { entries: [], invocation: null, outcome: rejected("unsupported") };
      return inRoom(pending, serviceRoomHandle(ctx.profileId, conversationId), async () => {
        if (kind === "invoke") return invoke(conversationId, target);
        if (kind === "revoke") {
          const room = read(conversationId);
          const allowed = authority.capabilities(ctx, conversationId).includes(installCapability);
          return view(conversationId, room, rejected(allowed ? "unsupported" : "unauthorized"));
        }
        if (!["install", "remove"].includes(kind)) return view(conversationId, read(conversationId), rejected("invalid"));
        const existing = await loadPending(conversationId);
        let room = read(conversationId);
        if (existing?.intent.kind === "invoke") return view(conversationId, room, rejected("conflict"), true);
        if (existing !== null) return reconcile(conversationId, room, existing);

        const membership = kind === "install" ? "proposed" : "installed";
        const service = room.services.find((entry) => entry.target === target && entry.membership === membership);
        if (service === undefined) return view(conversationId, room, rejected("stale_state"));
        if (!authority.capabilities(ctx, conversationId).includes(installCapability)) {
          return view(conversationId, room, rejected("unauthorized"));
        }
        if (kind === "install" && !matchesHead(service, room.heads.get(service.service))) {
          return view(conversationId, room, rejected("stale_state"));
        }

        const intent: ServiceIntent<RoomServiceChange> = { kind, target };
        const id = await operationId(ctx.profileId, conversationId, intent);
        const operation = { id, conversationId, service: service.service, intent };
        const owned = await pending.reserve(ctx.profileId, operation);
        try {
          room = read(conversationId);
        } catch {
          return view(conversationId, room, reconciling);
        }
        return owned ? reconcile(conversationId, room, operation) : view(conversationId, room, reconciling);
      });
    },
  };
}
