import type { ConversationId, Brand, ProfileExecutionContext } from "./ids";

/**
 * Ordinary groups (scope `groups`, ADR-0177): the typed steps a client drives and the views it renders.
 *
 * The wire between two devices (`GroupWire`) is Core-internal JSON that the host only carries, never reads or builds: which
 * 1:1 object type and delivery kind carry it is an open design question (groups plan, Q2). Do not persist it, parse it
 * or treat it as a protocol; no second implementation may rely on it. The engine behind it needs ADR-0187 (proposed).
 */

/** The random ConversationID of a group (hex). Unrelated to any 1:1 conversation id. */
export type GroupId = Brand<string, "GroupId">;
export const asGroupId = (v: string): GroupId => v as GroupId;

/** Core-internal and opaque: carry it as the reply's `out` says and hand it to `receive`; never inspect it. */
export interface GroupWire { readonly t: string; readonly [field: string]: unknown }

/** A KeyPackage release of one of this person's other devices (own-device sync delivers it). Opaque to the UI. */
export interface GroupDeviceRelease { readonly device: string; readonly keyPackage: string; readonly pk: string }

/** One typed step of the person. `relationshipId`s are the 1:1 conversation ids; only an active one may be invited. */
export type GroupIntent =
  | { readonly kind: "create"; readonly name: string; readonly relationshipIds: readonly ConversationId[] }
  | { readonly kind: "invite"; readonly conversationId: GroupId; readonly relationshipId: ConversationId }
  | { readonly kind: "accept"; readonly inviteId: string; readonly siblings?: readonly GroupDeviceRelease[] }
  | { readonly kind: "decline"; readonly inviteId: string }
  | { readonly kind: "leave"; readonly conversationId: GroupId }
  /** `account` is the AccountID (hex) of the person to remove. */
  | { readonly kind: "remove"; readonly conversationId: GroupId; readonly account: string }
  | { readonly kind: "send_text"; readonly conversationId: GroupId; readonly text: string }
  /** The host reports that the prepared Commit was published to its group. */
  | { readonly kind: "commit_published"; readonly intentId: string }
  /** The ordering profile accepted the candidate Commit. */
  | { readonly kind: "commit_accepted"; readonly intentId: string };

export type GroupIntentKind = GroupIntent["kind"];

/** What happened; the UI owns the words. */
export type GroupEvent =
  | { readonly kind: "created"; readonly conversationId: GroupId }
  | { readonly kind: "invite_sent" | "invite_received" | "invite_accepted" | "invite_declined"; readonly inviteId: string }
  /** A Commit was prepared: publish it, then report `commit_published` and `commit_accepted` with this id. */
  | { readonly kind: "commit_prepared"; readonly intentId: string }
  /** The person accepted but the Add cannot be prepared yet (for example `not_authorized`); the invitation stays accepted. */
  | { readonly kind: "admit_blocked"; readonly inviteId: string; readonly reason: string }
  | { readonly kind: "member_added" | "member_removed"; readonly conversationId: GroupId; readonly account: string }
  | { readonly kind: "joined" | "no_longer_member"; readonly conversationId: GroupId }
  | { readonly kind: "message_sent"; readonly conversationId: GroupId; readonly id: string }
  | { readonly kind: "message_received"; readonly conversationId: GroupId; readonly id: string; readonly senderAccount: string }
  /** Something arrived that nothing of ours precedes or authorizes; nothing changed. */
  | { readonly kind: "dropped"; readonly reason: string };

/** Where an outbound goes: over the 1:1 relationship (the host fans out to the person's devices), or to one member device. */
export type GroupTarget =
  | { readonly relationship: ConversationId }
  | { readonly device: string; readonly account: string };

export interface GroupOutbound { readonly to: GroupTarget; readonly wire: GroupWire }

/** What the transport authenticated about whoever sent a wire. */
export type GroupSender = { readonly relationship: ConversationId } | { readonly device: string };

/** One step's answer: events to show and outbounds to send once the step is durable, or a typed refusal (nothing changed). */
export type GroupStep =
  | { readonly ok: true; readonly events: readonly GroupEvent[]; readonly out: readonly GroupOutbound[] }
  | { readonly ok: false; readonly refused: string };

export interface GroupSummaryView {
  readonly conversationId: GroupId;
  readonly name: string;
  readonly members: number;
  readonly invited: number;
  readonly epoch: number;
}
export interface GroupMemberView {
  readonly account: string;
  readonly roles: readonly string[];
  /** An invited person is shown, but is not in the group. */
  readonly status: "member" | "invited";
}
export interface GroupDetailView {
  readonly summary: GroupSummaryView;
  readonly people: readonly GroupMemberView[];
  /** False while a Commit is unmerged or this person is leaving. */
  readonly canSend: boolean;
}
export interface GroupInviteView {
  readonly id: string;
  readonly conversationId: GroupId;
  readonly groupName: string;
  readonly dir: "outgoing" | "incoming";
  readonly state: "offered" | "accepted" | "declined" | "admitting" | "done";
  readonly relationshipId: ConversationId;
  /** The other person's account (hex): the invitee on an outgoing invitation, the inviter on an incoming one. */
  readonly peerAccount: string;
  readonly createdMs: number;
}
export interface GroupMessageView {
  readonly id: string;
  readonly senderAccount: string;
  readonly senderDevice: string;
  readonly epoch: number;
  readonly text: string;
  readonly atMs: number;
  readonly outgoing: boolean;
}

/**
 * The groups path of a CoreClient. `do` and `receive` resolve once the step is durable; only then may the host send
 * `out`. A client built without a group engine reports `available() === false` and refuses every step
 * `groups_unavailable`.
 */
export interface GroupApi {
  available(): boolean;
  do(ctx: ProfileExecutionContext, intent: GroupIntent): Promise<GroupStep>;
  receive(ctx: ProfileExecutionContext, from: GroupSender, wire: GroupWire): Promise<GroupStep>;
  list(ctx: ProfileExecutionContext): readonly GroupSummaryView[];
  /** `null` when the group is unknown here. */
  get(ctx: ProfileExecutionContext, conversationId: GroupId): GroupDetailView | null;
  invites(ctx: ProfileExecutionContext): readonly GroupInviteView[];
  messages(ctx: ProfileExecutionContext, conversationId: GroupId): readonly GroupMessageView[];
}
