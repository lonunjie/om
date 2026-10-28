import type { RejectReason, RelationshipStateCode, SecurityNoticeCode, UserActionCode } from "@om/platform-api";

/** All user-visible strings for the conversation slice. Components contain no copy; callers supply (localized) text. */
export interface ConversationCopy {
  readonly list: {
    readonly label: string;
    readonly empty: string;
    readonly noActivity: string;
    readonly unread: (count: number) => string;
    readonly securityNotice: string;
  };
  readonly view: {
    readonly label: string;
    readonly empty: string;
    readonly loadFailed: string;
    readonly incoming: string;
    readonly outgoing: string;
    readonly retry: string;
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
  readonly richText: {
    readonly opensExternally: string;
    readonly unsupported: string;
    readonly truncated: string;
    readonly attachment: string;
    readonly unsafeLink: string;
  };
}
