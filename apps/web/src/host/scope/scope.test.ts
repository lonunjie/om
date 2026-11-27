// SPDX-License-Identifier: MPL-2.0
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { createCoreRuntime, instantiateCore } from "@om/core-wasm";
import { createScopeEngines } from "../../../../../packages/om-core-wasm/src/scope-engines";
import type { ScopeEngines } from "../../../../../packages/om-core-wasm/src/scope-engines";
import { createMemoryKv, createMemoryProvider } from "@om/core-wasm/test-support";
import {
  asConversationId, asProfileGeneration, asProfileId, available, captureProfileContext, partial, unavailable,
  type ConversationId, type LocalDeliveryApi, type ProfileId,
} from "@om/platform-api";
import { describe, expect, it, vi } from "vitest";
import { createPortabilityEngine, type PortabilityEngine } from "../../../../../packages/om-core-wasm/src/portability";
import type { BridgeListing, RoomControlEntry } from "../../../../../packages/om-core-wasm/src/scope-engines";
import { connectPair } from "../../../../../packages/om-core-wasm/src/test-support-lan-signal";
import type { ExportChoices, ExportIntent } from "../../../../../packages/om-ui/src/portability/portability";
import { bridgeRoomHandle, createBridgeService, type PendingBridgeOperation, type PendingStore } from "./bridge";
import { createScopeServices } from "./index";
import { createNearbyService } from "./nearby";
import { createPortabilityService, type FileHost } from "./portability";

const ctx = captureProfileContext(asProfileId("one"), asProfileGeneration("generation-1"));
const other = captureProfileContext(asProfileId("two"), asProfileGeneration("generation-2"));
const room = asConversationId("room");
const bridge = "62".repeat(32);
const target = "aa".repeat(32);
const admit = { kind: "admit", target } as const;
const remove = { kind: "remove", target } as const;
const cap = ["room.add_bridge"];
const authority = { capabilities: vi.fn(() => cap) };

class MemoryPendingStore implements PendingStore {
  readonly records = new Map<string, PendingBridgeOperation>();
  readonly reservations: PendingBridgeOperation[] = [];
  load(profile: ProfileId, conversationId: ConversationId) {
    return Promise.resolve(this.records.get(bridgeRoomHandle(profile, conversationId)) ?? null);
  }
  reserve(profile: ProfileId, operation: PendingBridgeOperation) {
    const key = bridgeRoomHandle(profile, operation.conversationId);
    if (this.records.has(key)) return Promise.resolve(false);
    this.records.set(key, operation);
    this.reservations.push(operation);
    return Promise.resolve(true);
  }
  clear(profile: ProfileId, conversationId: ConversationId, id: string) {
    const key = bridgeRoomHandle(profile, conversationId);
    if (this.records.get(key)?.id === id) this.records.delete(key);
    return Promise.resolve();
  }
}

function bridgeFixture() {
  const pending = new MemoryPendingStore();
  let current: BridgeListing = {
    bridge, target, membership: "proposed", installed: false, name: "Example Bridge", externalClaim: "External claim",
    mapping: { active: true, direction: 3, namespace: "example.test", allowedClasses: [1, 4] },
  };
  let entries: readonly RoomControlEntry[] = [];
  let readable = true;
  const list = vi.fn(() => {
    if (!readable) throw new Error("view unavailable");
    return { bridges: [current] };
  });
  const admitOffer = vi.fn<ScopeEngines["bridgeRoom"]["admitOffer"]>((_room, _caps, input) => {
    if (current.target !== input.target) return { refused: "stale_target" };
    current = { ...current, membership: "member", installed: true, endpoints: ["running"], heldVersion: 1 };
    return { done: true };
  });
  const removeMember = vi.fn<ScopeEngines["bridgeRoom"]["removeMember"]>((_room, _caps, input) => {
    if (current.target !== input.target) return { refused: "stale_target" };
    current = { ...current, membership: "removed", installed: false };
    return { removedEndpoints: 1 };
  });
  const mockEngine = { call: () => { throw new Error("Unexpected Core command"); } };
  const engines = createScopeEngines(mockEngine);
  const control = vi.fn<ScopeEngines["roomControl"]["view"]>(() => ({ entries }));
  const opts = {
    ctx, authority, pending, now: () => 17, freshnessBoundMs: 100,
    engines: {
      bridgeRoom: { ...engines.bridgeRoom, list, admitOffer, removeMember },
      roomControl: { ...engines.roomControl, view: control },
    },
  };
  return {
    opts, pending, list, admitOffer, removeMember, control,
    update: (changes: Partial<BridgeListing>) => { current = { ...current, ...changes }; },
    heads: (heads: readonly RoomControlEntry[]) => { entries = heads; },
    viewAvailable: (value: boolean) => { readable = value; },
  };
}

