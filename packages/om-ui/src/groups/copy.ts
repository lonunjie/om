/** All user-visible strings for ordinary groups. Components contain no copy; callers supply (localized) text. */
export interface GroupsCopy {
  readonly list: {
    /** Accessible name of the marker on a group row: "Group, 3 people". */
    readonly mark: (people: number) => string;
    readonly noActivity: string;
  };
  readonly newGroup: {
    /** The list-header button, the Omnibar action and the sheet title. */
    readonly action: string;
    readonly close: string;
    readonly nameLabel: string;
    readonly namePlaceholder: string;
    /** Shown for a group with no name: "Maya, Joe and Ana". */
    readonly unnamed: (names: readonly string[]) => string;
    readonly pick: string;
    /** Nobody to put in a group yet: groups are made from people you already chat with. */
    readonly nobody: string;
    readonly selected: (n: number) => string;
    readonly create: string;
    readonly creating: string;
    /** Said under the button: nobody is added until they say yes. */
    readonly consent: string;
  };
  readonly invites: {
    readonly heading: string;
    readonly from: (inviter: string, group: string) => string;
    /** What joining means, in plain words. */
    readonly hint: string;
    readonly join: string;
    readonly decline: string;
    readonly joining: string;
    readonly declined: string;
    readonly joined: string;
  };
  readonly chat: {
    readonly label: string;
    readonly back: string;
    readonly people: (n: number) => string;
    readonly openPeople: (group: string) => string;
    readonly empty: string;
    readonly incoming: string;
    readonly outgoing: string;
    readonly formatTime: (epochMs: number) => string;
    readonly composerLabel: string;
    readonly placeholder: string;
    readonly send: string;
    readonly sendFailed: string;
    /** `canSend` is false while the group is changing or this person is leaving. */
    readonly paused: string;
    readonly notMember: string;
    readonly loadFailed: string;
  };
  readonly members: {
    readonly title: (group: string) => string;
    readonly close: string;
    readonly listLabel: string;
    readonly you: string;
    /** A person the screen has no name for yet. */
    readonly someone: (n: number) => string;
    readonly roles: Readonly<Record<string, string>>;
    readonly invited: string;
    /** Visible label of the row button, and its full accessible name. */
    readonly removeShort: string;
    readonly remove: (name: string) => string;
    readonly removeAsk: (name: string, group: string) => string;
    readonly removeConfirm: string;
    readonly removeKeep: string;
    readonly add: string;
    readonly addNobody: string;
    readonly addLabel: (name: string) => string;
    readonly adding: string;
    readonly leave: string;
    readonly leaveAsk: (group: string) => string;
    readonly leaveConfirm: string;
    readonly leaveStay: string;
    /** What leaving and removing do not do. */
    readonly leaveNote: string;
    readonly removeNote: string;
    /** People who join later see only what is said after they join. */
    readonly historyNote: string;
    /** Messages in a group can be shown to others as proof of who wrote them (ADR-0138: no deniability). */
    readonly proofNote: string;
  };
  /** Plain words for what Core refused; `code` is its machine code. */
  readonly refused: (code: string) => string;
  /** Said once a step went through, for screen readers. */
  readonly done: {
    readonly created: (group: string) => string;
    readonly invited: string;
    readonly removed: string;
    readonly left: string;
  };
}
