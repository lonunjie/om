// SPDX-License-Identifier: MPL-2.0
import { existsSync, readFileSync } from "node:fs";
import { createCoreRuntime, instantiateCore } from "@om/core-wasm";
import { createMemoryKv, createMemoryProvider } from "@om/core-wasm/test-support";
import { asConversationId, asProfileGeneration, asProfileId, captureProfileContext } from "@om/platform-api";
import { describe, expect, it, vi } from "vitest";
import { createScopeEngines, type DirectoryEntry, type PublicationView, type SpaceView } from "../../../../../packages/om-core-wasm/src/scope-engines";
import type { CommunitiesActionKind, CommunitiesPlace } from "../../../../../packages/om-ui/src/communities/communities";
import { communityChildLinkHandle, communitiesRoomLinkHandle, createCommunitiesService, type CommunitiesRoomViews, type FollowRecord, type FollowStore } from "./communities";
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
  const directory: DirectoryEntry[] = [];
  const feed = vi.fn<() => PublicationView | { refused: string }>(() => ({ refused: "unknown_handle" }));
  const space = vi.fn<() => SpaceView | { refused: string }>(() => ({ refused: "unknown_handle" }));
  const childLink = vi.fn<(handle: unknown) => typeof link | { refused: string }>(() => link);
  const call = vi.fn((request: Readonly<Record<string, unknown>>): unknown => {
    switch (request["cmd"]) {
      case "room_control_view": return { entries: [] };
      case "space_binding_do": return childLink(request["handle"]);
      case "directory_do": return { entries: directory };
      case "space_do": return space();
      case "publication_do": return feed();
      default: throw new Error("Unexpected Core command");
    }
  });
  const roomViews = { topics: { view: topics }, moderation: { view: moderation } };
  const options = { ctx, engines: createScopeEngines({ call }), roomViews };
  return { topics, moderation, call, directory, feed, space, childLink, options, service: createCommunitiesService(options) };
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
    expect(f.call).toHaveBeenNthCalledWith(1, { cmd: "room_control_view", ctx, room });
    expect(f.call).toHaveBeenNthCalledWith(2, { cmd: "directory_do", ctx, op: { kind: "list" } });
  });

  it("rejects every join intent without Core or storage access", async () => {
    const f = fixture();
    const community = { kind: "community", id: "community" } as const;
    const publication = { kind: "publication", id: "publication" } as const;
    await expect(f.service.state(community)).resolves.toEqual(empty);
    await expect(f.service.state(publication)).resolves.toEqual(empty);
    f.call.mockClear();
    const followStore = memoryFollowStore();
    const service = createCommunitiesService({ ...f.options, followStore });

    const places = {
      join_community: community, request_community_join: community,
      join_room: roomPlace, request_room_join: roomPlace,
    } satisfies Partial<Record<CommunitiesActionKind, CommunitiesPlace>>;
    for (const kind of Object.keys(places) as (keyof typeof places)[]) {
      await expect(service.change(places[kind], { kind, target: "review target" })).resolves.toEqual({
        details: null, outcome: { kind: "Rejected", reason: "unsupported" },
      });
    }
    expect(f.call).not.toHaveBeenCalled();
    expect(followStore.read).not.toHaveBeenCalled();
    expect(followStore.reserve).not.toHaveBeenCalled();
    expect(followStore.commit).not.toHaveBeenCalled();
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

function memoryFollowStore() {
  const records = new Map<string, FollowRecord>();
  const key = (profile: string, publication: string) => JSON.stringify([profile, publication]);
  const initial: FollowRecord = { revision: 0, following: false, pending: null, lastOperationId: null };
  return {
    read: vi.fn<FollowStore["read"]>(async (profile, publication) =>
      records.get(key(profile, publication)) ?? null),
    reserve: vi.fn<FollowStore["reserve"]>(async (profile, publication, operation) => {
      const id = key(profile, publication);
      const record = records.get(id) ?? initial;
      if (record.revision !== operation.revision || record.pending !== null) return false;
      records.set(id, { ...record, pending: operation });
      return true;
    }),
    commit: vi.fn<FollowStore["commit"]>(async (profile, publication, operationId) => {
      const id = key(profile, publication);
      const record = records.get(id);
      if (record?.pending?.id !== operationId) return;
      records.set(id, {
        revision: record.revision + 1, following: record.pending.following, pending: null, lastOperationId: operationId,
      });
    }),
  };
}

function observed(kind: DirectoryEntry["kind"], id: string, name: string, offer: DirectoryEntry["offer"] = null): DirectoryEntry {
  return { kind, id, name, description: name + " description", source: "host_observed", offer };
}

const community = { kind: "community", id: "11".repeat(32) } as const;
const publication = { kind: "publication", id: "22".repeat(32) } as const;
const offer = { target: "aa".repeat(32), approvalRequired: true, expiresAtMs: 9000, uses: "limited" } as const;

describe("community and publication places", () => {
  it("shows offers, Space governance, and pair-specific links without inferring child membership", async () => {
    const f = fixture();
    f.directory.push(
      observed("community", community.id, "Garden", offer),
      observed("room", room, "Talk"),
      observed("publication", publication.id, "Updates"),
      observed("community", "nested", "Nested"),
      observed("room", "elsewhere", "Unrelated"),
    );
    f.space.mockReturnValue({ governance: "orphaned", head: {
      hash: "head", version: "2", member: false, governanceThreshold: 1, links: [
        { kind: "room", id: room }, publication,
      ],
    } });
    f.childLink.mockImplementation((handle) => {
      if (handle === communityChildLinkHandle(ctx.profileId, community.id, "room", room)) return link;
      if (handle === communityChildLinkHandle(ctx.profileId, community.id, "community", "nested")) {
        return { official: false, spaceSide: false, childSide: true, bound: false };
      }
      return { refused: "unknown_handle" };
    });
    const liveGovernance = vi.fn(() => []);
    const service = createCommunitiesService({ ...f.options, liveGovernance });
    await expect(service.state(community)).resolves.toMatchObject({ details: {
      name: "Garden", description: "Garden description", nameSource: "host_observed", governance: { state: "orphaned" },
      rooms: [{ id: room, name: "Talk", link }],
      publications: [{ id: publication.id, link: { official: false, spaceSide: true, childSide: false, bound: false } }],
      communities: [{ id: "nested", link: { official: false, spaceSide: false, childSide: true, bound: false } }],
      admission: {
        membership: "not_joined", offer: { id: offer.target, approvalRequired: true, uses: "limited", expiresAtMs: 9000 },
        action: { availability: "blocked", kind: "request_community_join", reason: "unsupported" },
      },
    } });
    expect(liveGovernance).toHaveBeenCalledWith(ctx, community.id);
    f.space.mockReturnValue({ head: null, governance: "conflict" });
    await expect(service.state(community)).resolves.toMatchObject({ details: {
      governance: { state: "conflict" }, rooms: [], publications: [], communities: [],
    } });
  });

  it("makes a Room offer reviewable without exposing member-only Room facts", async () => {
    const f = fixture();
    f.moderation.mockReturnValue(null);
    f.directory.push(observed("room", room, "Observed Room", { ...offer, approvalRequired: false }));
    await expect(f.service.state(roomPlace)).resolves.toMatchObject({ details: {
      name: "Observed Room", topics: [], participation: [], members: [], link: null,
      admission: { membership: null, action: { availability: "blocked", kind: "join_room", reason: "unsupported" } },
    } });
    await expect(f.service.state(topicPlace)).resolves.toEqual(empty);
    expect(f.topics).not.toHaveBeenCalled();
  });

  it("maps signed feed projections and keeps the discussion independent", async () => {
    const f = fixture();
    f.directory.push(observed("publication", publication.id, "Updates"), observed("room", room, "Discussion"));
    await expect(f.service.state(publication)).resolves.toMatchObject({ details: {
      provenance: "checking", posts: [], followAction: { availability: "blocked", reason: "unavailable" },
    } });
    f.feed.mockReturnValue({ provenance: "confirmed", frozen: false, conflicts: [],
      head: { hash: "head", version: "1", discussion: room }, posts: [
        { id: "post", publishedAtMs: 3, state: "published", text: "First entry", reason: null },
        { id: "edit", publishedAtMs: 2, state: "edited", text: "Updated entry", reason: null },
        { id: "retract", publishedAtMs: 1, state: "retracted", text: null, reason: "Withdrawn" },
      ],
    });
    await expect(f.service.state(publication)).resolves.toMatchObject({ details: {
      provenance: "confirmed", discussion: { id: room, name: "Discussion" }, posts: [
        { id: "post", state: "published", content: { blocks: [{ children: [{ text: "First entry" }] }] } },
        { id: "edit", state: "edited", content: { blocks: [{ children: [{ text: "Updated entry" }] }] } },
        { id: "retract", state: "retracted", reason: "Withdrawn" },
      ],
    } });
    f.feed.mockReturnValue({ provenance: "conflict", frozen: true, conflicts: [], head: null, posts: [] });
    await expect(f.service.state(publication)).resolves.toMatchObject({ details: { provenance: "conflict" } });
  });

  it("waits for durable follow, isolates profiles, unfollows, and rejects stale targets", async () => {
    const f = fixture();
    f.directory.push(observed("publication", publication.id, "Updates"));
    const store = memoryFollowStore();
    const service = createCommunitiesService({ ...f.options, followStore: store });
    const first = (await service.state(publication)).details;
    if (first?.kind !== "publication" || first.followAction?.availability !== "ready") throw new Error("Expected follow action");
    const follow = first.followAction.intent;
    const commit = store.commit.getMockImplementation()!;
    let release!: () => void;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    let started!: () => void;
    const start = new Promise<void>((resolve) => { started = resolve; });
    store.commit.mockImplementationOnce(async (...args) => { started(); await gate; await commit(...args); });
    const changing = service.change(publication, follow);
    await start;
    await expect(store.read(ctx.profileId, publication.id)).resolves.toMatchObject({ following: false, pending: { id: follow.target } });
    release();
    const after = await changing;
    expect(after).toMatchObject({ details: { following: true }, outcome: { kind: "Committed" } });
    await expect(service.change(publication, follow)).resolves.toMatchObject({ outcome: { kind: "Committed" } });
    expect(store.reserve).toHaveBeenCalledTimes(1);
    const other = createCommunitiesService({ ...f.options, followStore: store,
      ctx: captureProfileContext(asProfileId("two"), ctx.generation),
    });
    await expect(other.state(publication)).resolves.toMatchObject({ details: { following: false } });
    await expect(other.change(publication, follow)).resolves.toMatchObject({ outcome: { kind: "Rejected", reason: "stale_state" } });
    if (after.details?.kind !== "publication" || after.details.followAction?.availability !== "ready") throw new Error("Expected unfollow action");
    await expect(service.change(publication, after.details.followAction.intent)).resolves.toMatchObject({
      details: { following: false }, outcome: { kind: "Committed" },
    });
    await expect(service.change(publication, follow)).resolves.toMatchObject({ outcome: { kind: "Rejected", reason: "stale_state" } });
  });

  it("resumes a pending follow under the same identity after a restart", async () => {
    const f = fixture();
    f.directory.push(observed("publication", publication.id, "Updates"));
    const followStore = memoryFollowStore();
    const options = { ...f.options, followStore };
    const service = createCommunitiesService(options);
    const details = (await service.state(publication)).details;
    if (details?.kind !== "publication" || details.followAction?.availability !== "ready") throw new Error("Expected follow action");
    const intent = details.followAction.intent;
    followStore.commit.mockRejectedValueOnce(new Error("Storage interrupted"));
    await expect(service.change(publication, intent)).resolves.toMatchObject({
      details: { following: false, followAction: null }, outcome: { kind: "PendingReconciliation" },
    });
    await expect(createCommunitiesService(options).state(publication)).resolves.toMatchObject({
      details: { following: true }, outcome: { kind: "Committed" },
    });
    expect(followStore.reserve).toHaveBeenCalledTimes(1);
    expect(followStore.commit).toHaveBeenNthCalledWith(2, ctx.profileId, publication.id, intent.target);
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

      expect(engines.directory.observe(profile, "publication", publication.id, { name: "Updates\u202e" })).toEqual({ done: true });
      expect(engines.publication.open(profile, publication.id)).toEqual({ done: true });
      const wired = createCommunitiesService({ ctx: profile, engines, roomViews, followStore: memoryFollowStore() });
      const published = (await wired.state(publication)).details;
      if (published?.kind !== "publication" || published.followAction?.availability !== "ready") throw new Error("Expected follow action from real directory");
      expect(published).toMatchObject({ name: "Updates", provenance: "checking" });
      await expect(wired.change(publication, published.followAction.intent)).resolves.toMatchObject({
        details: { following: true }, outcome: { kind: "Committed" },
      });

      const stale = createCommunitiesService({ ctx: { ...profile, generation: asProfileGeneration("stale") }, engines, roomViews });
      await expect(stale.state(roomPlace)).rejects.toThrow("stale profile context");
    } finally {
      runtime.dispose();
    }
  });
});