describe("bridge host", () => {
  it("shows the reviewed offer, joins head facts on membership, and removes with the same reviewed target", async () => {
    const f = bridgeFixture();
    const service = createBridgeService(f.opts);
    const head = {
      kind: "bridge", id: bridge, headVersion: 1, frozen: false, observedAgeMs: 100, reportedApprovers: [],
      endpoints: [{ id: "running", state: 1, usable: true }],
    } satisfies RoomControlEntry;
    const mapping = {
      network: "example.test", scope: "—", direction: "both", externalClaim: "External claim",
      exportedClasses: ["message.create", "reaction.set"],
    };
    expect(await service.state(room)).toMatchObject({ bridges: [{
      principal: { id: bridge, name: "Example Bridge" }, membership: "proposed",
      action: { availability: "ready", intent: admit }, mappings: [mapping],
    }], outcome: null });
    f.heads([head]);
    expect(await service.change(room, admit)).toMatchObject({
      bridges: [{ membership: "member", action: { intent: remove }, mappings: [{ ...mapping, state: "active" }] }],
      outcome: { kind: "Committed" },
    });
    expect(f.admitOffer).toHaveBeenCalledWith(bridgeRoomHandle(ctx.profileId, room), cap, { bridge, target, now: 17 });
    for (const [change, state] of [
      [{ observedAgeMs: 101 }, "stale"],
      [{ frozen: true }, "endpoint_stopped"],
      [{ endpoints: [] }, "endpoint_stopped"],
    ] as const) {
      f.heads([{ ...head, ...change }]);
      expect(await service.state(room)).toMatchObject({ bridges: [{ mappings: [{ state, exportedClasses: [] }] }] });
    }
    const current = (await service.state(room)).bridges[0]?.mappings[0];
    f.update({ mapping: { active: false, direction: 3, namespace: "example.test", allowedClasses: [1] } });
    expect(await service.state(room)).toMatchObject({ bridges: [{ mappings: [{ ...current, state: "revoked" }] }] });
    expect(await service.change(room, remove)).toMatchObject({
      bridges: [{ membership: "removed", action: null }], outcome: { kind: "Committed" },
    });
    expect(f.removeMember).toHaveBeenCalledWith(bridgeRoomHandle(ctx.profileId, room), cap, { bridge, target });
    expect(f.pending.records.size).toBe(0);
  });

  it("fails closed with the default authority and does not reserve a blocked or stale intent", async () => {
    const f = bridgeFixture();
    const { engines, pending, now } = f.opts;
    const denied = createBridgeService({ ctx, engines, pending, now });
    expect(await denied.state(room)).toMatchObject({
      bridges: [{ action: { availability: "blocked", kind: "admit", reason: "unauthorized" } }],
    });
    expect(await denied.change(room, admit)).toMatchObject({ outcome: { kind: "Rejected", reason: "unauthorized" } });
    f.update({ membership: "member" });
    expect(await denied.state(room)).toMatchObject({
      bridges: [{ action: { availability: "blocked", kind: "remove", reason: "unauthorized" } }],
    });
    expect(await denied.change(room, remove)).toMatchObject({ outcome: { kind: "Rejected", reason: "unauthorized" } });
    const allowed = createBridgeService(f.opts);
    expect(await allowed.change(room, { ...remove, target: "stale" })).toMatchObject({
      outcome: { kind: "Rejected", reason: "stale_state" },
    });
    expect(f.admitOffer).not.toHaveBeenCalled();
    expect(f.removeMember).not.toHaveBeenCalled();
    expect(f.pending.reservations).toHaveLength(0);
  });

  it("blocks offers with classes the review cannot display", async () => {
    const f = bridgeFixture();
    f.update({ mapping: { active: true, direction: 3, namespace: "example.test", allowedClasses: [1, 255] } });
    const service = createBridgeService(f.opts);
    expect(await service.state(room)).toMatchObject({
      bridges: [{ action: { availability: "blocked", kind: "admit", reason: "unsupported" } }],
    });
    expect(await service.change(room, admit)).toMatchObject({ outcome: { kind: "Rejected", reason: "unsupported" } });
    expect(f.admitOffer).not.toHaveBeenCalled();
    expect(f.pending.reservations).toHaveLength(0);
  });

  it("writes a stable pending identity before Core and retries that record if the effect is absent", async () => {
    const f = bridgeFixture();
    const id = createHash("sha256").update(JSON.stringify([ctx.profileId, room, "admit", target])).digest("hex");
    const original = f.admitOffer.getMockImplementation();
    if (original === undefined) throw new Error("Missing bridge fixture");
    f.admitOffer.mockImplementationOnce(() => {
      expect(f.pending.records.get(bridgeRoomHandle(ctx.profileId, room))?.id).toBe(id);
      throw new Error("operation result unknown");
    }).mockImplementationOnce((...args) => {
      expect(f.pending.records.get(bridgeRoomHandle(ctx.profileId, room))?.id).toBe(id);
      return original(...args);
    });
    expect(await createBridgeService(f.opts).change(room, admit)).toMatchObject({
      bridges: [{ action: null }], outcome: { kind: "PendingReconciliation" },
    });
    expect(await createBridgeService(f.opts).state(room)).toMatchObject({ outcome: { kind: "Committed" } });
    expect(f.admitOffer).toHaveBeenCalledTimes(2);
    expect(f.pending.reservations).toEqual([{ id, conversationId: room, bridge, intent: admit }]);
    expect(f.pending.records.size).toBe(0);
  });

  it("does not retry an applied operation after the acknowledgement and immediate reread are lost", async () => {
    const f = bridgeFixture();
    const original = f.admitOffer.getMockImplementation();
    if (original === undefined) throw new Error("Missing bridge fixture");
    f.admitOffer.mockImplementationOnce((...args) => {
      original(...args);
      f.viewAvailable(false);
      throw new Error("lost acknowledgement");
    });
    expect(await createBridgeService(f.opts).change(room, admit)).toMatchObject({ outcome: { kind: "PendingReconciliation" } });
    expect(f.pending.records.size).toBe(1);
    f.viewAvailable(true);
    expect(await createBridgeService(f.opts).state(room)).toMatchObject({ outcome: { kind: "Committed" } });
    expect(f.admitOffer).toHaveBeenCalledTimes(1);
    expect(f.pending.records.size).toBe(0);
  });

  it("serializes concurrent changes and guards the unscoped bridge API with the captured context", async () => {
    const f = bridgeFixture();
    const inputCtx = { ...ctx };
    const service = createBridgeService({ ...f.opts, ctx: inputCtx });
    const second = createBridgeService(f.opts);
    const firstAction = service.change(room, admit);
    Object.assign(inputCtx, other);
    const secondAction = second.change(room, admit);
    expect((await firstAction).outcome?.kind).toBe("Committed");
    await secondAction;
    expect(f.admitOffer).toHaveBeenCalledTimes(1);
    expect(f.control.mock.calls.every(([context, id]) => context.profileId === ctx.profileId && id === room)).toBe(true);
    expect(f.list.mock.calls.flat()).toEqual(expect.arrayContaining([bridgeRoomHandle(ctx.profileId, room)]));
    f.list.mockClear();
    f.control.mockImplementationOnce(() => { throw new Error("stale context"); });
    await expect(service.state(room)).rejects.toThrow("stale context");
    expect(f.list).not.toHaveBeenCalled();
  });
});

