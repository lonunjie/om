import type { GroupsCopy } from "../copy";

export const groupsTestCopy: GroupsCopy = {
  list: { mark: (n) => `Group, ${String(n)} people`, noActivity: "No messages yet" },
  newGroup: {
    action: "New group", close: "Close", nameLabel: "Group name", namePlaceholder: "Name", unnamed: (names) => (typeof names === "number" ? "New group" : Array.from(names).join(", ")),
    pick: "Choose people", nobody: "Nobody to add yet", selected: (n) => `${String(n)} chosen`, create: "Create group", creating: "Creating", consent: "Each person is only added if they say yes.",
  },
  protection: {
    legend: "Protection", summary: (l) => `Protection: ${l}`,
    levels: { 1: { label: "Recommended", line: "Easiest." }, 2: { label: "Hardened", line: "Admins approve." }, 3: { label: "Maximum Privacy", line: "Admins approve devices too." } },
    custom: "Custom", later: "Stricter any time; looser is asked first.",
  },
  invites: {
    heading: "Group invitations", from: (who, group) => `${who} invited you to ${group}`, hint: "You will see only what is said after you join.",
    join: "Join", decline: "No thanks", joining: "Joining", declined: "Declined", joined: "Joined",
    waiting: { title: (g) => `Waiting to join ${g}`, body: "You said yes." },
  },
  held: {
    heading: "Group invitations waiting", title: (n) => (n === 1 ? "You have a group invitation" : `You have ${String(n)} group invitations`),
    from: (who, group) => (who === "" ? `You are invited to ${group}` : `${who} invited you to ${group}`), received: (ms) => `Received t${String(ms)}`,
    body: "You are not in the group yet.", open: (n) => (n === 1 ? "Open invitation" : "Open invitations"), more: (n) => `and ${String(n)} more`,
    overflow: { title: "Some group invitations couldn't be kept", invites: "Ask them to invite you again.", repliesTitle: "Some answers couldn't be kept", replies: "Invite them again.", dismiss: "Got it" },
  },
  chat: {
    label: "Group messages", back: "Back", people: (n) => `${String(n)} people`, openPeople: (g) => `People in ${g}`, empty: "No messages yet", incoming: "From", outgoing: "You said",
    formatTime: (ms) => `t${String(ms)}`, composerLabel: "Message the group", placeholder: "Message", send: "Send", sendFailed: "Not sent", paused: "Updating", notMember: "Not in this group", loadFailed: "Could not open",
  },
  sameName: {
    of: (n, i, o) => `${n} (${String(i)} of ${String(o)})`,
    here: (n, c) => `${String(c)} people here are named ${n}. A name does not show who someone is.`,
    contacts: (n, c) => `${String(c)} of your contacts are named ${n}.`,
    alike: (n, c) => `${String(c)} people here have names that look similar to ${n}. A name does not show who someone is.`,
  },
  members: {
    title: (g) => `People in ${g}`, close: "Close", listLabel: "People in this group", you: "You", someone: (n) => `Someone ${String(n)}`, roles: { admin: "Admin", member: "Member" }, invited: "Invited",
    removeShort: "Remove", remove: (n) => `Remove ${n}`, removeAsk: (n, g) => `Remove ${n} from ${g}?`, removeConfirm: "Yes, remove", removeKeep: "Keep",
    add: "Add someone", addNobody: "Everyone is here", addLabel: (n) => `Invite ${n}`, adding: "Inviting",
    leave: "Leave group", leaveAsk: (g) => `Leave ${g}?`, leaveConfirm: "Yes, leave", leaveStay: "Stay", leaveNote: "Others keep what they have.", removeNote: "They keep what they have.",
    historyNote: "Later joiners see only later messages.", proofNote: "Messages can be shown as proof.", settings: "Room settings", membersSeeNote: "Everyone can see who is in it.",
  },
  settings: {
    title: (g) => `Settings for ${g}`, close: "Close", back: "Back to people", level: "Protection level", current: "Current", useLevel: (l) => `Use ${l}`,
    rows: {
      add: { title: "Who can add people", anyone: "Anyone can add.", admins: "Only admins can add.", tighten: "Only admins can add people", tightenCost: "Adding may take longer.", loosen: "Let anyone add people" },
      join: { title: "Do new people need an admin's OK?", auto: "No.", approval: "Yes.", open: "Anyone can join.", tighten: "An admin approves each new person", tightenCost: "Joining can be delayed.", loosen: "Joining is automatic" },
      device: { title: "A member's new device", auto: "Automatic.", approval: "Admin approves.", tighten: "An admin approves new devices", tightenCost: "New devices wait.", loosen: "New devices join automatically" },
    },
    history: "New people do not see earlier messages.", timerOff: "Messages stay.", timerOn: (s) => `Messages disappear after ${String(s)} seconds.`,
    confirm: {
      heading: "Make this group less strict?", lead: "This will change:",
      lines: { add: "Anyone can add people.", join: "No admin's OK for new people.", device: "New devices join automatically.", history: "New people see earlier messages." },
      level: (l) => `Level goes down to ${l}.`, generic: "Less strict.", announce: "Everyone will see who changed it.", confirm: "Make less strict", keep: "Keep it as it is",
    },
    applied: "Changed.", saving: "Saving",
  },
  notices: {
    label: "Group setting changed",
    line: (who, field, value) => (value === null ? `${who} changed ${field}.` : `${who} changed ${field}: now ${value}.`),
    loosened: "Less strict",
    field: { members_may_invite: "who can add people", join_rule: "how new people join", new_device_policy: "new devices", _: "a setting" },
    value: (f, c) => (f === "members_may_invite" ? (c === "anyone" ? "anyone in the group" : "only admins") : f === "join_rule" ? (c === "admin_approval" ? "an admin approves" : "automatic") : f === "new_device_policy" ? c : null),
    youChanged: "You", someone: "Someone",
  },
  states: {
    label: "Group status",
    held: { title: "Waiting for a group update", body: "A group update is waiting." },
    undeliverable: { title: (n) => `${String(n)} not delivered`, body: "Send again.", dismiss: "OK" },
    settingBlocked: { title: (g) => `Settings change in ${g} blocked`, dismiss: "OK" },
    admitBlocked: { title: (w) => `${w} not added yet`, dismiss: "OK" },
    reason: (code, kind) => `${kind}: ${code}`,
  },
  refused: (code) => `Refused: ${code}`,
  done: { created: (g) => `Created ${g}`, invited: "Invited", removed: "Removed", left: "Left" },
};
