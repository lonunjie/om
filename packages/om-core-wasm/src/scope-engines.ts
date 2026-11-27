// SPDX-License-Identifier: MPL-2.0
// Typed client for the engineering-only `scope-engines` module (om_wasm_scope.wasm, never bundled; the flag is off in every
// release). State lives in the module under caller-chosen handles; control heads are facts the caller already validated.
// This file adds no behaviour: it names the commands and their answers.
import type { ProfileExecutionContext } from "@om/platform-api";
import type { WasmCore } from "./wasm";

export type Refusal = { readonly refused: string };
export type Done = { readonly done: true };
export interface BridgeMember {
  readonly installed: boolean;
  readonly canReadAllContent?: boolean;
  readonly endpoints?: readonly string[];
  readonly filterClasses?: readonly number[] | null;
  readonly admittedVersion?: number;
  readonly heldVersion?: number | null;
  readonly mapping: { readonly namespace: string; readonly direction: number; readonly allowedClasses: readonly number[]; readonly active: boolean } | null;
}
export interface BridgeListing extends Omit<BridgeMember, "mapping"> {
  readonly bridge: string;
  readonly membership: "proposed" | "member" | "removed";
  /** Opaque review target for the current offer or member; null when none. */
  readonly target: string | null;
  readonly mapping?: BridgeMember["mapping"];
  readonly name?: string;
  readonly externalClaim?: string | null;
  readonly approversReportedAtProposal?: number;
}
export interface RoomControlEntry {
  readonly kind: "service" | "bridge";
  readonly id: string;
  readonly headVersion: number | null;
  readonly frozen: boolean;
  readonly endpoints: readonly { readonly id: string; readonly state: number; readonly usable: boolean }[];
  /** From the monotonic clock; the host compares it with its freshness bound. */
  readonly observedAgeMs: number;
  /** A governance fact the host reported at install, never a verified quorum. */
  readonly reportedApprovers: readonly Approver[];
}
export interface NearbyState { readonly enabled: boolean; readonly available: boolean }

export interface HeadFacts {
  readonly serviceId: string;
  readonly version: number;
  readonly hash: string;
  readonly endpoints: readonly { readonly id: string; readonly state: number }[];
}
export interface Approver { readonly kind: number; readonly id: string }
export interface Roster {
  readonly installed: boolean;
  readonly canReadAllContent?: true;
  readonly endpoints?: readonly string[];
  readonly filterClasses?: readonly number[] | null;
  readonly admittedVersion?: number;
  readonly heldVersion?: number | null;
}
export interface BridgeMapping {
  readonly incarnation: string;
  readonly namespace: string;
  readonly direction: 1 | 2 | 3;
  readonly allowedClasses: readonly number[];
}
export interface LinkState { readonly official: boolean; readonly spaceSide: boolean; readonly childSide: boolean; readonly bound: boolean }

