// SPDX-License-Identifier: MPL-2.0
import { asConversationId, captureProfileContext, type ConversationId, type OutcomeClass, type ProfileExecutionContext, type ProfileId } from "@om/platform-api";
import type { DirectoryEntry, DirectoryKind, LinkState, ScopeEngines } from "../../../../../packages/om-core-wasm/src/scope-engines";
import { plainTextToRichText } from "../../../../../packages/om-ui/src/conversations/richtext";
import type {
  AdmissionOffer, CommunitiesAction, CommunitiesDetails, CommunitiesIntent, CommunitiesPlace, CommunitiesService,
  PublicationDetails, RoomDetails,
} from "../../../../../packages/om-ui/src/communities/communities";

/** Core projections filtered for the captured reader and Room. */
export interface CommunitiesRoomViews {
  readonly topics: {
    view(ctx: ProfileExecutionContext, room: ConversationId): Pick<RoomDetails, "name" | "mode" | "topics"> | null;
  };
  readonly moderation: {
    view(ctx: ProfileExecutionContext, room: ConversationId): (Pick<RoomDetails, "governance" | "participation" | "members"> & {
      /** Core-confirmed membership of the reader identified by ctx. */
      readonly member: boolean;
    }) | null;
  };
}

export interface CommunitiesRoomDetails extends RoomDetails {
  readonly link: LinkState | null;
}

export interface FollowOperation {
  readonly id: string;
  readonly revision: number;
  readonly following: boolean;
}

export interface FollowRecord {
  readonly revision: number;
  readonly following: boolean;
  readonly pending: FollowOperation | null;
  readonly lastOperationId: string | null;
}

/** All keys are profile-private. Reserve uses an atomic revision check; commit atomically writes the preference
 * and receipt, increments the revision, and clears the matching pending record. Repeating a commit is safe.
 * Writes resolve after durable storage acknowledges them. Reads return the current committed store snapshot. */
export interface FollowStore {
  read(profile: ProfileId, publication: string): Promise<FollowRecord | null>;
  reserve(profile: ProfileId, publication: string, operation: FollowOperation): Promise<boolean>;
  commit(profile: ProfileId, publication: string, operationId: string): Promise<void>;
}

export interface CommunitiesServiceOptions {
  readonly ctx: ProfileExecutionContext;
  readonly engines: Pick<ScopeEngines, "roomControl" | "spaceBinding" | "publication" | "space" | "directory">;
  readonly roomViews: CommunitiesRoomViews;
  readonly followStore?: FollowStore;
  /** Reported currently reachable governors, used only for the Space governance label. */
  readonly liveGovernance?: (ctx: ProfileExecutionContext, community: string) => readonly string[] | undefined;
}

/** Existing Room-side link handle. */
export function communitiesRoomLinkHandle(profileId: ProfileId, room: ConversationId): string {
  return JSON.stringify([profileId, room]);
}

/** One link per exact parent and child, even when a child belongs to several Spaces. */
export function communityChildLinkHandle(profileId: ProfileId, parent: string, kind: DirectoryKind, child: string): string {
  return JSON.stringify([profileId, parent, kind, child]);
}

const reconciling = { kind: "PendingReconciliation" } as const;
const committed = { kind: "Committed" } as const;
const unsupported = { kind: "Rejected", reason: "unsupported" } as const;
const stale = { kind: "Rejected", reason: "stale_state" } as const;
const emptyFollow: FollowRecord = { following: false, revision: 0, pending: null, lastOperationId: null };

type AdmissionKind = "join_community" | "request_community_join" | "join_room" | "request_room_join";
function admissionOffer<Kind extends AdmissionKind>(entry: DirectoryEntry, join: Kind, request: Kind) {
  if (entry.offer === null) return { action: null };
  const { target, approvalRequired, uses, expiresAtMs } = entry.offer;
  const offer: AdmissionOffer = {
    id: target, approvalRequired, source: entry.source,
    ...(uses === null ? {} : { uses }),
    ...(expiresAtMs === null ? {} : { expiresAtMs }),
  };
  const action = { availability: "blocked", kind: approvalRequired ? request : join, reason: "unsupported" } as const;
  return { offer, action };
}