describe("nearby host", () => {
  it("probes only on a permitted setting read, preserves a partial capability, and returns Core's state", async () => {
    const initial = { enabled: false, available: true };
    const unavailableView = { enabled: false, available: false };
    const probe = vi.fn(() => Promise.resolve("available" as const));
    const platform = { localDelivery: vi.fn(() => partial({ probe })) };
    const state = vi.fn<ScopeEngines["lanNearby"]["state"]>(() => initial);
    const set = vi.fn<ScopeEngines["lanNearby"]["set"]>(() => unavailableView);
    const engines = createScopeEngines({ call: () => { throw new Error("Unexpected Core command"); } });
    const inputCtx = { ...ctx };
    const service = createNearbyService({ ctx: inputCtx, engines: { lanNearby: { ...engines.lanNearby, state, set } }, platform });
    expect(service.platform).toBe("partial");
    expect(probe).not.toHaveBeenCalled();
    const reading = service.state(room);
    Object.assign(inputCtx, other);
    expect(await reading).toEqual(initial);
    expect(probe).toHaveBeenCalledTimes(1);
    expect(state).toHaveBeenNthCalledWith(2, ctx, room);
    expect(await service.set(room, true)).toEqual(unavailableView);
    expect(set).toHaveBeenCalledWith(ctx, room, true);

    state.mockReturnValue(unavailableView);
    await service.state(room);
    expect(probe).toHaveBeenCalledTimes(1);

    const unsupported = createNearbyService({ ctx, engines, platform: { localDelivery: () => unavailable("off_by_policy") } });
    expect(unsupported.platform).toBe("unavailable");
    const failedProbe = createNearbyService({
      ctx, engines: { lanNearby: { ...engines.lanNearby, state: () => initial, set } },
      platform: { localDelivery: () => available({ probe: () => Promise.reject(new Error("unavailable")) }) },
    });
    await failedProbe.state(room);
    expect(failedProbe.platform).toBe("unavailable");
    set.mockClear();
    expect(await failedProbe.set(room, true)).toEqual(initial);
    expect(set).not.toHaveBeenCalled();
  });
});

