import type { ConversationId, Brand, ProfileExecutionContext, ProfileId } from "./ids";

/**
 * Ordinary groups (scope `groups`, ADR-0177): the typed steps a client drives and the views it renders.
 *
 * The wire between two devices (`GroupWire`) is Core-internal JSON that the host only carries, never reads or builds. On the
 * network it is Core that seals it (ADR-0189): Invite, Accept and Decline as events on the 1:1, everything else as sealed
 * `GroupTransportFrame`s to a per-Room route; the host driver only deposits and polls what Core made. Do not persist a
 * `GroupWire` yourself, parse it or treat it as a protocol; no second implementation may rely on it.
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
  | { readonly kind: "ask_to_rejoin"; readonly relationshipId: ConversationId; readonly roomHint: string }
  | { readonly kind: "resolve_rejoin"; readonly requestId: string; readonly agree: boolean }
  | { readonly kind: "accept"; readonly inviteId: string; readonly siblings?: readonly GroupDeviceRelease[] }
  | { readonly kind: "decline"; readonly inviteId: string }
  | { readonly kind: "leave"; readonly conversationId: GroupId }
  /** `account` is the AccountID (hex) of the person to remove. */
  | { readonly kind: "remove"; readonly conversationId: GroupId; readonly account: string }
  | { readonly kind: "send_text"; readonly conversationId: GroupId; readonly text: string }
  /** A text that answers `replyTo`, a message of this Room. Refused `not_found` when it is not one (D143). */
  | { readonly kind: "send_reply"; readonly conversationId: GroupId; readonly text: string; readonly replyTo: string }
  /** A text with mentions (ADR-0222): each names a code-point range of `text` and a current member of the Room; refused `bad_mention` otherwise. */
  | { readonly kind: "send_mentions"; readonly conversationId: GroupId; readonly text: string; readonly replyTo?: string; readonly mentions: readonly GroupMentionInput[] }
  /** Create a poll (ADR-0223): 2 to 12 options; `maxSelections` only for `multiple`. */
  | { readonly kind: "create_poll"; readonly conversationId: GroupId; readonly question: string; readonly options: readonly string[]; readonly multiple: boolean; readonly maxSelections?: number; readonly visibility: PollVisibility }
  /** Our vote in a poll of the Room: the chosen option ids; empty takes the vote back. The latest vote of a member counts. */
  | { readonly kind: "vote_poll"; readonly conversationId: GroupId; readonly pollId: string; readonly optionIds: readonly number[] }
  /** Close a poll: its author, or a member who may moderate. Votes ordered after the close are ignored. */
  | { readonly kind: "close_poll"; readonly conversationId: GroupId; readonly pollId: string }
  /** Set or remove our reaction `key` on a message of the Room (`reaction.set`). */
  | { readonly kind: "set_reaction"; readonly conversationId: GroupId; readonly messageId: string; readonly key: string; readonly present: boolean }
  /** Replace the text of our own message; refused `not_author` for another member's (`message.edit`). */
  | { readonly kind: "edit_message"; readonly conversationId: GroupId; readonly messageId: string; readonly text: string }
  /** Delete our own message for everyone in the Room; refused `not_author` for another member's (`message.delete`). */
  | { readonly kind: "delete_message"; readonly conversationId: GroupId; readonly messageId: string }
  /** The host reports that the prepared Commit was published to its group (progress only: Core accepts its own Commit itself, ADR-0189 section 2). */
  | { readonly kind: "commit_published"; readonly intentId: string }
  /** Create a Room from a protection level (1 Recommended, 2 Hardened, 3 Maximum Privacy). `create` is Recommended. */
  | { readonly kind: "create_with_protection"; readonly name: string; readonly relationshipIds: readonly ConversationId[]; readonly protection: GroupProtectionLevel }
  /** Change one Room setting. Anything that loosens the Room is refused `loosening_needs_confirmation` unless `confirmed` (ADR-0189 section 3). */
  | { readonly kind: "room_setting"; readonly conversationId: GroupId; readonly setting: GroupRoomSetting; readonly confirmed?: boolean };

/** 1 Recommended, 2 Hardened, 3 Maximum Privacy (`group-policy.security_profile`). */
export type GroupProtectionLevel = 1 | 2 | 3;

/** One Room setting: a whole protection level, or one field by its code (see `GroupProtectionView`). `disappearing_timer` is seconds, `null` off. */
export type GroupRoomSetting =
  | { readonly protection: GroupProtectionLevel }
  | { readonly join_rule: number }
  | { readonly new_device_policy: number }
  | { readonly history_policy: number }
  | { readonly members_may_invite: boolean }
  | { readonly disappearing_timer: number | null };

export type GroupIntentKind = GroupIntent["kind"];

