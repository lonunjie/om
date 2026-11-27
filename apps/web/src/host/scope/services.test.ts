// SPDX-License-Identifier: MPL-2.0
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { instantiateCore } from "@om/core-wasm";
import { asConversationId, asProfileGeneration, asProfileId, captureProfileContext, type ConversationId, type ProfileId } from "@om/platform-api";
import { describe, expect, it, vi } from "vitest";
import { createScopeEngines, type RoomControlEntry, type RoomMutationResult, type ScopeEngines, type ServiceListing } from "../../../../../packages/om-core-wasm/src/scope-engines";
import {
  createConnectedServicesService, serviceRoomHandle, type InvocationSelection, type PendingServiceOperation, type PendingServiceStore,
} from "./services";
import { createScopeServices } from "./index";

const ctx = captureProfileContext(asProfileId("one"), asProfileGeneration("generation-1"));
const room = asConversationId("room");
const service = "63".repeat(32);
const target = "a1".repeat(32);
const headHash = "b1".repeat(32);
const endpoint = "05".repeat(16);
const install = { kind: "install", target } as const;
const remove = { kind: "remove", target } as const;
const caps = ["room.install_service"];
const command = "fetch_url_via_scoped_network_provider";
const selection = {
  service, command, destination: "Assistant endpoint", expiresAtMs: 800,
  items: [
    { id: "z-trigger", kind: "trigger", value: "Summarize this" },
    { id: "a-attachment", kind: "attachment", value: "Agenda" },
    { id: "m-participant", kind: "participant", value: "Alex" },
    { id: "b-scope", kind: "scope", value: "Planning" },
  ],
} as const satisfies InvocationSelection;

function reviewId(value: InvocationSelection) {
  const items = value.items.map(({ id, kind, value: content }) => [id, kind, content]);
  return createHash("sha256").update(JSON.stringify([value.service, value.command, value.destination, value.expiresAtMs, items])).digest("hex");
}

const invoke = { kind: "invoke", target: reviewId(selection) } as const;

class MemoryPendingStore implements PendingServiceStore {
  readonly records = new Map<string, PendingServiceOperation>();
  readonly reservations: PendingServiceOperation[] = [];

  load(profile: ProfileId, conversationId: ConversationId) {
    return Promise.resolve(this.records.get(serviceRoomHandle(profile, conversationId)) ?? null);
  }

  reserve(profile: ProfileId, operation: PendingServiceOperation) {
    const key = serviceRoomHandle(profile, operation.conversationId);
    if (this.records.has(key)) return Promise.resolve(false);
    this.records.set(key, operation);
    this.reservations.push(operation);
    return Promise.resolve(true);
  }

  clear(profile: ProfileId, conversationId: ConversationId, id: string) {
    const key = serviceRoomHandle(profile, conversationId);
    if (this.records.get(key)?.id === id) this.records.delete(key);
    return Promise.resolve();
  }
}

function fixture() {
  const pending = new MemoryPendingStore();
  let current: ServiceListing = {
    service, target, membership: "proposed", name: "Assistant", installed: false,
    capabilities: ["message.create"], filterClasses: [1], approversReportedAtProposal: 1,
    admissionBasis: { serviceId: service, version: 1, hash: headHash, endpoints: [{ id: endpoint, state: 1 }] },
  };
  let head: RoomControlEntry = {
    kind: "service", id: service, headVersion: 1, headHash, frozen: false, observedAgeMs: 3,
    endpoints: [{ id: endpoint, state: 1, usable: true }], reportedApprovers: [],
  };
  let readable = true;
  const list = vi.fn<ScopeEngines["roomServices"]["list"]>(() => {
    if (!readable) throw new Error("view unavailable");
    return { services: [current] };
  });
  const receipts = new Map<string, RoomMutationResult>();
  const operation = vi.fn<ScopeEngines["roomServices"]["operation"]>((_room, { operationId }) =>
    ({ result: receipts.get(operationId) ?? null }));
  function recorded<T extends RoomMutationResult>(id: string | undefined, result: T): T {
    if (id !== undefined) receipts.set(id, result);
    return result;
  }
  const installOffer = vi.fn<ScopeEngines["roomServices"]["installOffer"]>((_room, _caps, input) => {
    if (current.target !== input.target) return recorded(input.operationId, { refused: "stale_target" });
    current = { ...current, membership: "installed", installed: true, canReadAllContent: true,
      endpoints: [endpoint], admittedVersion: 1, heldVersion: 1, capabilities: null };
    return recorded(input.operationId, { done: true });
  });
  const removeMember = vi.fn<ScopeEngines["roomServices"]["removeMember"]>((_room, _caps, input) => {
    if (current.target !== input.target) return recorded(input.operationId, { refused: "stale_target" });
    current = { ...current, membership: "removed", installed: false };
    return recorded(input.operationId, { removedEndpoints: 1 });
  });
  const base = createScopeEngines({ call: () => { throw new Error("Unexpected Core command"); } });
  const control = vi.fn<ScopeEngines["roomControl"]["view"]>(() => ({ entries: [head] }));
  const opts = {
    ctx, authority: { capabilities: () => caps }, pending,
    engines: {
      roomServices: { ...base.roomServices, list, operation, installOffer, removeMember },
      roomControl: { ...base.roomControl, view: control },
    },
  };
  return {
    opts, pending, installOffer, removeMember, control, list, operation,
    update: (patch: Partial<ServiceListing>) => { current = { ...current, ...patch }; },
    updateHead: (patch: Partial<RoomControlEntry>) => { head = { ...head, ...patch }; },
    viewAvailable: (value: boolean) => { readable = value; },
  };
}

