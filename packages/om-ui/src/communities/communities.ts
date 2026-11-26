// SPDX-License-Identifier: MPL-2.0
import type { ConversationId, OutcomeClass, RejectReason } from "@om/platform-api";
import type { RichTextV1 } from "../conversations/richtext";

export type CommunitiesPlace =
  | { readonly kind: "community" | "publication"; readonly id: string }
  | { readonly kind: "room"; readonly id: ConversationId }
  | { readonly kind: "topic"; readonly roomId: ConversationId; readonly id: string };

export type CommunitiesHref = (place: CommunitiesPlace) => string;

export interface CommunityReference {
  readonly id: string;
  readonly name: string;
}

export interface RoomReference {
  readonly id: ConversationId;
  readonly name: string;
}

/** Mirrors the spaceBinding view from the scope engine. Use its materialized answers as supplied. */
export interface CommunityLinkState {
  readonly official: boolean;
  readonly spaceSide: boolean;
  readonly childSide: boolean;
  readonly bound: boolean;
}

export type CommunitiesActionKind =
  | "join_community"
  | "request_community_join"
  | "join_room"
  | "request_room_join"
  | "follow"
  | "unfollow";

export interface CommunitiesIntent<Kind extends CommunitiesActionKind = CommunitiesActionKind> {
  readonly kind: Kind;
  /** Exact reviewed offer or preference target, issued and interpreted by the service. */
  readonly target: string;
}

export type CommunitiesAction<Kind extends CommunitiesActionKind = CommunitiesActionKind> =
  | { readonly availability: "ready"; readonly intent: CommunitiesIntent<Kind> }
  | { readonly availability: "blocked"; readonly kind: Kind; readonly reason: RejectReason };

export type MembershipState = "not_joined" | "pending" | "joined" | "denied";

export interface CapabilityLabel {
  readonly id: string;
  readonly label: string;
}

export interface AdmissionOffer {
  readonly id: string;
  readonly approvalRequired: boolean;
  readonly uses?: "one_time" | "limited" | "reusable";
  readonly expiresAtMs?: number;
  readonly grants?: readonly CapabilityLabel[];
}

export type AdmissionEvidence =
  | { readonly kind: "community_membership"; readonly community: string }
  | { readonly kind: "community_role"; readonly community: string; readonly role: string };

type JoinCommunity = "join_community" | "request_community_join";
type JoinRoom = "join_room" | "request_room_join";
export interface AdmissionView<Kind extends JoinCommunity | JoinRoom> {
  readonly membership: MembershipState;
  readonly offer?: AdmissionOffer;
  readonly evidence?: AdmissionEvidence;
  readonly action: CommunitiesAction<Kind> | null;
}

export type PermissionSource =
  | { readonly kind: "direct" | "governance" }
  | { readonly kind: "role" | "delegated"; readonly name: string };

export type PermissionAnswer =
  | { readonly kind: "allowed"; readonly sources: readonly PermissionSource[] }
  | { readonly kind: "restricted" | "not_granted" };

export interface PermissionView extends CapabilityLabel {
  readonly answer: PermissionAnswer;
}

export interface RestrictionView {
  readonly id: string;
  readonly capability: CapabilityLabel;
  readonly state: "active" | "pending" | "lifted";
  readonly reason?: string;
  readonly intendedEndAtMs?: number;
}

export interface MemberPermissionsView {
  readonly id: string;
  readonly name: string;
  readonly roles: readonly { readonly id: string; readonly name: string }[];
  readonly permissions: readonly PermissionView[];
  readonly restrictions: readonly RestrictionView[];
}

export type GovernanceState = "active" | "orphaned" | "conflict";

export interface GovernanceView {
  readonly state: GovernanceState;
  readonly successor?:
    | { readonly kind: "community"; readonly community: CommunityReference }
    | { readonly kind: "room"; readonly room: RoomReference };
}

export interface TopicView {
  readonly id: string;
  readonly title: string;
  readonly status: "open" | "resolved" | "archived";
}

export interface CommunityDetails extends CommunityReference {
  readonly kind: "community";
  readonly description?: string;
  readonly admission: AdmissionView<JoinCommunity>;
  readonly governance: GovernanceView;
  readonly rooms: readonly (RoomReference & { readonly link: CommunityLinkState })[];
  readonly publications: readonly (CommunityReference & { readonly link: CommunityLinkState })[];
  readonly communities: readonly (CommunityReference & { readonly link: CommunityLinkState })[];
  readonly members: readonly MemberPermissionsView[];
}

export interface RoomDetails extends RoomReference {
  readonly kind: "room";
  readonly community?: CommunityReference;
  readonly admission: AdmissionView<JoinRoom>;
  readonly governance: GovernanceView;
  readonly mode: "chat" | "forum" | "announcement";
  readonly topics: readonly TopicView[];
  readonly selectedTopic?: TopicView;
  /** Core's answers for the current reader, independent of the Room's presentation mode. */
  readonly participation: readonly PermissionView[];
  readonly members: readonly MemberPermissionsView[];
}

export type PublicationProvenance = "confirmed" | "checking" | "conflict";

export type PublicationPostView = {
  readonly id: string;
  readonly publishedAtMs: number;
} & (
  | { readonly state: "published" | "edited"; readonly content: RichTextV1 }
  | { readonly state: "retracted"; readonly reason?: string }
);

export interface PublicationDetails extends CommunityReference {
  readonly kind: "publication";
  readonly description?: string;
  readonly provenance: PublicationProvenance;
  readonly following: boolean;
  readonly followAction: CommunitiesAction<"follow" | "unfollow"> | null;
  readonly discussion?: RoomReference;
  readonly posts: readonly PublicationPostView[];
}

export type CommunitiesDetails = CommunityDetails | RoomDetails | PublicationDetails;

export interface CommunitiesView {
  /** Only the information the current profile is allowed to see. */
  readonly details: CommunitiesDetails | null;
  /** A read returns any outstanding operation's current outcome. */
  readonly outcome: OutcomeClass | null;
}

/**
 * Optional profile-bound service. Core owns validation, eligibility, confirmation binding,
 * and operation identity. The host maps engine refusals to shared outcome codes. A rejected
 * Promise leaves the mutation outcome unknown. Refresh reads the existing operation.
 */
export interface CommunitiesService {
  state(place: CommunitiesPlace): Promise<CommunitiesView>;
  change(place: CommunitiesPlace, intent: CommunitiesIntent): Promise<CommunitiesView>;
}
