// SPDX-License-Identifier: MPL-2.0
import type { OutcomeClass, RejectReason, UserActionCode } from "@om/platform-api";
import type { ReactNode } from "react";
import type { ConversationCopy } from "../conversations/copy";
import type {
  AdmissionEvidence, AdmissionOffer, CommunitiesActionKind, GovernanceState,
  MembershipState, PermissionAnswer, PermissionSource, PublicationProvenance,
  RestrictionView, RoomDetails, TopicView,
} from "./communities";

export interface CommunitiesCopy {
  readonly place: Readonly<Record<"community" | "room" | "publication" | "topic", string>>;
  readonly loading: string;
  readonly changing: string;
  readonly loadFailed: string;
  readonly changeUnknown: string;
  readonly refresh: string;
  readonly close: string;
  readonly outcome: Readonly<Record<Exclude<OutcomeClass["kind"], "Rejected" | "NeedsUserAction">, string>>;
  readonly rejected: Readonly<Record<RejectReason, string>>;
  readonly needsAction: Readonly<Record<UserActionCode, string>>;
  readonly action: Readonly<Record<CommunitiesActionKind, string>>;
  readonly community: {
    readonly rooms: string;
    readonly publications: string;
    readonly communities: string;
    readonly noRooms: string;
    readonly noPublications: string;
    readonly affiliation: Readonly<Record<"linked" | "reference", string>>;
    readonly delegated: string;
  };
  readonly admission: {
    readonly boundary: string;
    readonly roomBoundary: string;
    readonly review: string;
    readonly reviewTitle: string;
    readonly membership: Readonly<Record<"community" | "room", Readonly<Record<MembershipState, string>>>>;
    readonly approval: string;
    readonly noApproval: string;
    readonly useLabel: string;
    readonly uses: Readonly<Record<NonNullable<AdmissionOffer["uses"]>, string>>;
    readonly expires: string;
    readonly grants: string;
    readonly noGrants: string;
    readonly evidence: Readonly<Record<AdmissionEvidence["kind"], (community: ReactNode, role?: ReactNode) => ReactNode>>;
  };
  readonly authority: {
    readonly heading: string;
    readonly roles: string;
    readonly noRoles: string;
    readonly permissions: string;
    readonly restrictions: string;
    readonly answer: Readonly<Record<PermissionAnswer["kind"], string>>;
    readonly source: Readonly<Record<PermissionSource["kind"], (name?: ReactNode) => ReactNode>>;
    readonly restriction: Readonly<Record<RestrictionView["state"], string>>;
    readonly intendedEnd: string;
  };
  readonly governance: {
    readonly notice: Readonly<Record<Exclude<GovernanceState, "active">, string>>;
    readonly recovery: string;
    readonly fork: string;
    readonly openSuccessor: (name: ReactNode) => ReactNode;
  };
  readonly room: {
    readonly mode: Readonly<Record<RoomDetails["mode"], string>>;
    readonly forum: string;
    readonly announcement: string;
    readonly participation: string;
    readonly topics: string;
    readonly noTopics: string;
    readonly topicStatus: Readonly<Record<TopicView["status"], string>>;
    readonly backToRoom: (name: ReactNode) => ReactNode;
  };
  readonly publication: {
    readonly publicFeed: string;
    readonly provenance: Readonly<Record<PublicationProvenance, string>>;
    readonly following: string;
    readonly notFollowing: string;
    readonly followBoundary: string;
    readonly discussion: string;
    readonly discussionBoundary: string;
    readonly noPosts: string;
    readonly edited: string;
    readonly retracted: string;
    readonly copiesRemain: string;
    readonly richText: ConversationCopy["richText"];
  };
  readonly formatTime: (atMs: number) => string;
}
