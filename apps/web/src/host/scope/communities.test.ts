// SPDX-License-Identifier: MPL-2.0
import { existsSync, readFileSync } from "node:fs";
import { createCoreRuntime, instantiateCore } from "@om/core-wasm";
import { createMemoryKv, createMemoryProvider } from "@om/core-wasm/test-support";
import { asConversationId, asProfileGeneration, asProfileId, captureProfileContext } from "@om/platform-api";
import { describe, expect, it, vi } from "vitest";
import { createScopeEngines } from "../../../../../packages/om-core-wasm/src/scope-engines";
import type { CommunitiesActionKind, CommunitiesPlace } from "../../../../../packages/om-ui/src/communities/communities";
import { communitiesRoomLinkHandle, createCommunitiesService, type CommunitiesRoomViews } from "./communities";
import { createScopeServices } from "./index";

const ctx = captureProfileContext(asProfileId("one"), asProfileGeneration("generation-1"));
const room = asConversationId("room");
const roomPlace = { kind: "room", id: room } as const;
const topicId = "01".repeat(16);
const topicPlace = { kind: "topic", roomId: room, id: topicId } as const;
const topicView = {
  name: "Workshop", mode: "forum",
  topics: [{ id: topicId, title: "Planning", status: "open" }, { id: "02".repeat(16), title: "Notes", status: "archived" }],
} as const;
const moderationView = {
  member: true,
  governance: { state: "conflict" },
  participation: [
    { id: "room.post", label: "Post", answer: { kind: "restricted" } },
    { id: "room.reply", label: "Reply", answer: { kind: "allowed", sources: [{ kind: "role", name: "Participant" }] } },
  ],
  members: [{
    id: "reader", name: "Ari", roles: [], permissions: [],
    restrictions: [{ id: "restriction", capability: { id: "room.post", label: "Post" }, state: "active" }],
  }],
} as const;
const link = { official: true, spaceSide: true, childSide: true, bound: true };
const empty = { details: null, outcome: null };

function fixture() {
  const topics = vi.fn<CommunitiesRoomViews["topics"]["view"]>(() => topicView);
  const moderation = vi.fn<CommunitiesRoomViews["moderation"]["view"]>(() => moderationView);
  const call = vi.fn((request: Readonly<Record<string, unknown>>): unknown => {
    switch (request["cmd"]) {
      case "room_control_view": return { entries: [] };
      case "space_binding_do": return link;
      default: throw new Error("Unexpected Core command");
    }
  });
  const roomViews = { topics: { view: topics }, moderation: { view: moderation } };
  const options = { ctx, engines: createScopeEngines({ call }), roomViews };
  return { topics, moderation, call, options, service: createCommunitiesService(options) };
}

describe("communities host", () => {
  it("composes a Room view and selects a topic from that Room's list", async () => {
    const f = fixture();
    const details = {
      kind: "room", id: room, ...topicView, link,
      governance: moderationView.governance, participation: moderationView.participation, members: moderationView.members,
      admission: { membership: "joined", action: null },
    };
    await expect(f.service.state(roomPlace)).resolves.toEqual({ details, outcome: null });
    await expect(f.service.state(topicPlace)).resolves.toEqual({
      details: { ...details, selectedTopic: topicView.topics[0] }, outcome: null,
    });
    await expect(f.service.state({ ...topicPlace, id: "missing" })).resolves.toEqual({ details, outcome: null });
    expect(f.moderation).toHaveBeenCalledWith(ctx, room);
    expect(f.topics).toHaveBeenCalledWith(ctx, room);
    expect(f.call).toHaveBeenCalledWith({ cmd: "space_binding_do", handle: communitiesRoomLinkHandle(ctx.profileId, room), op: { kind: "view" } });
  });

  it("uses only the moderation view for viewer permissions, even for an announcement Room with a binding", async () => {
    const f = fixture();
    f.topics.mockReturnValue({ ...topicView, mode: "announcement" });
    const participation = [{ id: "room.react", label: "React", answer: { kind: "not_granted" } }] as const;
    f.moderation.mockReturnValue({ ...moderationView, participation });
    await expect(f.service.state(roomPlace)).resolves.toMatchObject({ details: { participation } });

    f.moderation.mockReturnValue({ ...moderationView, participation: [] });
    await expect(f.service.state(roomPlace)).resolves.toMatchObject({ details: { participation: [] } });
  });

  it("leaves an unknown link unset and withholds details without confirmed Room facts", async () => {
    const f = fixture();
    f.call.mockReturnValueOnce({ entries: [] }).mockReturnValueOnce({ refused: "unknown_handle" });
    await expect(f.service.state(roomPlace)).resolves.toMatchObject({ details: { link: null } });
    f.topics.mockReturnValueOnce(null);
    await expect(f.service.state(roomPlace)).resolves.toEqual(empty);

    f.topics.mockClear();
    f.call.mockClear();
    f.moderation.mockReturnValue({ ...moderationView, member: false });
    await expect(f.service.state(roomPlace)).resolves.toEqual(empty);
    expect(f.topics).not.toHaveBeenCalled();
    expect(f.call).toHaveBeenCalledExactlyOnceWith({ cmd: "room_control_view", ctx, room });
  });

  it("returns no community or publication details and rejects all changes without Core or storage access", async () => {
    const f = fixture();
    const community = { kind: "community", id: "community" } as const;
    const publication = { kind: "publication", id: "publication" } as const;
    await expect(f.service.state(community)).resolves.toEqual(empty);
    await expect(f.service.state(publication)).resolves.toEqual(empty);

    const places = {
      join_community: community, request_community_join: community,
      join_room: roomPlace, request_room_join: roomPlace,
      follow: publication, unfollow: publication,
    } satisfies Record<CommunitiesActionKind, CommunitiesPlace>;
    for (const kind of Object.keys(places) as CommunitiesActionKind[]) {
      await expect(f.service.change(places[kind], { kind, target: "review target" })).resolves.toEqual({
        details: null, outcome: { kind: "Rejected", reason: "unsupported" },
      });
    }
    expect(f.call).not.toHaveBeenCalled();
    expect(f.topics).not.toHaveBeenCalled();
    expect(f.moderation).not.toHaveBeenCalled();
  });

  it("only composes when opted in and refuses a stale generation before reading Room facts", async () => {
    const f = fixture();
    const core = { call: f.call };
    const loaded = { core, communities: { roomViews: f.options.roomViews } };
    expect(createScopeServices({ ctx, enabled: false, loaded })).toEqual({});
    expect(createScopeServices({ ctx, enabled: true, loaded: { core } })).toEqual({});
    const inputCtx = { ...ctx };
    const { communities } = createScopeServices({ ctx: inputCtx, enabled: true, loaded });
    if (communities === undefined) throw new Error("Expected a communities service");
    inputCtx.generation = asProfileGeneration("replacement");
    await expect(communities.state(topicPlace)).resolves.toMatchObject({ details: { id: room } });
    expect(f.moderation).toHaveBeenCalledWith(ctx, room);

    f.moderation.mockClear();
    f.topics.mockClear();
    f.call.mockClear();
    f.call.mockImplementationOnce(() => { throw new Error("stale profile context"); });
    await expect(communities.state(topicPlace)).rejects.toThrow("stale profile context");
    expect(f.call).toHaveBeenCalledExactlyOnceWith({ cmd: "room_control_view", ctx, room });
    expect(f.moderation).not.toHaveBeenCalled();
    expect(f.topics).not.toHaveBeenCalled();
  });
});

