// SPDX-License-Identifier: MPL-2.0
import { existsSync, readFileSync } from "node:fs";
import { instantiateCore, toHex } from "@om/core-wasm";
import {
  asConversationId, asDeviceId, asMessageId, asPersonId, asProfileGeneration, asProfileId, captureProfileContext,
  type AttachmentView, type ConversationSummary, type ConversationTimelineItem, type ConversationView,
  type CoreClient, type CoreQuery, type DevicesView, type QueryResult,
} from "@om/platform-api";
import { describe, expect, it, vi } from "vitest";
import { createPortabilityEngine } from "../../../../../packages/om-core-wasm/src/portability";
import type { ExportIntent } from "../../../../../packages/om-ui/src/portability/portability";
import { createCoreHistorySource, createScopeServices } from "./index";
import { emptyHistory, type FileHost } from "./portability";

const ctx = captureProfileContext(asProfileId("profile"), asProfileGeneration("generation"));
const chat = (id: string, displayName: string): ConversationSummary => ({
  id: asConversationId(id), personId: asPersonId(id), displayName, relationship: "active",
  lastMessageAt: 123, notices: [], peerAccount: "account-id-must-not-export",
});
const selected = { ...chat("selected", "Original name"), nickname: "Chat label" };
const target = { kind: "chat", id: selected.id } as const;
const archive: Extract<ExportIntent, { format: "archive" }> = {
  format: "archive", scope: { history: [target], from: null, to: null, contents: [] },
};
const unavailable = { archive: "device_unavailable", readable: "device_unavailable" };

function message(id: string, text: string, flags = { viewOnce: false, disappearing: false }) {
  return { direction: "incoming", messageId: asMessageId(id), sentAt: 123, text, ...flags } as const;
}

function fakeClient(views: readonly ConversationView[], { identity = true, refused }: {
  readonly identity?: boolean;
  readonly refused?: CoreQuery["kind"];
} = {}) {
  const devices: DevicesView = {
    controlState: "normal", linkRequests: [], notices: [], strictNotice: false,
    devices: identity ? [{
      id: asDeviceId("device-id-must-not-export"), label: "This device", role: "control", state: "active",
      isCurrent: true, enrolledAt: 0, lastSeen: "now", actions: [], cautions: [], noticeState: "told",
    }] : [],
  };
  const query = vi.fn((request: CoreQuery): Promise<DevicesView | readonly ConversationSummary[] | ConversationView> => {
    if (!("ctx" in request) || request.ctx.profileId !== ctx.profileId || request.ctx.generation !== ctx.generation) {
      throw new Error("Wrong query context");
    }
    if (request.kind === refused) return Promise.reject(new Error("Query refused"));
    switch (request.kind) {
      case "GetDevices": return Promise.resolve(devices);
      case "GetConversationList": return Promise.resolve(views.map(({ summary }) => summary));
      case "GetConversationView": {
        const view = views.find(({ summary }) => summary.id === request.conversationId);
        if (view === undefined) throw new Error("Conversation unavailable");
        return Promise.resolve(view);
      }
      default: throw new Error("Unexpected query");
    }
  });
  const unexpectedCall = () => { throw new Error("The history source only uses queries"); };
  const client = {
    query: <Q extends CoreQuery>(request: Q): Promise<QueryResult<Q>> => query(request) as Promise<QueryResult<Q>>,
    dispatch: vi.fn(unexpectedCall), subscribe: vi.fn(unexpectedCall),
    observe: vi.fn(unexpectedCall), pendingOperations: vi.fn(unexpectedCall),
    attachments: { limits: unexpectedCall, prepare: vi.fn(unexpectedCall), open: vi.fn(unexpectedCall) },
  } satisfies CoreClient;
  return { client, query };
}

function files() {
  return {
    pick: vi.fn<FileHost["pick"]>().mockResolvedValue(null),
    save: vi.fn<FileHost["save"]>().mockResolvedValue("saved"),
  } satisfies FileHost;
}

function requiredService(options: Parameters<typeof createScopeServices>[0]) {
  const { portability } = createScopeServices(options);
  if (portability === undefined) throw new Error("Expected a portability service");
  return portability;
}

const unexpectedCore = { call: () => { throw new Error("Unexpected scope engine call"); } };

