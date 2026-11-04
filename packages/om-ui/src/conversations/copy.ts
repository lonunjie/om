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
  readonly delivery: {
    readonly pending: string;
    readonly pendingNetwork: string;
    readonly pendingReconciliation: string;
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
  };
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