it("composes optional services against one loaded Core and captures the originating profile", async () => {
  const core = { call: vi.fn((request: Readonly<Record<string, unknown>>) => {
    switch (request["cmd"]) {
      case "room_control_view": return { entries: [] };
      case "bridge_room_do": return { bridges: [] };
      case "lan_nearby_state": return { enabled: false, available: false };
      default: throw new Error("Unexpected Core command");
    }
  }) };
  const loaded = {
    core,
    bridge: { pending: new MemoryPendingStore(), now: () => 0 },
    portability: { files: files() },
    nearby: { platform: { localDelivery: () => unavailable<LocalDeliveryApi>() } },
  };
  expect(createScopeServices({ ctx, enabled: false, loaded })).toEqual({});
  expect(createScopeServices({ ctx, enabled: true })).toEqual({});
  const inputCtx = { ...ctx };
  const services = createScopeServices({ ctx: inputCtx, enabled: true, loaded });
  Object.assign(inputCtx, other);
  expect(await services.bridge?.state(room)).toEqual({ bridges: [], outcome: null });
  expect(await services.portability?.exportHistory(archive)).toMatchObject({ reason: "device_unavailable" });
  expect(await services.nearby?.state(room)).toEqual({ enabled: false, available: false });
  expect(core.call).toHaveBeenCalledWith({ cmd: "room_control_view", ctx, room });
  expect(core.call).toHaveBeenCalledWith({ cmd: "lan_nearby_state", ctx, conversationId: room });
});

const choices: ExportChoices = {
  history: [{ kind: "chat", id: "wanted", name: "Chat" }],
  contents: { media: "available", contacts: "unsupported", aliases: "unsupported", provenance: "unsupported", local_preferences: "unsupported" },
};
const archive: ExportIntent = { format: "archive", scope: { history: "all", from: null, to: null, contents: [] } };
const files = () => ({
  pick: vi.fn(() => Promise.resolve({ name: "archive.zip", bytes: Uint8Array.of(1, 2) })),
  save: vi.fn(() => Promise.resolve("saved" as const)),
} satisfies FileHost);
const testEngine = (): PortabilityEngine => createPortabilityEngine({ call: () => { throw new Error("Unexpected Core command"); } });