const wasm = new URL("../../../../../packages/om-core-wasm/dist/om_wasm_scope.wasm", import.meta.url);

describe.skipIf(!existsSync(wasm))("engineering Wasm communities host", () => {
  it("reads the real topic, reader membership and Space link in the captured profile", async () => {
    const core = await instantiateCore(readFileSync(wasm));
    const runtime = await createCoreRuntime({
      core, provider: createMemoryProvider(), storage: () => createMemoryKv(), autoDrain: false,
      home: { tier: "none", observers: ["no_third_party"], migrationAvailable: false, clientOriginChoice: ["default_origin"] },
    });
    try {
      const profile = await runtime.openProfile(ctx.profileId);
      const reader = "reader";
      expect(core.call({ cmd: "topics_do", caps: ["room.create_topic"], actor: reader, op: {
        kind: "create", id: topicId, title: "Core topic", at: { clock: 1, event: "ab".repeat(32) },
      } })).toEqual({ applied: true });
      const engines = createScopeEngines(core);
      const handle = communitiesRoomLinkHandle(profile.profileId, room);
      expect(engines.spaceBinding.propose(handle)).toEqual({ done: true });
      expect(engines.spaceBinding.childAccepts(handle)).toEqual({ ...link, bound: false });

      // The module has single-topic and single-reader queries. The rest of the Room projection is fixture data.
      const roomViews: CommunitiesRoomViews = {
        topics: { view(context, id) {
          if (context.profileId !== profile.profileId || id !== room) return null;
          const answer = core.call({ cmd: "topics_view", topic: topicId }) as { topic: { title: string; status: number } | null };
          if (answer.topic?.status !== 1) throw new Error("Expected the open topic from Core");
          return { ...topicView, topics: [{ id: topicId, title: answer.topic.title, status: "open" }] };
        } },
        moderation: { view(context, id) {
          if (context.profileId !== profile.profileId || id !== room) return null;
          const answer = core.call({ cmd: "moderation_view", who: reader }) as { member: boolean };
          return { ...moderationView, member: answer.member };
        } },
      };
      const { communities } = createScopeServices({ ctx: profile, enabled: true, loaded: { core, communities: { roomViews } } });
      if (communities === undefined) throw new Error("Expected a communities service");
      await expect(communities.state(roomPlace)).resolves.toEqual(empty);
      expect(core.call({ cmd: "moderation_do", caps: [], actor: reader, op: {
        kind: "admit", who: reader, incarnation: "03".repeat(16), governance: false,
      } })).toEqual({ applied: true });
      await expect(communities.state(topicPlace)).resolves.toMatchObject({ details: {
        id: room, selectedTopic: { id: topicId, title: "Core topic", status: "open" }, link: { ...link, bound: false },
      } });

      const stale = createCommunitiesService({ ctx: { ...profile, generation: asProfileGeneration("stale") }, engines, roomViews });
      await expect(stale.state(roomPlace)).rejects.toThrow("stale profile context");
    } finally {
      runtime.dispose();
    }
  });
});