describe("Core history source", () => {
  it("reports unavailable for no identity, no chat, and a refused list or timeline query", async () => {
    const view = { summary: selected, timeline: [] };
    for (const { client, query } of [
      fakeClient([view], { identity: false }),
      fakeClient([]),
      fakeClient([view], { refused: "GetConversationList" }),
    ]) {
      const host = files();
      const service = requiredService({ ctx, enabled: true, client, loaded: { core: unexpectedCore, portability: { files: host } } });
      expect((await service.state()).formats).toEqual(unavailable);
      expect(await service.exportHistory(archive)).toEqual({ status: "rejected", reason: "device_unavailable" });
      expect(query.mock.calls.some(([request]) => request.kind === "GetConversationView")).toBe(false);
      expect(host.save).not.toHaveBeenCalled();
    }
    const { client } = fakeClient([view], { refused: "GetConversationView" });
    const host = files();
    const service = requiredService({ ctx, enabled: true, client, loaded: { core: unexpectedCore, portability: { files: host } } });
    expect(await service.exportHistory(archive)).toEqual({ status: "rejected", reason: "device_unavailable" });
    expect(host.save).not.toHaveBeenCalled();
  });

  it("rejects history whose projection does not classify message retention", async () => {
    const unclassified: ConversationTimelineItem = {
      direction: "incoming", messageId: asMessageId("unclassified"), sentAt: 123, text: "Unclassified",
    };
    const { client } = fakeClient([{ summary: selected, timeline: [unclassified] }]);
    const host = files();
    const service = requiredService({ ctx, enabled: true, client, loaded: { core: unexpectedCore, portability: { files: host } } });
    expect(await service.exportHistory(archive)).toEqual({ status: "rejected", reason: "device_unavailable" });
    expect(host.save).not.toHaveBeenCalled();
  });

  it("never queries from another profile or generation", async () => {
    const { client, query } = fakeClient([{ summary: selected, timeline: [] }]);
    const source = createCoreHistorySource({ ctx, client });
    for (const caller of [
      captureProfileContext(asProfileId("other"), ctx.generation),
      captureProfileContext(ctx.profileId, asProfileGeneration("old")),
    ]) {
      expect(await source.choices(caller)).toEqual(unavailable);
      const selection = { targets: [target], contents: [] };
      await expect(source.archive?.(caller, selection)).rejects.toThrow("device_unavailable");
      await expect(source.readable?.(caller, selection)).rejects.toThrow("device_unavailable");
    }
    expect(query).not.toHaveBeenCalled();
  });

  it("caps conversation reads and total timeline entries before saving anything", async () => {
    const messages = Array.from({ length: 4096 }, (_, i) => message(String(i), "Message"));
    const views = [
      { summary: selected, timeline: messages },
      { summary: chat("second", "Second"), timeline: [message("overflow", "Overflow")] },
      { summary: chat("third", "Third"), timeline: [] },
    ];
    for (const overflow of [views, Array.from({ length: 129 }, (_, i) => ({ summary: chat(String(i), ""), timeline: [] }))]) {
      const { client, query } = fakeClient(overflow);
      const host = files();
      const service = requiredService({ ctx, enabled: true, client, loaded: { core: unexpectedCore, portability: { files: host } } });
      expect(await service.exportHistory({ ...archive, scope: { ...archive.scope, history: "all" } })).toEqual({
        status: "rejected", reason: "too_large",
      });
      const reads = query.mock.calls.flatMap(([request]) => request.kind === "GetConversationView" ? [request.conversationId] : []);
      expect(reads).toEqual(overflow === views ? [selected.id, views[1]?.summary.id] : []);
      expect(host.save).not.toHaveBeenCalled();
    }
  });

  it("prefers an explicitly supplied history source", async () => {
    const { client, query } = fakeClient([{ summary: selected, timeline: [] }]);
    const service = requiredService({
      ctx, enabled: true, client, loaded: { core: unexpectedCore, portability: { files: files(), history: emptyHistory } },
    });
    expect((await service.state()).formats).toEqual(unavailable);
    expect(query).not.toHaveBeenCalled();
  });
});

const wasm = new URL("../../../../../packages/om-core-wasm/dist/om_wasm_scope.wasm", import.meta.url);

