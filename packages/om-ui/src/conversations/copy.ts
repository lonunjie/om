import type { ScheduleCopy } from "./Schedule";
import type { MentionsCopy } from "./MentionPicker";
import type { AttachmentRefusal, RejectReason, RelationshipStateCode, SecurityNoticeCode, UserActionCode } from "@om/platform-api";

/** All user-visible strings for the conversation slice. Components contain no copy; callers supply (localized) text. */
export interface ConversationCopy {
  readonly list: {
    readonly label: string;
    readonly empty: string;
    readonly noActivity: string;
    readonly unread: (count: number) => string;
    /** Marks a preview that is the last thing you sent: "You:". */
    readonly you: string;
    /** Marks unsent text kept for this chat in place of the preview: "Draft:". */
    readonly draft: string;
    readonly securityNotice: string;
  };
  readonly view: {
    readonly label: string;
    readonly empty: string;
    readonly loadFailed: string;
    readonly incoming: string;
    readonly outgoing: string;
    readonly retry: string;
    /** Accessible name of the control that leaves a conversation for the list. */
    readonly back: string;
    readonly formatTime: (epochMs: number) => string;
  };
  /** Replying to a message (`reply_to`): the quoted block in a bubble and the chip above the composer. */
  readonly reply: {
    /** Accessible name of the per-message control that starts a reply. */
    readonly action: string;
    /** The chip above the composer: "Replying to Maria" / "Replying to yourself". */
    readonly replyingTo: (name: string) => string;
    readonly replyingToSelf: string;
    readonly cancel: string;
    /** The quoted block of a reply, by name of the person who wrote the original. */
    readonly inReplyTo: (name: string) => string;
    readonly inReplyToSelf: string;
    /** Shown in place of the quote when the original is not on this device (never arrived, deleted, or expired). */
    readonly unavailable: string;
    /** Stands in for an attachment-only original. */
    readonly attachment: string;
    /** Accessible name of the quoted block: it moves to the original. */
    readonly jump: string;
  };
  /** Editing and deleting one of your own messages (`message.edit`, `message.delete`). */
  readonly change: {
    readonly edit: string;
    /** Label of the text box while editing. */
    readonly editLabel: string;
    readonly save: string;
    readonly cancel: string;
    /** The small marker on a message whose text was replaced. */
    readonly edited: string;
    readonly editFailed: string;
    readonly delete: string;
    readonly deleteTitle: string;
    /** Says the scope plainly: everyone in this chat, and that a copy already read cannot be called back. */
    readonly deleteBody: string;
    readonly deleteConfirm: string;
    readonly deleteFailed: string;
  };
  /** Reactions (`reaction.set`): the picker beside a message and the chips under it. */
  readonly react: {
    /** Accessible name of the per-message control that opens the picker. */
    readonly action: string;
    readonly picker: string;
    /** Accessible name of the list of reactions under a message. */
    readonly list: string;
    /** Accessible name of one choice in the picker. */
    readonly set: (key: string) => string;
    /** Accessible name of a chip under a message: it adds your reaction, or removes it when it is yours. */
    readonly chip: (key: string, count: number, mine: boolean) => string;
    readonly failed: string;
  };
  /** The contact detail sheet and the nickname (ADR-0196). */
  readonly contact: {
    /** Accessible name of the header control that opens the sheet. */
    readonly open: (name: string) => string;
    readonly title: (name: string) => string;
    readonly close: string;
    /** Label before the name the contact chose for themselves: "Their name". */
    readonly theirName: string;
    readonly nicknameLabel: string;
    readonly allowCalls: string;
    readonly allowCallsFailed: string;
    readonly nicknamePlaceholder: string;
    /** One plain sentence: only you see it, it is kept on this device and backup, never sent. */
    readonly nicknameHint: string;
    readonly save: string;
    readonly clear: string;
    readonly saved: string;
    readonly cleared: string;
    /** The nickname was refused (too long or has hidden direction characters). */
    readonly invalid: string;
    readonly failed: string;
    readonly verification: {
      readonly open: string;
      readonly hint: string;
      readonly theirAccount: string;
      readonly ownAccount: string;
      readonly input: string;
      readonly check: string;
      readonly invalid: string;
      readonly match: string;
      readonly mismatch: string;
    };
  };
  readonly delivery: {
    readonly pending: string;
    readonly pendingNetwork: string;
    readonly pendingReconciliation: string;
    /** Kept on this device while the contact recovers their account; sent once their new device is confirmed (ADR-0190 section 5). */
    readonly waitingForPeerRecovery: string;
    readonly sent: string;
    readonly delivered: string;
    readonly rejected: Readonly<Record<RejectReason, string>>;
  };
  readonly composer: {
    readonly label: string;
    readonly placeholder: string;
    readonly send: string;
    readonly sendFailed: string;
    readonly rejected: Readonly<Record<RejectReason, string>>;
    /** Said in place of the generic refusal when the contact is recovering their account (ADR-0190 section 5): nothing is sent or queued. */
    readonly peerRecovering: (name: string) => string;
    readonly needsAction: Readonly<Record<UserActionCode, string>>;
  };
  /** Status lines for a relationship that is not active yet (first contact, ADR-0181). */
  readonly relationship: Readonly<Record<Exclude<RelationshipStateCode, "active">, string>>;
  /** Shown when requests from strangers are being dropped; people you invited can still reach you. */
  readonly pressure: { readonly notice: string };
  readonly request: {
    readonly heading: string;
    readonly unknownSender: string;
    /** One plain sentence on what the name means and when to accept. */
    readonly hint: string;
    /** Marks a requester whose name is self-chosen and not tied to any verified account. */
    readonly unverified: string;
    readonly accept: string;
    readonly ignore: string;
    readonly block: string;
    readonly blockConfirm: string;
    readonly blockCancel: string;
    readonly blockPrompt: string;
    readonly pending: string;
    readonly done: string;
    readonly rejected: Readonly<Record<RejectReason, string>>;
    readonly failed: string;
  };
  readonly notices: Readonly<Record<SecurityNoticeCode, string>>;
  /** Files, voice notes and location cards: plain words, no jargon. */
  readonly attachments: AttachmentCopy;
  readonly richText: {
    readonly opensExternally: string;
    readonly unsupported: string;
    readonly truncated: string;
    readonly attachment: string;
    readonly unsafeLink: string;
    /** Read after a mention that names the reader. */
    readonly mentionsYou: string;
  };
  /** The list of people a message can mention (ADR-0222). */
  readonly mentions: MentionsCopy;
  /** Device-local scheduled send (ADR-0224). */
  readonly schedule: ScheduleCopy;
}