async function preferenceTarget(profile: ProfileId, publication: string, revision: number, kind: "follow" | "unfollow"): Promise<string> {
  const data = new TextEncoder().encode(JSON.stringify(["publication.follow", profile, publication, revision, kind]));
  const digest = await globalThis.crypto.subtle.digest("SHA-256", data);
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function createCommunitiesService({
  ctx: inputCtx, engines, roomViews, followStore, liveGovernance,
}: CommunitiesServiceOptions): CommunitiesService {
  const ctx = captureProfileContext(inputCtx.profileId, inputCtx.generation);

  function directory() {
    const listed = engines.directory.list(ctx);
    if ("refused" in listed) throw new Error("Community directory unavailable");
    return listed.entries;
  }

  function roomDetails(place: Extract<CommunitiesPlace, { kind: "room" | "topic" }>): CommunitiesRoomDetails | null {
    const room = place.kind === "topic" ? place.roomId : place.id;
    const topicId = place.kind === "topic" ? place.id : undefined;
    const moderation = roomViews.moderation.view(ctx, room);
    if (moderation?.member !== true) {
      if (place.kind === "topic") return null;
      const observed = directory().find((entry) => entry.kind === "room" && entry.id === room);
      if (observed?.offer == null) return null;
      return {
        kind: "room", id: room, name: observed.name, nameSource: observed.source,
        ...(observed.description === null ? {} : { description: observed.description }),
        topics: [], participation: [], members: [], link: null,
        admission: {
          membership: moderation === null ? null : "not_joined",
          ...admissionOffer(observed, "join_room", "request_room_join"),
        },
      };
    }
    const topics = roomViews.topics.view(ctx, room);
    if (topics === null) return null;
    const link = engines.spaceBinding.view(communitiesRoomLinkHandle(ctx.profileId, room), ctx);
    const selectedTopic = topicId === undefined ? undefined : topics.topics.find(({ id }) => id === topicId);
    return {
      kind: "room", id: room, ...topics,
      ...(selectedTopic === undefined ? {} : { selectedTopic }),
      admission: { membership: "joined", action: null },
      ...(moderation.governance === undefined ? {} : { governance: moderation.governance }),
      participation: moderation.participation, members: moderation.members,
      link: "refused" in link ? null : link,
    };
  }

  function communityDetails(entry: DirectoryEntry, entries: readonly DirectoryEntry[]): CommunitiesDetails {
    const space = engines.space.view(ctx, entry.id, liveGovernance?.(ctx, entry.id));
    const head = "refused" in space ? null : space.head;
    const governance = "refused" in space ? null : space.governance;
    const member = head?.member ?? null;
    const spaceLinks = new Set(head?.links.map(({ kind, id }) => JSON.stringify([kind, id])));
    const children = entries.flatMap((child) => {
      if (head === null || (child.kind === "community" && child.id === entry.id)) return [];
      if (member !== true || !spaceLinks.has(JSON.stringify([child.kind, child.id]))) return [];
      // The verified Space head establishes only the Space side of this link.
      return [{ ...child, link: { official: false, spaceSide: true, childSide: false, bound: false } }];
    });
    const references = (kind: DirectoryKind) =>
      children.filter((child) => child.kind === kind).map(({ id, name, source, link }) => ({ id, name, nameSource: source, link }));
    return {
      kind: "community", id: entry.id, name: entry.name, nameSource: entry.source,
      ...(entry.description === null ? {} : { description: entry.description }),
      ...(governance === null ? {} : { governance: { state: governance } }),
      admission: member === true ? { membership: "joined", action: null } : {
        membership: member === null ? null : "not_joined",
        ...admissionOffer(entry, "join_community", "request_community_join"),
      },
      rooms: references("room").map((room) => ({ ...room, id: asConversationId(room.id) })),
      publications: references("publication"),
      communities: references("community"),
      members: [],
    };
  }

  function publicationFacts(id: string) {
    const entries = directory();
    const entry = entries.find((item) => item.kind === "publication" && item.id === id);
    if (entry === undefined) return null;
    const result = engines.publication.view(ctx, id);
    const feed = "refused" in result ? null : result;
    const room = entries.find((item) => item.kind === "room" && item.id === feed?.head?.discussion);
    return { entry, feed, room };
  }

  async function publicationView(
    facts: NonNullable<ReturnType<typeof publicationFacts>>,
    record: FollowRecord,
    outcome: OutcomeClass | null,
  ) {
    const { entry, feed, room } = facts;
    const kind = record.following ? "unfollow" : "follow";
    const followAction: CommunitiesAction<"follow" | "unfollow"> | null =
      followStore === undefined ? { availability: "blocked", kind, reason: "unavailable" }
        : outcome?.kind === "PendingReconciliation" ? null
        : { availability: "ready", intent: { kind, target: await preferenceTarget(ctx.profileId, entry.id, record.revision, kind) } };
    const details: PublicationDetails = {
      kind: "publication", id: entry.id, name: entry.name, nameSource: entry.source,
      ...(entry.description === null ? {} : { description: entry.description }),
      ...(room === undefined ? {} : { discussion: { id: asConversationId(room.id), name: room.name, nameSource: room.source } }),
      provenance: feed?.provenance ?? "checking", following: record.following, followAction,
      posts: (feed?.posts ?? []).map((post) => {
        const { id, state, publishedAtMs } = post;
        if (state === "retracted") {
          return { id, state, publishedAtMs, ...(post.reason === null ? {} : { reason: post.reason }) };
        }
        return { id, state, publishedAtMs, content: plainTextToRichText(post.text) };
      }),
    };
    return { details, outcome };
  }

  async function followResolution(store: FollowStore, publication: string, pending: FollowOperation) {
    // Commit is conditional on the exact reserved id, so competing callers can only finish that operation.
    try {
      await store.commit(ctx.profileId, publication, pending.id);
    } catch {
      // The durable commit may have completed before acknowledgement was lost.
    }
    try {
      const record = await store.read(ctx.profileId, publication) ?? emptyFollow;
      return { record, outcome: record.revision > pending.revision ? committed : reconciling };
    } catch {
      return { record: null, outcome: reconciling };
    }
  }

  async function publicationState(id: string, intent?: CommunitiesIntent) {
    let facts = publicationFacts(id);
    if (followStore === undefined) {
      const outcome = intent === undefined ? null : { kind: "Rejected", reason: "unavailable" } as const;
      return facts === null ? { details: null, outcome } : publicationView(facts, emptyFollow, outcome);
    }
    let record = await followStore.read(ctx.profileId, id) ?? emptyFollow;
    let outcome: OutcomeClass | null = null;
    let pending = record.pending;
    if (pending === null && intent !== undefined) {
      if (facts !== null && record.lastOperationId === intent.target &&
          intent.kind === (record.following ? "follow" : "unfollow")) {
        const current = publicationFacts(id);
        return current === null ? { details: null, outcome: committed } : publicationView(current, record, committed);
      }
      const kind = record.following ? "unfollow" : "follow";
      const target = await preferenceTarget(ctx.profileId, id, record.revision, kind);
      // Revalidate the captured profile after the async read and digest, before writing a pending record.
      facts = publicationFacts(id);
      if (facts === null) return { details: null, outcome: stale };
      if (intent.kind !== kind || intent.target !== target) return publicationView(facts, record, stale);
      const operation: FollowOperation = { id: target, revision: record.revision, following: kind === "follow" };
      let reserveFailed = false;
      try {
        if (await followStore.reserve(ctx.profileId, id, operation)) pending = operation;
      } catch {
        // Recover the reserved operation by reading its durable record.
        reserveFailed = true;
      }
      if (pending === null) {
        try {
          record = await followStore.read(ctx.profileId, id) ?? emptyFollow;
          pending = record.pending;
          outcome = record.lastOperationId === target ? committed
            : reserveFailed && record.revision === operation.revision ? { kind: "Rejected", reason: "unavailable" }
            : stale;
        } catch {
          outcome = reconciling;
        }
      }
    }
    if (pending !== null) {
      // A revoked profile context cannot start or resume a write.
      directory();
      const resolved = await followResolution(followStore, id, pending);
      record = resolved.record ?? record;
      outcome = resolved.outcome;
    }
    facts = publicationFacts(id);
    return facts === null ? { details: null, outcome } : publicationView(facts, record, outcome);
  }

  return {
    async state(place) {
      if (place.kind === "room" || place.kind === "topic") return { details: roomDetails(place), outcome: null };
      if (place.kind === "publication") return publicationState(place.id);
      const entries = directory();
      const entry = entries.find((item) => item.kind === "community" && item.id === place.id);
      return { details: entry === undefined ? null : communityDetails(entry, entries), outcome: null };
    },
    async change(place, intent) {
      if (place.kind !== "publication" || (intent.kind !== "follow" && intent.kind !== "unfollow")) {
        return { details: null, outcome: unsupported };
      }
      return publicationState(place.id, { kind: intent.kind, target: intent.target });
    },
  };
}