describe.skipIf(!existsSync(wasm))("Core history exports with the scope engine", () => {
  it("reopens an archive as projection records and renders a readable copy, with Core counting exclusions", async () => {
    const core = await instantiateCore(readFileSync(wasm));
    const engine = createPortabilityEngine(core);
    const media = new TextEncoder().encode("local attachment bytes");
    const present = {
      index: 0, blobId: "01".repeat(32), mediaType: "text/plain", size: media.length, state: "delivered_locally",
      bytes: media,
    } satisfies AttachmentView & { bytes: Uint8Array };
    const remote = {
      index: 1, blobId: "02".repeat(32), mediaType: "text/plain", size: 32, state: "remote",
    } satisfies AttachmentView;
    const ordinary = {
      ...message("ordinary", "Ordinary message"), attachments: [present, remote],
      signature: "signature-must-not-export",
    };
    const { client, query } = fakeClient([
      {
        summary: selected, timeline: [
          ordinary,
          message("once", "View-once text", { viewOnce: true, disappearing: false }),
          message("expires", "Disappearing text", { viewOnce: false, disappearing: true }),
        ],
      },
      { summary: chat("unselected", "Unselected chat"), timeline: [message("elsewhere", "Unselected text")] },
    ]);
    const host = files();
    const inputCtx = { ...ctx };
    const service = requiredService({ ctx: inputCtx, enabled: true, client, loaded: { core, portability: { files: host } } });
    Object.assign(inputCtx, { profileId: asProfileId("other") });
    expect(await service.state()).toMatchObject({
      formats: {
        archive: { history: [{ ...target, name: "Chat label" }, { id: "unselected" }], contents: { media: "available" } },
        readable: { history: [{ ...target, name: "Chat label" }, { id: "unselected" }], contents: { media: "unsupported" } },
      },
      import: "device_unavailable",
    });
    const lossy = [{ code: "view_once", count: 1 }, { code: "disappearing", count: 1 }];
    const evidence = { code: "canonical_evidence", count: 0 };
    expect(await service.exportHistory({ ...archive, scope: { ...archive.scope, contents: ["media"] } })).toMatchObject({
      status: "saved", report: { details: {
        preserved: [evidence, { code: "media", count: 1 }, { code: "history", count: 1 }],
        lossy, missingMedia: [{ code: "missing_media", count: 1 }],
      } },
    });
    const archiveFile = host.save.mock.calls[0]?.[2][0];
    if (archiveFile === undefined) throw new Error("Expected archive bytes");
    const archiveHex = toHex(archiveFile.bytes);
    expect(engine.inspect(archiveHex)).toMatchObject({
      manifest: { formatVersion: "om-archive-v1", objects: 0, blobs: 1, records: 1 },
    });
    expect(engine.importReadonly(archiveHex, [], [])).toEqual({
      objects: [], quarantined: [], missingBlobs: [], records: [{
        importedRecordId: JSON.stringify([selected.id, ordinary.messageId]),
        sourceNetworkNamespace: "om.local_projection",
        claimedLevel: "foreign_export_label", effectiveLevel: "foreign_export_label", downgraded: false,
      }],
    });
    const archivedText = new TextDecoder().decode(archiveFile.bytes);
    expect(archivedText).toContain("Ordinary message");
    expect(archivedText).toContain("local attachment bytes");
    expect(archivedText).not.toMatch(/View-once text|Disappearing text|Unselected text|must-not-export/);

    expect(await service.exportHistory({ format: "readable", scope: archive.scope, includeDisappearing: false })).toMatchObject({
      status: "saved", report: { details: { preserved: [evidence], lossy } },
    });
    const readableFile = host.save.mock.calls[1]?.[2][0];
    if (readableFile === undefined) throw new Error("Expected readable bytes");
    const readableText = new TextDecoder().decode(readableFile.bytes);
    expect(readableText).toContain("This file is readable plain text.");
    expect(readableText).toContain("Chat label: Ordinary message");
    expect(readableText).not.toMatch(/View-once text|Disappearing text|Unselected text/);
    expect(engine.inspect(toHex(readableFile.bytes))).toHaveProperty("refused");

    expect(query.mock.calls.filter(([request]) => request.kind === "GetConversationView")).toEqual([
      [{ kind: "GetConversationView", ctx, conversationId: selected.id }],
      [{ kind: "GetConversationView", ctx, conversationId: selected.id }],
    ]);
    expect(client.dispatch).not.toHaveBeenCalled();
    expect(client.attachments.prepare).not.toHaveBeenCalled();
    expect(client.attachments.open).not.toHaveBeenCalled();
  });
});
