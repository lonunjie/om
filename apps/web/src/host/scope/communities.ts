// SPDX-License-Identifier: MPL-2.0
// ADR-0271 decision 8: the engineering host offers Room and Topic reads from Core views.
// Core supplies no community list, community membership, admission offers or publication feed.
// Every change is rejected as unsupported without creating a pending operation.
import { captureProfileContext, type ConversationId, type ProfileExecutionContext, type ProfileId } from "@om/platform-api";
import type { LinkState, ScopeEngines } from "../../../../../packages/om-core-wasm/src/scope-engines";
import type { CommunitiesService, RoomDetails } from "../../../../../packages/om-ui/src/communities/communities";

/** Core projections filtered for the captured reader and Room. The scope module currently has only per-topic and per-account lookups. */
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

/** RoomDetails has no Room-side link field in the shared UI contract. */
export interface CommunitiesRoomDetails extends RoomDetails {
  readonly link: LinkState | null;
}

export interface CommunitiesServiceOptions {
  readonly ctx: ProfileExecutionContext;
  readonly engines: {
    readonly roomControl: Pick<ScopeEngines["roomControl"], "view">;
    readonly spaceBinding: Pick<ScopeEngines["spaceBinding"], "view">;
  };
  readonly roomViews: CommunitiesRoomViews;
}

/** Use this handle when the engineering host opens the Room's Space link. */
export function communitiesRoomLinkHandle(profileId: ProfileId, room: ConversationId): string {
  return JSON.stringify([profileId, room]);
}

export function createCommunitiesService({
  ctx: inputCtx, engines, roomViews,
}: CommunitiesServiceOptions): CommunitiesService {
  const ctx = captureProfileContext(inputCtx.profileId, inputCtx.generation);

  return {
    state(place) {
      if (place.kind !== "room" && place.kind !== "topic") return Promise.resolve({ details: null, outcome: null });
      const room = place.kind === "topic" ? place.roomId : place.id;
      const topicId = place.kind === "topic" ? place.id : undefined;
      return Promise.resolve().then(() => {
        // The contextual read checks the generation before consulting the handle-based link view.
        engines.roomControl.view(ctx, room);
        const moderation = roomViews.moderation.view(ctx, room);
        if (moderation?.member !== true) return { details: null, outcome: null };
        const topics = roomViews.topics.view(ctx, room);
        if (topics === null) return { details: null, outcome: null };

        const link = engines.spaceBinding.view(communitiesRoomLinkHandle(ctx.profileId, room));
        const selectedTopic = topicId === undefined ? undefined : topics.topics.find(({ id }) => id === topicId);
        const details: CommunitiesRoomDetails = {
          kind: "room", id: room, name: topics.name, mode: topics.mode, topics: topics.topics,
          ...(selectedTopic === undefined ? {} : { selectedTopic }),
          admission: { membership: "joined", action: null },
          governance: moderation.governance, participation: moderation.participation, members: moderation.members,
          link: "refused" in link ? null : link,
        };
        return { details, outcome: null };
      });
    },
    change() {
      return Promise.resolve({ details: null, outcome: { kind: "Rejected", reason: "unsupported" } });
    },
  };
}