describe("portability host", () => {
  it("reports empty history as device unavailable and never opens a save dialog", async () => {
    const host = files();
    const service = createPortabilityService({ ctx, engine: testEngine(), files: host });
    expect((await service.state()).formats).toEqual({ archive: "device_unavailable", readable: "device_unavailable" });
    expect(await service.exportHistory(archive)).toEqual({ status: "rejected", reason: "device_unavailable" });
    expect(host.save).not.toHaveBeenCalled();
  });

  it("validates the scope and passes selected dates and contents into the archive engine before saving", async () => {
    const host = files();
    const engine = testEngine();
    const meta = { archiveId: "ab".repeat(16), exportScopeHex: "a0", generatedAtHint: 0, provenanceProfiles: [] };
    const exportArchive = vi.fn<PortabilityEngine["export"]>(() => ({
      archiveHex: "0102", report: { objects: 1, blobs: 0, records: 0, omittedBlobs: [], excluded: { view_once: 2 } },
    }));
    const service = createPortabilityService({
      ctx, engine: { ...engine, export: exportArchive }, files: host,
      history: {
        choices: () => Promise.resolve({ archive: choices, readable: "unsupported" }),
        archive: () => Promise.resolve({ meta, candidates: [] }),
      },
    });
    expect(await service.exportHistory({ ...archive, scope: { ...archive.scope, to: "2026-02-30" } })).toMatchObject({ reason: "invalid_scope" });
    expect(await service.exportHistory({ ...archive, scope: { ...archive.scope, history: [{ kind: "space", id: "wanted" }] } }))
      .toMatchObject({ reason: "invalid_scope" });
    expect(exportArchive).not.toHaveBeenCalled();
    const result = await service.exportHistory({
      ...archive, scope: { ...archive.scope, from: "2026-01-03", to: "2026-01-03", contents: ["media"] },
    });
    expect(exportArchive).toHaveBeenCalledWith({ candidates: [], meta, selection: {
      includeBlobs: true, conversations: ["wanted"], from: new Date(2026, 0, 3).getTime(), to: new Date(2026, 0, 4).getTime() - 1,
    } });
    expect(host.save).toHaveBeenCalledWith(ctx, "archive", [{ name: "om-history.zip", bytes: Uint8Array.of(1, 2) }]);
    expect(result).toMatchObject({ status: "saved", report: { details: { lossy: [{ code: "view_once", count: 2 }] } } });
  });

  it("passes an explicit disappearing choice and excludes unselected readable history", async () => {
    const readableExport = vi.fn<PortabilityEngine["readableExport"]>(() => ({
      files: [{ path: "Chat-1.txt", bytesHex: "01" }],
      disappearingChoice: "included_explicitly", excludedDisappearing: 0, excludedViewOnce: 1,
      isMigrationFormat: false, warning: "Plaintext",
    }));
    const message = { text: "Selected", time: "1", author: "Author", disappearing: true, viewOnce: false };
    const service = createPortabilityService({
      ctx, engine: { ...testEngine(), readableExport }, files: files(),
      history: {
        choices: () => Promise.resolve({ archive: "unsupported", readable: choices }),
        readable: () => Promise.resolve([
          { target: { kind: "chat", id: "wanted" }, title: "Chat", messages: [{ timestamp: 1, message }] },
          { target: { kind: "chat", id: "other" }, title: "Another chat", messages: [{ timestamp: 1, message }] },
        ]),
      },
    });
    expect(await service.exportHistory({ format: "readable", scope: archive.scope, includeDisappearing: true })).toMatchObject({ status: "saved" });
    expect(readableExport).toHaveBeenCalledWith([{ title: "Chat", messages: [message] }], true);
  });

  it("pins picker handles and host-held import facts to the original profile", async () => {
    const inputCtx = { ...ctx };
    const liveControl = [{ stream: "room", version: 2 }];
    const verdicts = [{ objectType: "test.room.control", sha256: "aa".repeat(32), verified: null }];
    const imported = { objects: [], records: [], quarantined: [], missingBlobs: [] };
    const facts = vi.fn(() => Promise.resolve({ liveControl, verdicts }));
    const stage = vi.fn(() => Promise.resolve());
    const host = files();
    const engine = {
      ...testEngine(),
      inspect: vi.fn<PortabilityEngine["inspect"]>(() => ({
        manifest: {
          formatVersion: "om-archive-v1", archiveId: "aa".repeat(16), encryptionProfile: null,
          objects: 0, blobs: 0, records: 0, missingBlobs: [],
        },
        modes: { read_only: "available", same_account: "requires_live_validation" },
      })),
      importReadonly: vi.fn<PortabilityEngine["importReadonly"]>(() => imported),
    };
    const options = {
      engine, files: host,
      history: { choices: () => Promise.resolve({ archive: "unsupported", readable: "unsupported" } as const), readOnly: { facts, stage } },
    };
    const service = createPortabilityService({ ...options, ctx: inputCtx });
    const selection = service.chooseImportSource();
    Object.assign(inputCtx, other);
    const result = await selection;
    expect(host.pick).toHaveBeenCalledWith(ctx);
    expect(result).toMatchObject({ status: "selected", source: { modes: { read_only: "available", same_account: "unsupported" } } });
    if (result.status !== "selected") throw new Error("expected selected archive");
    const { handle } = result.source;
    const wrongProfile = createPortabilityService({ ...options, ctx: other });
    expect(await wrongProfile.importHistory({ handle, mode: "read_only" })).toMatchObject({ reason: "read_failed" });
    expect(await service.importHistory({ handle, mode: "same_account" })).toMatchObject({ reason: "unsupported" });
    expect(facts).not.toHaveBeenCalled();
    expect(await service.importHistory({ handle, mode: "read_only" })).toMatchObject({ status: "imported" });
    expect(facts).toHaveBeenCalledWith(ctx);
    expect(engine.importReadonly).toHaveBeenCalledWith("0102", liveControl, verdicts);
    expect(stage).toHaveBeenCalledWith(ctx, { handle, archiveHex: "0102", view: imported });
    expect(await service.importHistory({ handle, mode: "read_only" })).toMatchObject({ reason: "read_failed" });
  });
});