export interface AttachmentCopy {
  /** Composer controls. Each names its own action (a voice note is not the Omnibar's voice command). */
  readonly attach: string;
  readonly recordVoice: string;
  readonly shareLocation: string;
  readonly removeStaged: (name: string) => string;
  /** The tray of things waiting to be sent with the message. */
  readonly tray: string;
  /** "12 KB", "3.4 MB". */
  readonly size: (bytes: number) => string;
  /** The file is over what the delivery service takes: said before anything is sent. */
  readonly tooBig: (maxBytes: number | null) => string;
  /** Typed refusals from Core when a file is prepared or opened. */
  readonly refusal: (r: AttachmentRefusal) => string;
  /** On a message. */
  readonly file: string;
  readonly uploading: string;
  readonly keptUntil: (when: string) => string;
  readonly download: (name: string) => string;
  readonly downloading: string;
  readonly downloaded: string;
  readonly formatDay: (epochMs: number) => string;
  /**
   * A file too big for the delivery service can be offered to go straight to the other person while they are online.
   * Without a way to do that on this device (`ConversationServices.directChannel` unset) every state says so plainly
   * instead of promising it (2026-11-14 decision: say what is true, never "waiting" for something that cannot happen).
   */
  readonly direct: {
    /** Refused before anything was sent: over what the delivery service takes. `limit` null: no limit known. */
    readonly tooBigNotSent: (limit: number | null) => string;
    /** Said when a way to send it straight to them exists. */
    readonly offer: string;
    /** The tray button that sends such a file straight to them. */
    readonly send: string;
    /** Said when no way exists on this device. */
    readonly unavailable: string;
    /** A file that goes straight to them cannot share a message with ordinary files. */
    readonly alone: string;
    /** Outgoing states. */
    readonly waiting: string;
    readonly waitingNoWay: string;
    readonly sending: string;
    /** Their device acknowledged every byte but nothing has checked it (ADR-0194): never worded as delivered. */
    readonly peerSaysReceived: string;
    /** Their device checked the file and has it (an authenticated receipt). */
    readonly delivered: string;
    /** They said they had it, the offer lapsed, and it was never confirmed. */
    readonly notConfirmed: string;
    /** The button beside `notConfirmed`, shown only when the host can start another send. */
    readonly sendAgain: string;
    readonly notOffered: string;
    readonly withdraw: string;
    readonly withdrawing: string;
    /** Incoming. */
    readonly availableWhileOnline: string;
    readonly cannotGet: string;
  };
  readonly voice: {
    readonly recording: string;
    readonly elapsed: (ms: number) => string;
    readonly stop: string;
    readonly discard: string;
    readonly note: string;
    readonly review: string;
    readonly play: string;
    readonly loading: string;
    readonly denied: string;
    readonly failed: string;
    readonly nothing: string;
    readonly duration: (ms: number) => string;
  };
  readonly location: {
    readonly heading: string;
    readonly asking: string;
    readonly confirm: string;
    readonly send: string;
    readonly cancel: string;
    readonly denied: string;
    readonly unavailable: string;
    readonly card: string;
    readonly accuracy: (m: number) => string;
    readonly point: (lat: string, lon: string) => string;
  };
}