describe("connected services host", () => {
  it("shows the review, installs and removes by target, and reports revocation from Core", async () => {
    const f = fixture();
    const connected = createConnectedServicesService(f.opts);
    expect(await connected.state(room)).toMatchObject({
      entries: [{
        principal: { id: service, name: "Assistant" }, status: "offered", mode: "full_room_member",
        capabilities: [{ id: "message.create" }], readAccess: { kind: "none" },
        actions: [{ availability: "ready", intent: install }],
      }],
      invocation: null,
    });
    expect(await connected.change(room, install)).toMatchObject({
      entries: [{ status: "installed", capabilities: [], readAccess: {
        kind: "full_room_member", roster: { canReadAllContent: true, filterClasses: [1] },
      } }],
      outcome: { kind: "Committed" },
    });
    expect(f.installOffer).toHaveBeenCalledWith(serviceRoomHandle(ctx.profileId, room), caps, { service, target, operationId: expect.any(String) }, ctx);

    f.updateHead({ headVersion: 2, endpoints: [{ id: endpoint, state: 3, usable: false }] });
    expect(await connected.state(room)).toMatchObject({
      entries: [{ status: "revoked", readAccess: { kind: "full_room_member" },
        actions: [{ availability: "ready", intent: remove }] }],
    });
    expect(await connected.change(room, remove)).toMatchObject({
      entries: [{ status: "removed", readAccess: { kind: "none" }, actions: [] }],
      outcome: { kind: "Committed" },
    });
    expect(f.removeMember).toHaveBeenCalledWith(serviceRoomHandle(ctx.profileId, room), caps, { service, target, operationId: expect.any(String) }, ctx);
    expect(f.pending.records.size).toBe(0);
  });

  it("refuses a stale target without leaking a pending operation", async () => {
    const f = fixture();
    const connected = createConnectedServicesService(f.opts);
    expect(await connected.change(room, { ...install, target: "stale" })).toMatchObject({
      outcome: { kind: "Rejected", reason: "stale_state" },
    });
    expect(f.pending.reservations).toHaveLength(0);

    const original = f.installOffer.getMockImplementation();
    if (original === undefined) throw new Error("Missing install fixture");
    f.installOffer.mockImplementationOnce((...args) => {
      expect(f.pending.records.size).toBe(1);
      f.update({ target: "b2".repeat(32) });
      return original(...args);
    });
    expect(await connected.change(room, install)).toMatchObject({
      outcome: { kind: "Rejected", reason: "stale_state" },
    });
    expect(f.pending.records.size).toBe(0);
  });

  it("keeps the pending identity and does not replay an install after losing its acknowledgement", async () => {
    const f = fixture();
    const id = createHash("sha256").update(JSON.stringify([ctx.profileId, room, "install", target])).digest("hex");
    const original = f.installOffer.getMockImplementation();
    if (original === undefined) throw new Error("Missing install fixture");
    f.installOffer.mockImplementationOnce((...args) => {
      expect(f.pending.records.get(serviceRoomHandle(ctx.profileId, room))?.id).toBe(id);
      original(...args);
      f.viewAvailable(false);
      throw new Error("lost acknowledgement");
    });
    expect(await createConnectedServicesService(f.opts).change(room, install)).toMatchObject({
      outcome: { kind: "PendingReconciliation" },
    });
    expect(f.pending.records.size).toBe(1);
    expect(await createConnectedServicesService(f.opts).state(room)).toMatchObject({ outcome: { kind: "Committed" } });
    expect(f.installOffer).toHaveBeenCalledTimes(1);
    expect(f.pending.reservations).toEqual([{ id, conversationId: room, service, intent: install }]);
    expect(f.pending.records.size).toBe(0);
  });

  it("blocks Room changes by default and offers no invocation without a selection source", async () => {
    const f = fixture();
    const connected = createConnectedServicesService({ ctx, engines: f.opts.engines, pending: f.pending });
    expect(await connected.state(room)).toMatchObject({
      entries: [{ actions: [{ availability: "blocked", kind: "install", reason: "unauthorized" }] }],
      invocation: null,
    });
    expect(await connected.change(room, install)).toMatchObject({ outcome: { kind: "Rejected", reason: "unauthorized" } });
    f.update({ membership: "installed", installed: true });
    expect(await connected.state(room)).toMatchObject({
      entries: [{ actions: [
        { availability: "blocked", kind: "remove", reason: "unauthorized" },
        { availability: "blocked", kind: "revoke", reason: "unauthorized" },
      ] }],
      invocation: null,
    });
    expect(await connected.change(room, { kind: "invoke", target })).toMatchObject({
      invocation: null, outcome: { kind: "Rejected", reason: "unsupported" },
    });
    expect(await createConnectedServicesService(f.opts).change(room, { kind: "revoke", target })).toMatchObject({
      outcome: { kind: "Rejected", reason: "unsupported" },
    });
    expect(f.pending.reservations).toHaveLength(0);
    expect(f.installOffer).not.toHaveBeenCalled();
    expect(f.removeMember).not.toHaveBeenCalled();
  });

  it("settles storage and view failures without dispatching a different pending intent", async () => {
    const f = fixture();
    const connected = createConnectedServicesService(f.opts);
    vi.spyOn(f.pending, "load").mockRejectedValueOnce(new Error("storage unavailable"));
    expect((await connected.change(room, install)).outcome).toEqual({ kind: "Rejected", reason: "unavailable" });
    f.installOffer.mockImplementationOnce(() => { throw new Error("lost acknowledgement"); });
    expect((await connected.change(room, install)).outcome).toEqual({ kind: "PendingReconciliation" });
    f.operation.mockImplementationOnce(() => { throw new Error("receipt unavailable"); });
    expect((await connected.change(room, install)).outcome).toEqual({ kind: "PendingReconciliation" });
    expect((await connected.change(room, remove)).outcome).toEqual({ kind: "Rejected", reason: "conflict" });
    expect(f.installOffer).toHaveBeenCalledTimes(1);
    expect(f.removeMember).not.toHaveBeenCalled();
  });

  it("requires the exact verified head and uses the captured profile when composing the adapter", async () => {
    const f = fixture();
    const inputCtx = { ...ctx };
    const connected = createConnectedServicesService({ ...f.opts, ctx: inputCtx });
    Object.assign(inputCtx, { profileId: asProfileId("two"), generation: asProfileGeneration("generation-2") });
    f.updateHead({ headHash: "bc".repeat(32) });
    expect(await connected.state(room)).toMatchObject({
      entries: [{ actions: [{ availability: "blocked", kind: "install", reason: "stale_state" }] }],
    });
    expect(await connected.change(room, install)).toMatchObject({ outcome: { kind: "Rejected", reason: "stale_state" } });
    expect(f.control).toHaveBeenCalledWith(ctx, room);
    expect(f.list).toHaveBeenCalledWith(serviceRoomHandle(ctx.profileId, room), ctx);
    expect(f.pending.reservations).toHaveLength(0);

    const core = { call: vi.fn((request: Readonly<Record<string, unknown>>) =>
      request["cmd"] === "room_control_view" ? { entries: [] } : { services: [] }) };
    const loaded = { core, connectedServices: { pending: f.pending } };
    expect(createScopeServices({ ctx, enabled: false, loaded })).toEqual({});
    const composed = createScopeServices({ ctx, enabled: true, loaded }).connectedServices;
    expect(await composed?.state(room)).toEqual({ entries: [], invocation: null, outcome: null });
    expect(core.call).toHaveBeenCalledWith({ cmd: "room_control_view", ctx, room });
  });
});