/** What happened; the UI owns the words. */
export type GroupEvent =
  | { readonly kind: "rejoin_changed" }
  | { readonly kind: "device_removed"; readonly conversationId: GroupId }
  | { readonly kind: "created"; readonly conversationId: GroupId }
  | { readonly kind: "invite_sent" | "invite_received" | "invite_accepted" | "invite_declined"; readonly inviteId: string }
  /** A Commit was prepared and is durable; the driver carries it and Core accepts it itself (ADR-0189 section 2). Progress only. */
  | { readonly kind: "commit_prepared"; readonly intentId: string }
  /** The person accepted but the Add cannot be prepared yet (for example `not_authorized`); the invitation stays accepted. */
  | { readonly kind: "admit_blocked"; readonly inviteId: string; readonly reason: string }
  | { readonly kind: "member_added" | "member_removed"; readonly conversationId: GroupId; readonly account: string }
  | { readonly kind: "joined" | "no_longer_member"; readonly conversationId: GroupId }
  | { readonly kind: "message_sent"; readonly conversationId: GroupId; readonly id: string }
  | { readonly kind: "message_received"; readonly conversationId: GroupId; readonly id: string; readonly senderAccount: string }
  /** A reaction, an edit or a delete was applied to the message `id` (ours or received). */
  | { readonly kind: "message_changed"; readonly conversationId: GroupId; readonly id: string }
  /** Something arrived that nothing of ours precedes or authorizes; nothing changed. */
  | { readonly kind: "dropped"; readonly reason: string }
  /** A frame names a device whose account head is missing or stale: it is kept (bounded, expiring) and retried; nothing was accepted. */
  | { readonly kind: "held"; readonly reason: string }
  /** A Room setting changed (on this device too): who and what; every loosening is marked. Never silent. */
  | { readonly kind: "policy_changed"; readonly conversationId: GroupId; readonly byAccount: string; readonly changes: readonly GroupSettingChange[] }
  /** A queued Room setting could not be carried out (for example the person lost the capability meanwhile). */
  | { readonly kind: "room_setting_blocked"; readonly conversationId: GroupId; readonly reason: string }
  /** This device's own Commit lost a same-parent race or its parent moved: nothing of it was merged. */
  | { readonly kind: "commit_superseded"; readonly intentId: string }
  /** A relay refused a deposit for good, so that frame will not arrive ("could not deliver"); never silent. The wording is for the client. */
  | { readonly kind: "undeliverable"; readonly count: number };