const wasm = new URL("../../../../../packages/om-core-wasm/dist/om_wasm_scope.wasm", import.meta.url);

describe.skipIf(!existsSync(wasm))("engineering Wasm scope host", () => {
  it("uses the real portability engine to inspect and stage archive-only history", async () => {
    const core = await instantiateCore(readFileSync(wasm));
    const engine = createPortabilityEngine(core);
    const bytesHex = "0102";
    const exported = engine.export({
      candidates: [{ item: { kind: "object", objectType: "test.room.control", bytesHex }, disappearing: false, viewOnce: false }],
      selection: { includeBlobs: false },
      meta: { archiveId: "ab".repeat(16), exportScopeHex: "a0", generatedAtHint: 0, provenanceProfiles: [] },
    });
    if ("refused" in exported) throw new Error(exported.refused);
    const stage = vi.fn(() => Promise.resolve());
    const hash = createHash("sha256").update(Buffer.from(bytesHex, "hex")).digest("hex");
    const verdicts = [{
      objectType: "test.room.control",
      sha256: hash,
      verified: { control: { stream: "room", version: 1 } },
    }];
    const scope = createScopeServices({
      ctx, enabled: true, loaded: {
        core,
        portability: {
          files: { ...files(), pick: () => Promise.resolve({ name: "archive.zip", bytes: Buffer.from(exported.archiveHex, "hex") }) },
          history: {
            choices: () => Promise.resolve({ archive: "device_unavailable", readable: "device_unavailable" }),
            readOnly: { facts: () => Promise.resolve({ liveControl: [{ stream: "room", version: 2 }], verdicts }), stage },
          },
        },
      },
    });
    const service = scope.portability;
    if (service === undefined) throw new Error("Expected portability service");
    const selected = await service.chooseImportSource();
    if (selected.status !== "selected") throw new Error("Expected a valid archive");
    expect(await service.importHistory({ mode: "read_only", handle: selected.source.handle })).toMatchObject({
      status: "imported", report: { details: { lossy: [{ code: "stale_control", count: 1 }] } },
    });
    expect(stage.mock.calls).toMatchObject([[ctx, {
      handle: selected.source.handle, archiveHex: exported.archiveHex,
      view: {
        objects: [{ path: `canonical/objects/${hash}`, status: { kind: "stale_control", archived: "1", live: "2" } }],
      },
    }]]);
  });

  it("reconciles an applied bridge change and clears a refusal after the offer changed during reservation", async () => {
    const core = await instantiateCore(readFileSync(wasm));
    const rt = await createCoreRuntime({
      core, provider: createMemoryProvider(), storage: () => createMemoryKv(), autoDrain: false,
      home: { tier: "none", observers: ["no_third_party"], migrationAvailable: false, clientOriginChoice: ["default_origin"] },
    });
    try {
      const profile = await rt.openProfile(ctx.profileId);
      const engines = createScopeEngines(core);
      const handle = bridgeRoomHandle(profile.profileId, room);
      const pending = new MemoryPendingStore();
      expect(engines.bridgeRoom.open(handle, { quorum: 2, sequencingTimeout: 300, freshnessBound: 600 })).toEqual({ done: true });
      const head = { serviceId: bridge, version: 1, hash: "01".repeat(32), endpoints: [{ id: "10".repeat(16), state: 1 }] };
      const propose = (incarnation: string) => {
        const result = engines.bridgeRoom.propose(handle, {
          head, mapping: { incarnation, namespace: "example.test", direction: 3, allowedClasses: [1] },
          approvers: [{ kind: 1, id: "01".repeat(32) }, { kind: 1, id: "02".repeat(32) }],
          name: "Example Bridge", externalClaim: null,
        });
        if ("refused" in result) throw new Error(result.refused);
        return result.target;
      };
      const first = propose("01".repeat(16));
      const opts = { ctx: profile, engines, pending, now: () => 0 };
      expect(await createBridgeService(opts).state(room)).toMatchObject({
        bridges: [{ action: { availability: "blocked", reason: "unauthorized" } }],
      });
      let current = first;
      const reserve = pending.reserve.bind(pending);
      vi.spyOn(pending, "reserve").mockImplementationOnce((...args) => {
        current = propose("02".repeat(16));
        return reserve(...args);
      });
      const admitOffer = vi.spyOn(engines.bridgeRoom, "admitOffer");
      const service = createBridgeService({ ...opts, authority });
      expect(await service.change(room, { kind: "admit", target: first })).toMatchObject({
        outcome: { kind: "Rejected", reason: "stale_state" },
      });
      expect(admitOffer).toHaveReturnedWith({ refused: "stale_target" });
      expect(pending.records.size).toBe(0);
      admitOffer.mockClear();
      const real = createScopeEngines(core);
      const list = vi.spyOn(engines.bridgeRoom, "list");
      admitOffer.mockImplementationOnce((...args) => {
        expect(real.bridgeRoom.admitOffer(...args)).toEqual({ done: true });
        list.mockImplementationOnce(() => { throw new Error("lost view"); });
        throw new Error("lost acknowledgement");
      });
      expect(await service.change(room, { kind: "admit", target: current })).toMatchObject({
        outcome: { kind: "PendingReconciliation" },
      });
      expect(await createBridgeService({ ...opts, authority }).state(room)).toMatchObject({
        bridges: [{ membership: "member" }], outcome: { kind: "Committed" },
      });
      expect(admitOffer).toHaveBeenCalledTimes(1);
      expect(pending.records.size).toBe(0);

      const another = await rt.openProfile(other.profileId);
      await expect(createBridgeService({ ...opts, ctx: another }).state(room)).rejects.toThrow("Bridge Room view unavailable");
      const portability = createPortabilityService({ ctx: profile, engine: createPortabilityEngine(core), files: files() });
      expect(await portability.exportHistory(archive)).toEqual({ status: "rejected", reason: "device_unavailable" });
    } finally {
      rt.dispose();
    }
  });

  it("uses the real nearby choice only in the captured profile", async () => {
    const { alice, bob, carol, aConv } = await connectPair();
    const platform = { localDelivery: () => available({ probe: () => Promise.resolve("available" as const) }) };
    try {
      const service = createNearbyService({ ctx: alice.ctx, engines: alice.engines, platform });
      expect(await service.state(aConv)).toEqual({ enabled: false, available: true });
      expect(await service.set(aConv, true)).toEqual({ enabled: true, available: true });
      // A context for a stale generation is refused by Core; the adapter never retries it under another profile.
      const stale = createNearbyService({ ctx: { ...alice.ctx, generation: "gen-stale" as typeof alice.ctx.generation }, engines: alice.engines, platform });
      await expect(stale.state(aConv)).rejects.toThrow();
    } finally {
      for (const { rt } of [alice, bob, carol]) rt.dispose();
    }
  });
});