function invocationFixture() {
  const f = fixture();
  f.update({ membership: "installed", installed: true, capabilities: null });
  let selected: InvocationSelection | null = selection;
  let time = 17;
  const selectionSource = vi.fn(() => selected);
  const brokerGrants = vi.fn(() => [command]);
  const egressPolicy = vi.fn(() => true);
  const call = vi.fn(({ disclosure }: Readonly<Record<string, unknown>>) => {
    const target = reviewId(disclosure as InvocationSelection);
    if (!brokerGrants().includes(command)) return {
      target, allowed: false, refusal: "not_granted", flow: { tainted: true, egress: "blocked_by_capability" },
    };
    if (!egressPolicy()) return {
      target, allowed: false, refusal: "tainted_egress", flow: { tainted: true, egress: "blocked_by_conversation" },
    };
    return { target, allowed: true, flow: { tainted: true, egress: "allowed" } };
  });
  const opts = {
    ...f.opts, selectionSource, invocation: { core: { call }, wallNow: () => time },
  };
  return {
    ...f, opts, call, brokerGrants, selectionSource, egressPolicy,
    select: (value: InvocationSelection | null) => { selected = value; },
    advance: (value: number) => { time = value; },
  };
}

describe("connected services invocation review", () => {
  it("uses the captured profile and shows the selected disclosure exactly in order", async () => {
    const f = invocationFixture();
    const inputCtx = { ...ctx };
    const connected = createConnectedServicesService({ ...f.opts, ctx: inputCtx });
    Object.assign(inputCtx, { profileId: asProfileId("two"), generation: asProfileGeneration("generation-2") });
    const state = await connected.state(room);
    expect(state.invocation).toEqual({
      id: invoke.target, principal: { id: service, name: "Assistant" }, command,
      disclosure: {
        kind: "selected_context", destination: selection.destination, expiresAtMs: selection.expiresAtMs, items: selection.items,
      },
      flow: { tainted: true, egress: "allowed" },
      action: { availability: "ready", intent: invoke },
    });
    expect(f.selectionSource).toHaveBeenCalledWith(ctx, room);
    expect(f.call).toHaveBeenCalledExactlyOnceWith({
      cmd: "service_invocation_review", ctx, conversationId: room, disclosure: selection,
    });
    f.select(null);
    expect((await connected.state(room)).invocation).toBeNull();
  });

  it("blocks invoke when Core returns no grant", async () => {
    const f = invocationFixture();
    f.brokerGrants.mockReturnValue([]);
    const connected = createConnectedServicesService(f.opts);
    expect(await connected.state(room)).toMatchObject({
      invocation: {
        flow: { tainted: true, egress: "blocked_by_capability" },
        action: { availability: "blocked", kind: "invoke", reason: "unauthorized" },
      },
    });
    expect(await connected.change(room, invoke)).toMatchObject({ outcome: { kind: "Rejected", reason: "unauthorized" } });
    expect(f.pending.reservations).toHaveLength(0);
  });

  it("requires both the installed grant and a broker allow", async () => {
    const f = invocationFixture();
    const connected = createConnectedServicesService(f.opts);
    f.egressPolicy.mockReturnValueOnce(false);
    expect(await connected.state(room)).toMatchObject({
      invocation: {
        flow: { tainted: true, egress: "blocked_by_conversation" },
        action: { availability: "blocked", reason: "policy_denied" },
      },
    });
    expect(await connected.state(room)).toMatchObject({ invocation: { action: { availability: "ready" } } });
    f.brokerGrants.mockReturnValueOnce([]);
    expect(await connected.state(room)).toMatchObject({
      invocation: { action: { availability: "blocked", reason: "unauthorized" } },
    });
    f.update({ membership: "proposed", installed: false, capabilities: [command] });
    expect(await connected.change(room, invoke)).toMatchObject({ outcome: { kind: "Rejected", reason: "unauthorized" } });
    expect(f.pending.reservations).toHaveLength(0);
  });

  it("voids the earlier target when the selection is reordered or removed", async () => {
    const f = invocationFixture();
    const connected = createConnectedServicesService(f.opts);
    expect(await connected.state(room)).toMatchObject({ invocation: { id: invoke.target } });
    const [trigger, attachment, participant, scope] = selection.items;
    const reordered = { ...selection, items: [trigger, participant, attachment, scope] } as const;
    f.select(reordered);
    expect(await connected.change(room, invoke)).toMatchObject({
      invocation: { id: reviewId(reordered), disclosure: { items: reordered.items } },
      outcome: { kind: "Rejected", reason: "stale_state" },
    });
    f.select(null);
    expect(await connected.change(room, invoke)).toMatchObject({
      invocation: null, outcome: { kind: "Rejected", reason: "stale_state" },
    });
    expect(f.pending.reservations).toHaveLength(0);
  });

  it("rejects a broker verdict for another disclosure and requires review when expiry changes", async () => {
    const f = invocationFixture();
    const connected = createConnectedServicesService(f.opts);
    const response = { target: "other", allowed: true, flow: { tainted: true, egress: "allowed" } };
    f.call.mockReturnValueOnce(response);
    expect((await connected.state(room)).invocation?.action).toMatchObject({ availability: "blocked" });
    f.select({ ...selection, expiresAtMs: selection.expiresAtMs + 1 });
    expect((await connected.change(room, invoke)).outcome).toEqual({ kind: "Rejected", reason: "stale_state" });
    expect(f.pending.reservations).toHaveLength(0);
  });

  it("refuses a review at its expiry", async () => {
    const f = invocationFixture();
    const connected = createConnectedServicesService(f.opts);
    expect(await connected.state(room)).toMatchObject({ invocation: { action: { availability: "ready" } } });
    f.advance(selection.expiresAtMs);
    expect(await connected.change(room, invoke)).toMatchObject({ outcome: { kind: "Rejected", reason: "expired" } });
    expect(f.pending.reservations).toHaveLength(0);
  });

  it("reserves one pending operation across simultaneous confirmation and restart", async () => {
    const f = invocationFixture();
    const connected = createConnectedServicesService(f.opts);
    expect(await connected.state(room)).toMatchObject({ invocation: { action: { availability: "ready", intent: invoke } } });
    const answers = await Promise.all([connected.change(room, invoke), connected.change(room, invoke)]);
    expect(answers.map((answer) => answer.outcome)).toEqual([{ kind: "PendingNetwork" }, { kind: "PendingNetwork" }]);

    const restarted = createConnectedServicesService(f.opts);
    expect(await restarted.state(room)).toMatchObject({ outcome: { kind: "PendingNetwork" } });
    expect(await restarted.change(room, invoke)).toMatchObject({ outcome: { kind: "PendingNetwork" } });
    f.select(null);
    expect(await restarted.state(room)).toMatchObject({ invocation: null, outcome: { kind: "PendingNetwork" } });
    const id = createHash("sha256").update(JSON.stringify([ctx.profileId, room, "invoke", invoke.target])).digest("hex");
    expect(f.pending.reservations).toEqual([{ id, conversationId: room, service, intent: invoke }]);
    expect([...f.pending.records.values()]).toEqual(f.pending.reservations);
    expect(f.installOffer).not.toHaveBeenCalled();
    expect(f.removeMember).not.toHaveBeenCalled();
  });
});

const wasm = new URL("../../../../../packages/om-core-wasm/dist/om_wasm_scope.wasm", import.meta.url);

describe.skipIf(!existsSync(wasm))("connected services with the engineering Wasm scope", () => {
  it("does not accept an installation or policy supplied by the host", async () => {
    const f = invocationFixture();
    const core = await instantiateCore(readFileSync(wasm));
    const connected = createConnectedServicesService({ ...f.opts, invocation: { ...f.opts.invocation, core } });
    expect(await connected.state(room)).toMatchObject({ invocation: { action: { availability: "blocked" } } });
    expect(await connected.change(room, invoke)).toMatchObject({ outcome: { kind: "Rejected" } });
    expect(f.pending.reservations).toHaveLength(0);
  });
});