export interface ScopeEngines {
  /** Room-member service path (one Room per handle). */
  readonly roomServices: {
    open(room: string, opts: { quorum: number | null; sequencingTimeout: number }): Done | Refusal;
    close(room: string): { readonly done: boolean };
    install(room: string, caps: readonly string[], a: { head: HeadFacts; incarnation: string; filterClasses: readonly number[] | null; approvers: readonly Approver[] }): Done | Refusal;
    observeHead(room: string, head: HeadFacts, now: number): Done | Refusal;
    mustStopSending(room: string, service: string, endpoint: string): boolean;
    mayCommitRemoval(room: string, a: { service: string; endpoint: string; isCoordinator: boolean; now: number }): Done | Refusal;
    removalCommitted(room: string, service: string, endpoint: string): boolean;
    remove(room: string, caps: readonly string[], service: string): { readonly removedEndpoints: number } | Refusal;
    view(room: string, service: string): Roster | Refusal;
  };
  readonly bridgeRoom: {
    open(room: string, opts: { quorum: number | null; sequencingTimeout: number; freshnessBound: number }): Done | Refusal;
    admit(room: string, caps: readonly string[], a: { head: HeadFacts; mapping: BridgeMapping; approvers: readonly Approver[]; now: number }): Done | Refusal;
    observeHead(room: string, head: HeadFacts, now: number): Done | Refusal;
    revokeMapping(room: string, caps: readonly string[], bridge: string): Done | Refusal;
    mayExport(room: string, a: { bridge: string; endpoint: string; class: number; now: number }): { readonly allowed: boolean; readonly blocked?: string } | Refusal;
    importAuthorOk(room: string, authorKind: number): boolean;
    /** One bridge as the Room holds it: roster entry plus its recorded mapping. */
    view(room: string, bridge: string): BridgeMember | Refusal;
    /** Every bridge the Room can show, in id order, with membership and review target (ADR-0270). */
    list(room: string): { readonly bridges: readonly BridgeListing[] } | Refusal;
    /** Places a proposed bridge. Grants nothing; returns the review target for this exact offer. */
    propose(room: string, a: { head: HeadFacts; mapping: BridgeMapping; approvers: readonly Approver[]; name: string; externalClaim: string | null }): { readonly target: string } | Refusal;
    withdrawOffer(room: string, bridge: string): { readonly done: boolean } | Refusal;
    /** Admits the offer the reviewer saw; refuses `stale_target` when the offer changed. */
    admitOffer(room: string, caps: readonly string[], a: { bridge: string; target: string; now: number }): Done | Refusal;
    removeMember(room: string, caps: readonly string[], a: { bridge: string; target: string }): { readonly removedEndpoints: number } | Refusal;
  };
  readonly lanTransfer: {
    openSender(handle: string, a: { blobId: string; digest: string; total: number }): Done | Refusal;
    openReceiver(handle: string, total: number, restoredOffset?: number): Done | Refusal;
    nextChunk(handle: string): { readonly chunk: { readonly offset: number; readonly len: number } | null; readonly paused: boolean } | Refusal;
    accept(handle: string, offset: number, len: number): { readonly acknowledged: number; readonly complete: boolean } | Refusal;
    transportAck(handle: string, upTo: number): Done | Refusal;
    /** Only an authenticated receipt confirms delivery. */
    receipt(handle: string, a: { authenticated: boolean; blobId: string; digest: string; length: number }): Done | Refusal;
  };
  /** Sealed WebRTC signalling for an accepted relationship. The pair seed stays inside Core: the host names the
   * conversation, never a key. */
  readonly lanSignal: {
    seal(ctx: ProfileExecutionContext, conversationId: string, a: { kind: "offer" | "answer"; sessionId: string; sdp: string }): { readonly envelope: string } | Refusal;
    open(ctx: ProfileExecutionContext, conversationId: string, envelope: string): { readonly kind: "offer" | "answer"; readonly sessionId: string; readonly sdp: string } | Refusal;
  };
  /** The per-contact "find each other on the same Wi-Fi" choice and the rendezvous tags. Core holds the choice and the seeds. */
  readonly lanNearby: {
    set(ctx: ProfileExecutionContext, conversationId: string, on: boolean): NearbyState;
    state(ctx: ProfileExecutionContext, conversationId: string): NearbyState;
    advertise(ctx: ProfileExecutionContext): { readonly tags: readonly string[] };
    observe(ctx: ProfileExecutionContext, tag: string): { readonly conversationId: string | null } | Refusal;
  };
  /** What a Room has observed of its services and bridges (ADR-0269). Core verifies every signature; the host only supplies bytes. */
  readonly roomControl: {
    observe(ctx: ProfileExecutionContext, room: string, kind: "service" | "bridge", signedHex: string, reportedApprovers?: readonly Approver[]): { readonly outcome: "accepted" | "duplicate" | "forked" } | Refusal;
    view(ctx: ProfileExecutionContext, room: string): { readonly entries: readonly RoomControlEntry[] };
  };
  readonly spaceBinding: {
    propose(handle: string): Done | Refusal;
    childAccepts(handle: string): LinkState | Refusal;
    unlink(handle: string): LinkState | Refusal;
    createBinding(handle: string, a: { spaceId: string; governance: readonly Approver[]; capabilities: readonly string[]; freshnessBound: number; acceptsSubdelegation: boolean }): Done | Refusal;
    view(handle: string): LinkState | Refusal;
  };
}

