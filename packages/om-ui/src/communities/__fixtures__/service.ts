// SPDX-License-Identifier: MPL-2.0
import { asConversationId } from "@om/platform-api";
import { plainTextToRichText } from "../../conversations/richtext";
import type {
  CommunitiesPlace, CommunitiesService, CommunitiesView, CommunityDetails,
  PublicationDetails, RoomDetails, TopicView,
} from "../communities";

export type CommunitiesPreviewMode = "active" | "slow" | "failed" | "orphan";
const community = { id: "garden", name: "Neighbourhood Garden" };
const forum = { id: asConversationId("workdays"), name: "Workdays" };
const announcements = { id: asConversationId("announcements"), name: "Garden announcements" };
const exchange = { id: asConversationId("exchange"), name: "Seed exchange" };
const publication = { id: "garden-notes", name: "Garden Notes" };
const successor = { id: "garden-successor", name: "Garden Together" };
const linked = { official: true, spaceSide: true, childSide: true, bound: false };
const post = { id: "room.post", label: "Post an announcement" };
const reply = { id: "room.reply", label: "Reply" };
const date = Date.UTC(2026, 10, 25, 10);
const inviteExpiry = Date.UTC(2026, 11, 12, 10);

export const communityPlace = { kind: "community", id: community.id } as const;
export const forumPlace = { kind: "room", id: forum.id } as const;
export const announcementPlace = { kind: "room", id: announcements.id } as const;
export const publicationPlace = { kind: "publication", id: publication.id } as const;
export const exchangePlace = { kind: "room", id: exchange.id } as const;
const successorPlace = { kind: "community", id: successor.id } as const;

const topics: readonly TopicView[] = [
  { id: "december", title: "December workday", status: "open" },
  { id: "tools", title: "Tool cupboard inventory", status: "resolved" },
  { id: "seed-swap", title: "Spring seed swap", status: "archived" },
];

export function previewHref(place: CommunitiesPlace): string {
  return place.kind === "topic"
    ? `#topic/${place.roomId}/${place.id}`
    : `#${place.kind}/${place.id}`;
}

export function previewPlace(hash: string): CommunitiesPlace {
  const places: CommunitiesPlace[] = [
    communityPlace, forumPlace, announcementPlace, exchangePlace, publicationPlace, successorPlace,
    ...topics.map((topic) => ({ kind: "topic" as const, roomId: forum.id, id: topic.id })),
  ];
  return places.find((place) => previewHref(place) === hash) ?? communityPlace;
}

export const communityFixture: CommunityDetails = {
  kind: "community",
  ...community,
  description: "A place to plan workdays, swap seeds and share garden news.",
  governance: { state: "active" },
  admission: {
    membership: "not_joined",
    offer: { id: "garden-invitation", approvalRequired: false, uses: "one_time", expiresAtMs: inviteExpiry, grants: [{ id: "space.view", label: "See the Community’s member pages" }] },
    action: { availability: "ready", intent: { kind: "join_community", target: "garden-invitation" } },
  },
  rooms: [
    { ...forum, link: linked },
    { ...announcements, link: { ...linked, bound: true } },
    { ...exchange, link: linked },
  ],
  publications: [{ ...publication, link: linked }],
  communities: [],
  members: [],
};

export const forumFixture: RoomDetails = {
  kind: "room",
  ...forum,
  community,
  governance: { state: "active" },
  admission: { membership: "joined", action: null },
  mode: "forum",
  participation: [{ id: "room.post", label: "Start a discussion", answer: { kind: "not_granted" } }],
  topics,
  members: [],
};

export const exchangeFixture: RoomDetails = {
  kind: "room",
  ...exchange,
  community,
  governance: { state: "active" },
  admission: {
    membership: "not_joined",
    evidence: { kind: "community_membership", community: community.name },
    offer: { id: "garden-exchange-request", approvalRequired: true },
    action: { availability: "ready", intent: { kind: "request_room_join", target: "garden-exchange-request" } },
  },
  mode: "chat",
  participation: [],
  topics: [],
  members: [],
};

