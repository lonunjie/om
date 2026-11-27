// SPDX-License-Identifier: MPL-2.0
import type { WasmCore } from "@om/core-wasm";
import { captureProfileContext, type ConversationId, type OutcomeClass, type ProfileExecutionContext, type ProfileId, type RejectReason } from "@om/platform-api";
import type { RoomControlEntry, ScopeEngines, ServiceListing } from "../../../../../packages/om-core-wasm/src/scope-engines";
import type {
  ConnectedServicesService, ConnectedServicesView, ServiceAction, ServiceEntryView, ServiceFlowView,
  ServiceDisclosureItem, ServiceIntent, ServiceInvocationView, ServiceReadAccess,
} from "../../../../../packages/om-ui/src/services/services";
import { denyAll, roomEventClasses, type RoomAuthority } from "./bridge";

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
 * The review includes this selection’s expiry.
 */
export type SelectionSource = (ctx: ProfileExecutionContext, conversationId: ConversationId) => InvocationSelection | null;

export interface ServiceInvocationOptions {
  readonly core: Pick<WasmCore, "call">;
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
  malformed: "invalid",
  expired: "expired",
  not_authorized: "unauthorized",
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

function operationId(profileId: ProfileId, conversationId: ConversationId, intent: ServiceIntent): Promise<string> {
  return digestHex(JSON.stringify([profileId, conversationId, intent.kind, intent.target]));
}

function reviewInput({ service, command, destination, expiresAtMs, items }: InvocationSelection): string {
  return JSON.stringify([service, command, destination, expiresAtMs, items.map(({ id, kind, value }) => [id, kind, value])]);
}

function brokerVerdict(
  options: ServiceInvocationOptions | undefined, ctx: ProfileExecutionContext, conversationId: ConversationId,
  disclosure: InvocationSelection, target: string, eligible: boolean,
): { readonly flow: ServiceFlowView; readonly reason: RejectReason | null } {
  const egress = disclosure.command === "fetch_url_via_scoped_network_provider" ? "blocked_by_capability" : "not_requested";
  const denied = { flow: { tainted: true, egress } } as const;
  try {
    const answer = options?.core.call({ cmd: "service_invocation_review", ctx, conversationId, disclosure }) as {
      target?: string; allowed?: boolean; refusal?: string; refused?: string; flow?: ServiceFlowView;
    } | undefined;
    if (answer?.target === target && answer.flow !== undefined && answer.flow.tainted === true &&
        ["not_requested", "blocked_by_conversation", "blocked_by_capability", "allowed"].includes(answer.flow.egress)) {
      if (answer.allowed === true && eligible) return { flow: answer.flow, reason: null };
      const refusal = answer.refusal;
      const reason = refusal !== undefined && Object.hasOwn(brokerRefusalReasons, refusal)
        ? brokerRefusalReasons[refusal] : undefined;
      return { flow: answer.allowed === false ? answer.flow : denied.flow, reason: reason ?? "unauthorized" };
    }
    if (answer?.refused === "not_authorized") return { ...denied, reason: "unauthorized" };
  } catch {
    // An unavailable Core cannot authorize a review.
  }
  return { ...denied, reason: "unavailable" };
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

function knownTriggerClasses(service: ServiceListing): boolean {
  return service.filterClasses?.every((code) => roomEventClasses[code - 1] !== undefined) ?? true;
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
    const listed = engines.roomServices.list(serviceRoomHandle(ctx.profileId, conversationId), ctx);
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
    const eligible = installed && entry?.target != null;
    const { flow, reason: brokerReason } = brokerVerdict(invocation, ctx, conversationId, selection, id, eligible);
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
    const { service, command, destination, expiresAtMs } = selection;
    return { service, command, destination, expiresAtMs, items: [copy(first), ...rest.map(copy)] };
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
        } else if (kind === "install" && !knownTriggerClasses(service)) {
          actions.push({ availability: "blocked", kind, reason: "unsupported" });
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
        ...(service.filterClasses === undefined ? {} : {
          triggerClasses: service.filterClasses?.flatMap((code) => {
            const event = roomEventClasses[code - 1];
            return event === undefined ? [] : [event];
          }) ?? null,
        }),
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
            : engines.roomServices.removeMember(handle, caps, input, ctx);
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
    try {
      return await view(conversationId, room, outcome);
    } catch {
      return { entries: [], invocation: null, outcome };
    }
  }

  async function invoke(conversationId: ConversationId, intent: ServiceIntent<"invoke">, id: string,
    settleFailureAs: (outcome: OutcomeClass) => void): Promise<ConnectedServicesView> {
    const { target } = intent;
    const selection = readSelection(conversationId);
    if (selection === null) {
      const reason = invocation === undefined ? "unsupported" : "stale_state";
      return render(conversationId, read(conversationId), rejected(reason), false, null);
    }
    const input = reviewInput(selection);
    const reviewId = await digestHex(input);
    // Read again after hashing so changed selections or Room facts cannot authorize the earlier target.
    const room = read(conversationId);
    const current = readSelection(conversationId);
    if (target !== reviewId || current === null || reviewInput(current) !== input) {
      return view(conversationId, room, rejected("stale_state"));
    }
    const review = invocationView(conversationId, room, current, reviewId, false);
    if (review.action.availability === "blocked") {
      return render(conversationId, room, rejected(review.action.reason), false, review);
    }
    // Only the operation identity is durable. There is no session through which to send the reviewed plaintext.
    settleFailureAs(reconciling);
    if (await pending.reserve(ctx.profileId, { id, conversationId, service: current.service, intent })) {
      settleFailureAs(pendingNetwork);
      return view(conversationId, room, pendingNetwork);
    }
    settleFailureAs(rejected("unavailable"));
    const winner = await loadPending(conversationId);
    const outcome = winner?.id === id ? pendingNetwork
      : rejected(winner === null ? "unavailable" : "conflict");
    settleFailureAs(outcome);
    return view(conversationId, room, outcome, winner !== null);
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
        let fallback = rejected("unavailable");
        try {
          const intent = { kind, target };
          const id = await operationId(ctx.profileId, conversationId, intent);
          const existing = await loadPending(conversationId);
          if (existing !== null) {
            fallback = existing.id !== id ? rejected("conflict")
              : existing.intent.kind === "invoke" ? pendingNetwork : reconciling;
            const room = read(conversationId);
            return existing.id === id ? await reconcile(conversationId, room, existing)
              : await view(conversationId, room, fallback, true);
          }
          if (kind === "invoke") {
            return await invoke(conversationId, { kind, target }, id, (outcome) => { fallback = outcome; });
          }
          const room = read(conversationId);
          const allowed = authority.capabilities(ctx, conversationId).includes(installCapability);
          if (kind === "revoke") return await view(conversationId, room, rejected(allowed ? "unsupported" : "unauthorized"));
          if (kind !== "install" && kind !== "remove") return await view(conversationId, room, rejected("invalid"));

          const membership = kind === "install" ? "proposed" : "installed";
          const service = room.services.find((entry) => entry.target === target && entry.membership === membership);
          if (service === undefined) return await view(conversationId, room, rejected("stale_state"));
          if (!allowed) return await view(conversationId, room, rejected("unauthorized"));
          if (kind === "install" && !matchesHead(service, room.heads.get(service.service))) {
            return await view(conversationId, room, rejected("stale_state"));
          }

          if (kind === "install" && !knownTriggerClasses(service)) {
            return await view(conversationId, room, rejected("unsupported"));
          }
          const operation = { id, conversationId, service: service.service, intent: { kind, target } };
          fallback = reconciling;
          if (await pending.reserve(ctx.profileId, operation)) {
            return await reconcile(conversationId, read(conversationId), operation);
          }
          fallback = rejected("unavailable");
          const winner = await loadPending(conversationId);
          if (winner?.id === id) {
            fallback = reconciling;
            return await reconcile(conversationId, read(conversationId), winner);
          }
          if (winner !== null) fallback = rejected("conflict");
          return await view(conversationId, room, fallback, winner !== null);
        } catch {
          return { entries: [], invocation: null, outcome: fallback };
        }
      });
    },
  };
}
