import type { GroupsCopy } from "../copy";

export const groupsTestCopy: GroupsCopy = {
  list: { mark: (n) => `Group, ${String(n)} people`, noActivity: "No messages yet" },
  newGroup: {
    action: "New group", close: "Close", nameLabel: "Group name", namePlaceholder: "Name", unnamed: (names) => (typeof names === "number" ? "New group" : Array.from(names).join(", ")),
    pick: "Choose people", nobody: "Nobody to add yet", selected: (n) => `${String(n)} chosen`, create: "Create group", creating: "Creating", consent: "Each person is only added if they say yes.",
  },
  invites: {
    heading: "Group invitations", from: (who, group) => `${who} invited you to ${group}`, hint: "You will see only what is said after you join.",
    join: "Join", decline: "No thanks", joining: "Joining", declined: "Declined", joined: "Joined",
  },
  chat: {
    label: "Group messages", back: "Back", people: (n) => `${String(n)} people`, openPeople: (g) => `People in ${g}`, empty: "No messages yet", incoming: "From", outgoing: "You said",
    formatTime: (ms) => `t${String(ms)}`, composerLabel: "Message the group", placeholder: "Message", send: "Send", sendFailed: "Not sent", paused: "Updating", notMember: "Not in this group", loadFailed: "Could not open",
  },
  members: {
    title: (g) => `People in ${g}`, close: "Close", listLabel: "People in this group", you: "You", someone: (n) => `Someone ${String(n)}`, roles: { admin: "Admin", member: "Member" }, invited: "Invited",
    removeShort: "Remove", remove: (n) => `Remove ${n}`, removeAsk: (n, g) => `Remove ${n} from ${g}?`, removeConfirm: "Yes, remove", removeKeep: "Keep",
    add: "Add someone", addNobody: "Everyone is here", addLabel: (n) => `Invite ${n}`, adding: "Inviting",
    leave: "Leave group", leaveAsk: (g) => `Leave ${g}?`, leaveConfirm: "Yes, leave", leaveStay: "Stay", leaveNote: "Others keep what they have.", removeNote: "They keep what they have.",
    historyNote: "Later joiners see only later messages.", proofNote: "Messages can be shown as proof.",
  },
  refused: (code) => `Refused: ${code === "" ? "?" : "try again"}`,
  done: { created: (g) => `Created ${g}`, invited: "Invited", removed: "Removed", left: "Left" },
};