export function createScopeEngines(core: Pick<WasmCore, "call">): ScopeEngines {
  const q = <T>(cmd: Record<string, unknown>): T => core.call(cmd) as T;
  const rs = (room: string, caps: readonly string[], op: Record<string, unknown>) => q<never>({ cmd: "room_services_do", room, caps, op });
  const br = (room: string, caps: readonly string[], op: Record<string, unknown>) => q<never>({ cmd: "bridge_room_do", room, caps, op });
  const lt = (side: "sender" | "receiver", handle: string, op: Record<string, unknown>) => q<never>({ cmd: "lan_transfer_do", side, handle, op });
  const sb = (handle: string, op: Record<string, unknown>) => q<never>({ cmd: "space_binding_do", handle, op });
  return {
    roomServices: {
      open: (room, o) => rs(room, [], { kind: "open", ...o }),
      close: (room) => rs(room, [], { kind: "close" }),
      install: (room, caps, a) => rs(room, caps, { kind: "install", ...a }),
      observeHead: (room, head, now) => rs(room, [], { kind: "observe_head", head, now }),
      mustStopSending: (room, service, endpoint) => (rs(room, [], { kind: "must_stop", service, endpoint }) as { mustStopSending: boolean }).mustStopSending === true,
      mayCommitRemoval: (room, a) => rs(room, [], { kind: "may_commit_removal", ...a }),
      removalCommitted: (room, service, endpoint) => (rs(room, [], { kind: "removal_committed", service, endpoint }) as { applied: boolean }).applied === true,
      remove: (room, caps, service) => rs(room, caps, { kind: "remove", service }),
      view: (room, service) => rs(room, [], { kind: "view", service }),
    },
    bridgeRoom: {
      open: (room, o) => br(room, [], { kind: "open", ...o }),
      admit: (room, caps, a) => br(room, caps, { kind: "admit", ...a }),
      observeHead: (room, head, now) => br(room, [], { kind: "observe_head", head, now }),
      revokeMapping: (room, caps, bridge) => br(room, caps, { kind: "revoke_mapping", bridge }),
      mayExport: (room, a) => br(room, [], { kind: "may_export", ...a }),
      view: (room, bridge) => br(room, [], { kind: "view", bridge }),
      list: (room) => br(room, [], { kind: "list" }),
      propose: (room, a) => br(room, [], { kind: "propose", ...a }),
      withdrawOffer: (room, bridge) => br(room, [], { kind: "withdraw_offer", bridge }),
      admitOffer: (room, caps, a) => br(room, caps, { kind: "admit_offer", ...a }),
      removeMember: (room, caps, a) => br(room, caps, { kind: "remove_member", ...a }),
      importAuthorOk: (room, authorKind) => (br(room, [], { kind: "import_author_ok", authorKind }) as { allowed: boolean }).allowed === true,
    },
    lanTransfer: {
      openSender: (h, a) => lt("sender", h, { kind: "sender_open", ...a }),
      openReceiver: (h, total, restoredOffset) => lt("receiver", h, { kind: "receiver_open", total, restoredOffset: restoredOffset ?? null }),
      nextChunk: (h) => lt("sender", h, { kind: "next_chunk" }),
      accept: (h, offset, len) => lt("receiver", h, { kind: "accept", offset, len }),
      transportAck: (h, upTo) => lt("sender", h, { kind: "transport_ack", upTo }),
      receipt: (h, a) => lt("sender", h, { kind: "receipt", ...a }),
    },
    lanSignal: {
      seal: (ctx, conversationId, a) => q({ cmd: "lan_signal_seal", ctx, conversationId, kind: a.kind, sessionId: a.sessionId, sdp: a.sdp }),
      open: (ctx, conversationId, envelope) => q({ cmd: "lan_signal_open", ctx, conversationId, bytes: envelope }),
    },
    lanNearby: {
      set: (ctx, conversationId, on) => q({ cmd: "lan_nearby_set", ctx, conversationId, on }),
      state: (ctx, conversationId) => q({ cmd: "lan_nearby_state", ctx, conversationId }),
      advertise: (ctx) => q({ cmd: "lan_advertise", ctx }),
      observe: (ctx, tag) => q({ cmd: "lan_observe", ctx, tag }),
    },
    roomControl: {
      observe: (ctx, room, kind, signedHex, reportedApprovers) => q({ cmd: "room_control_observe", ctx, room, kind, signed: signedHex, approvers: reportedApprovers ?? [] }),
      view: (ctx, room) => q({ cmd: "room_control_view", ctx, room }),
    },
    spaceBinding: {
      propose: (h) => sb(h, { kind: "propose" }),
      childAccepts: (h) => sb(h, { kind: "child_accepts" }),
      unlink: (h) => sb(h, { kind: "unlink" }),
      createBinding: (h, a) => sb(h, { kind: "create_binding", ...a }),
      view: (h) => sb(h, { kind: "view" }),
    },
  };
}

export { createPluginEngines } from "./plugins";
export type {
  PluginCapability, PluginEgressFacts, PluginEngines, PluginErrorCodes, PluginHostConfig, PluginRefusal, PluginRegistry,
  PluginUiNode, PluginUint,
} from "./plugins";

export { createPortabilityEngine } from "./portability";
export type {
  PortabilityRefusalClass, PortabilityErrorCodes, PortabilityRefusal, PortabilityAnswer, PortabilityUint,
  PortabilityInspection, PortabilityControlVersion, PortabilityVerdict, PortabilityObjectStatus,
  PortabilityProvenance, PortabilityImportView, PortabilityItem, PortabilityCandidate,
  PortabilitySelection, PortabilityManifestMeta, PortabilityExportInput, PortabilityExclusion,
  PortabilityReport, PortabilityExportPlan, PortabilityExport, PortabilityReadableConversation,
  PortabilityReadableExport, PortabilityRehydrateItem, PortabilityRehydrateContext,
  PortabilityRehydratePlan, PortabilityEngine,
} from "./portability";