/** One visible difference between two states of a Room (`field`: join_rule, new_device_policy, history_policy, members_may_invite, disappearing_timer, security_profile). */
export interface GroupSettingChange { readonly field: string; readonly from: string; readonly to: string; readonly loosened: boolean }
/** One announced change of a Room's settings, kept so it is never silent. */
export interface GroupNoticeView { readonly atMs: number; readonly byAccount: string; readonly changes: readonly GroupSettingChange[] }

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
  /**
   * The name this device's own contact list gives that account, as the 1:1 chat shows it (ADR-0189: no new metadata; a name is
   * never sent and never read from the Room). `null` when this device has no contact for the account: say "Someone", never guess.
   */
  readonly display: string | null;
  /** This account is the person using this device (`display` is then `null`; the client says "You"). */
  readonly you: boolean;
}
/** An account named in the Room's messages or notices that is no longer on its member list (removed or left), so older lines keep their name. */
export interface GroupFormerView { readonly account: string; readonly display: string | null; readonly you: boolean }
/** The Room's protection fields in codes (the client words them; `custom` shows "Custom"). */
export interface GroupProtectionView {
  readonly profile: number;
  readonly custom: boolean;
  readonly joinRule: number;
  readonly newDevicePolicy: number;
  readonly historyPolicy: number;
  readonly membersMayInvite: boolean;
  readonly disappearingTimer: number | null;
}
export interface GroupDetailView {
  readonly summary: GroupSummaryView;
  readonly people: readonly GroupMemberView[];
  /** Accounts that appear in this Room's messages and notices but are not in `people`. */
  readonly former: readonly GroupFormerView[];
  /** False while a Commit is unmerged or this person is leaving. */
  readonly canSend: boolean;
  readonly protection: GroupProtectionView;
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
/**
 * A group invitation a device holds before it has the group module (ADR-0189 section 1.1; D70). Core verified it and keeps it durably;
 * nothing has been accepted and nothing about the person has reached the group. Names are the client's to resolve: the inviter is
 * the contact of `relationshipId`, as the 1:1 chat shows them.
 */
export interface HeldGroupInvitation {
  readonly relationshipId: ConversationId;
  readonly inviteId: string;
  readonly groupName: string;
  /** When this device received it (local clock, display only), or `null` when unknown. */
  readonly receivedMs: number | null;
}
/** What could not be kept because the held queue was full, until the person acknowledges the notice (D71). */
export interface HeldGroupOverflow {
  /** Invitations lost: the person can ask to be invited again. */
  readonly invites: number;
  /** Answers (accept or decline) lost: the person who invited can invite again. */
  readonly replies: number;
}
export interface HeldGroupInvitations {
  readonly invitations: readonly HeldGroupInvitation[];
  readonly overflow: HeldGroupOverflow;
}
/** When a poll's counts are shown: at once, once you have voted, or once it is closed (a display rule, not secrecy: members' devices can read every vote). */
export type PollVisibility = "immediate" | "after_vote" | "after_close";
export interface GroupPollOptionView { readonly id: number; readonly label: string; /** Absent while the counts are not shown to this person. */ readonly count?: number }
export interface GroupPollView {
  readonly options: readonly GroupPollOptionView[];
  readonly multiple: boolean;
  readonly maxSelections?: number;
  readonly visibility: PollVisibility;
  readonly closed: boolean;
  /** How many members have a vote on record; absent while the counts are not shown. */
  readonly voters?: number;
  /** The option ids this person chose. */
  readonly mine: readonly number[];
  /** This person may close it: its author or a member who may moderate. */
  readonly canClose: boolean;
}
export interface GroupMentionInput { readonly start: number; readonly end: number; readonly account: string }
export interface GroupMessageView {
  readonly id: string;
  readonly senderAccount: string;
  readonly senderDevice: string;
  readonly epoch: number;
  readonly text: string;
  readonly atMs: number;
  readonly outgoing: boolean;
  /** The id of the message this one answers. Whether it is still here is read from the messages the viewer holds. */
  readonly replyTo?: string;
  /** An accepted edit replaced the text. */
  readonly edited?: boolean;
  /** Present when this message is a poll; `text` is then its question. */
  readonly poll?: GroupPollView;
  /** Mentions inside the text: code-point ranges and the member named. Absent when there are none. */
  readonly mentions?: readonly GroupMentionInput[];
  /** One entry per reaction key: how many members have it set and whether one is you. */
  readonly reactions?: readonly GroupReactionView[];
}
export interface GroupReactionView { readonly key: string; readonly count: number; readonly mine: boolean }

export interface GroupRejoinReviewView {
  readonly id: string;
  readonly conversationId: GroupId;
  readonly relationshipId: ConversationId;
  readonly working: boolean;
  readonly canInvite: boolean;
}

export interface GroupRejoinView {
  readonly contacts: readonly ConversationId[];
  readonly pending: readonly { readonly relationshipId: ConversationId; readonly roomHint: string }[];
  readonly reviews: readonly GroupRejoinReviewView[];
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
  rejoin?(ctx: ProfileExecutionContext): GroupRejoinView;
  messages(ctx: ProfileExecutionContext, conversationId: GroupId): readonly GroupMessageView[];
  /** The announced changes of a Room's settings, oldest first. */
  notices(ctx: ProfileExecutionContext, conversationId: GroupId): readonly GroupNoticeView[];
  /**
   * Host drivers only (the UI never calls it): Core accepts every own Commit whose three conditions hold (ADR-0189 section 2)
   * and releases its Welcome. The driver calls it after every durable step and at start-up.
   */
  settle(ctx: ProfileExecutionContext): Promise<GroupStep>;
  /**
   * Group invitations held before the group module is loaded, and what could not be kept. A base-module query: it works when
   * `available()` is false (that is its point) and is empty once the module has taken the invitations in. Absent on a client
   * with no Core behind it.
   */
  held?(ctx: ProfileExecutionContext): HeldGroupInvitations;
  /** The person read the overflow notice: clears it (durable when this resolves). */
  ackHeldOverflow?(ctx: ProfileExecutionContext): Promise<void>;
  /**
   * Events that happened in the background (a frame arrived, a Commit settled, a Room setting changed, a frame was held), so a
   * screen can tell the person. Present on a client with a host driver; the state itself is always read through the views.
   */
  onEvents?(listener: (profileId: ProfileId, events: readonly GroupEvent[]) => void): () => void;
  /**
   * The change feed (ADR-0209): `listener` is called with the profile whenever Core made a state change durable for it, which is
   * every moment a group view, the invitation lists, the rejoin view or the held invitations could read differently. It carries no
   * content (re-read through the views) and comes from Core's own commit stream, so it never causes or reveals network traffic.
   * Present on a client with a Core behind it; a screen with no feed re-reads on a calm interval instead.
   */
  onChange?(listener: (profileId: ProfileId) => void): () => void;
}
