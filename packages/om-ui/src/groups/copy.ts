import type { ConversationCopy } from "../conversations/copy";

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
  /** The three protection levels (ADR-0189 section 3), by their existing names, each with what it means for the group in one line. */
  readonly protection: {
    /** Heading of the choice, and of the level in Room settings. */
    readonly legend: string;
    /** The collapsed choice at creation: "Protection: Recommended". */
    readonly summary: (levelLabel: string) => string;
    readonly levels: Readonly<Record<1 | 2 | 3, { readonly label: string; readonly line: string }>>;
    /** Shown instead of a level name when a Room's settings no longer match one level. */
    readonly custom: string;
    /** Said once under the choice: stricter later is easy, looser later is asked first. */
    readonly later: string;
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
    /** An accepted invitation whose add is not done yet. */
    readonly waiting: { readonly title: (group: string) => string; readonly body: string };
  };
  /**
   * Invitations a device holds before it has the group files (D70): a calm entry, never an acceptance. Tapping it gets the groups
   * ready and the ordinary invitation card then offers Join. `overflow` says, without alarm, that some could not be kept.
   */
  readonly held: {
    readonly heading: string;
    readonly title: (n: number) => string;
    readonly from: (inviter: string, group: string) => string;
    /** "Received Tuesday, 10:55 AM": when this device got it. */
    readonly received: (epochMs: number) => string;
    /** Said once: nothing was accepted, nobody was told. */
    readonly body: string;
    readonly open: (n: number) => string;
    /** Invitations beyond the ones listed. */
    readonly more: (n: number) => string;
    readonly overflow: {
      readonly title: string;
      /** What the person can do: ask to be invited again. */
      readonly invites: string;
      /** An answer (yes or no) that did not get kept; the person who invited can invite again. */
      readonly repliesTitle: string;
      readonly replies: string;
      readonly dismiss: string;
    };
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
    /** Reply, reactions, edit and delete are the chat's own words (the same strings, so a group speaks them as a chat does). */
    readonly reply: ConversationCopy["reply"];
    readonly react: ConversationCopy["react"];
    readonly change: ConversationCopy["change"];
  };
  /** Two people who show the same name (GROUPS_AND_SPACES_UX section 10): calm, one line, nothing to dismiss. */
  readonly sameName: {
    /** "Maria (1 of 2)": the name with its place among the people who share it. */
    readonly of: (name: string, index: number, of: number) => string;
    /** One line for a room or sheet where people share a name. */
    readonly here: (name: string, count: number) => string;
    /** The same for a list of your own contacts. */
    readonly contacts: (name: string, count: number) => string;
    /** Names that are not the same but look similar (ADR-0197); a calm hint, never an accusation. */
    readonly alike: (name: string, count: number) => string;
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
    /** The button that opens Room settings. */
    readonly settings: string;
    /** Said once in the details: membership is known to everyone in the group (ADR-0189 "Hide the member list" is not offered). */
    readonly membersSeeNote: string;
  };
  /** Room settings: tighten at once, loosen only after a confirmation that names what changes. */
  readonly settings: {
    readonly title: (group: string) => string;
    readonly close: string;
    /** Back to the people sheet. */
    readonly back: string;
    readonly level: string;
    /** "Current level" next to the level that is in force. */
    readonly current: string;
    readonly useLevel: (levelLabel: string) => string;
    readonly rows: {
      readonly add: { readonly title: string; readonly anyone: string; readonly admins: string; readonly tighten: string; readonly tightenCost: string; readonly loosen: string };
      readonly join: { readonly title: string; readonly auto: string; readonly approval: string; readonly open: string; readonly tighten: string; readonly tightenCost: string; readonly loosen: string };
      readonly device: { readonly title: string; readonly auto: string; readonly approval: string; readonly tighten: string; readonly tightenCost: string; readonly loosen: string };
    };
    /** Read-only: what this version does not let a person change here. */
    readonly history: string;
    readonly timerOff: string;
    readonly timerOn: (seconds: number) => string;
    /** One line per level cost shown before choosing it comes from `protection.levels`. */
    readonly confirm: {
      readonly heading: string;
      readonly lead: string;
      /** What a loosening changes, by key (see `LoosenKey`). */
      readonly lines: Readonly<Record<"add" | "join" | "device" | "history", string>>;
      readonly level: (levelLabel: string) => string;
      /** Used when Core asks for a confirmation the screen did not predict. */
      readonly generic: string;
      readonly announce: string;
      readonly confirm: string;
      readonly keep: string;
    };
    readonly applied: string;
    readonly saving: string;
  };
  /** Policy-change notices, shown as system lines in the timeline. */
  readonly notices: {
    readonly label: string;
    /** "Alice changed who can add people: now anyone in the group." */
    readonly line: (who: string, field: string, value: string | null) => string;
    /** Text marker on a notice that made the group less strict (never colour alone). */
    readonly loosened: string;
    readonly field: Readonly<Record<string, string>>;
    /** The words for the new value of a field; `null` when the code is not known here. */
    readonly value: (field: string, code: string) => string | null;
    readonly youChanged: string;
    readonly someone: string;
  };
  /** Honest states of the group carriage. A state names the thing, what it means and what happens next. */
  readonly states: {
    /** A group update is kept until what it needs arrives. */
    readonly held: { readonly title: string; readonly body: string };
    /** A delivery service refused for good: the messages will not arrive by themselves. */
    readonly undeliverable: { readonly title: (n: number) => string; readonly body: string; readonly dismiss: string };
    /** A queued settings change could not be carried out. */
    readonly settingBlocked: { readonly title: (group: string) => string; readonly dismiss: string };
    /** Someone said yes but the add is not done. */
    readonly admitBlocked: { readonly title: (who: string) => string; readonly dismiss: string };
    /** Plain words for the reason codes of the two blocked states. */
    readonly reason: (code: string, kind: "admit" | "setting") => string;
    readonly label: string;
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