export const announcementFixture: RoomDetails = {
  kind: "room",
  ...announcements,
  community,
  governance: { state: "active" },
  admission: { membership: "joined", action: null },
  mode: "announcement",
  participation: [
    { ...post, answer: { kind: "restricted" } },
    { ...reply, answer: { kind: "allowed", sources: [{ kind: "direct" }] } },
  ],
  topics: [],
  members: [
    {
      id: "mira",
      name: "Mira",
      roles: [{ id: "publisher", name: "Publisher" }],
      permissions: [{ ...post, answer: { kind: "allowed", sources: [{ kind: "role", name: "Publisher" }] } }],
      restrictions: [],
    },
    {
      id: "alex",
      name: "Alex",
      roles: [{ id: "publisher", name: "Publisher" }],
      permissions: [{ ...post, answer: { kind: "restricted" } }],
      restrictions: [{ id: "posting-restriction", capability: post, state: "active", intendedEndAtMs: date }],
    },
  ],
};

export const publicationFixture: PublicationDetails = {
  kind: "publication",
  ...publication,
  provenance: "confirmed",
  following: false,
  followAction: { availability: "ready", intent: { kind: "follow", target: "garden-notes" } },
  posts: [
    { id: "winter", state: "edited", publishedAtMs: date, content: plainTextToRichText("Winter planting starts next Saturday. Bring gloves if you have them.") },
    { id: "hours", state: "retracted", publishedAtMs: date, reason: "The opening hours in this post were incorrect." },
    { id: "seeds", state: "published", publishedAtMs: date, content: plainTextToRichText("The seed library is open to everyone.") },
  ],
  discussion: forum,
};

export const orphanFixture: CommunityDetails = {
  ...communityFixture,
  admission: { membership: "not_joined", action: { availability: "blocked", kind: "join_community", reason: "unavailable" } },
  governance: { state: "orphaned", successor: { kind: "community", community: successor } },
};

/** Sample replies for the gallery. Mutations change only fixture data in this instance. */
export function createCommunitiesPreview(mode: CommunitiesPreviewMode): CommunitiesService {
  let garden = mode === "orphan" ? orphanFixture : communityFixture;
  let seedExchange = exchangeFixture;
  let notes = publicationFixture;
  let exchangeOutcome: CommunitiesView["outcome"] = null;

  function details(place: CommunitiesPlace): CommunitiesView["details"] {
    switch (place.kind) {
      case "community":
        return place.id === successor.id
          ? { ...communityFixture, ...successor, rooms: [], publications: [], admission: { membership: "not_joined", action: null } }
          : garden;
      case "publication": return notes;
      case "room":
        if (place.id === exchange.id) return seedExchange;
        return place.id === announcements.id ? announcementFixture : forumFixture;
      case "topic": {
        const selectedTopic = topics.find((topic) => topic.id === place.id);
        return selectedTopic === undefined ? null : { ...forumFixture, selectedTopic };
      }
    }
  }

  return {
    async state(place) {
      if (mode === "slow") return new Promise<CommunitiesView>(() => { /* Keep the loading state visible. */ });
      if (mode === "failed") throw new Error("Community preview unavailable");
      const view = details(place);
      return {
        details: view,
        outcome: view === null ? { kind: "Rejected", reason: "not_found" }
          : place.kind === "room" && place.id === exchange.id ? exchangeOutcome : null,
      };
    },
    async change(place, intent) {
      switch (intent.kind) {
        case "join_community":
          garden = { ...garden, admission: { membership: "joined", action: null } };
          break;
        case "request_room_join":
          exchangeOutcome = { kind: "PendingAuthorization" };
          seedExchange = { ...seedExchange, admission: { ...seedExchange.admission, membership: "pending", action: null } };
          break;
        case "follow":
        case "unfollow":
          notes = {
            ...notes,
            following: intent.kind === "follow",
            followAction: { availability: "ready", intent: { kind: intent.kind === "follow" ? "unfollow" : "follow", target: publication.id } },
          };
          break;
        default:
          return { details: details(place), outcome: { kind: "Rejected", reason: "unsupported" } };
      }
      return { details: details(place), outcome: { kind: intent.kind === "request_room_join" ? "PendingAuthorization" : "Committed" } };
    },
  };
}
