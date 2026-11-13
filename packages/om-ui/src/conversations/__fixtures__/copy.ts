import type { RejectReason, SecurityNoticeCode, UserActionCode } from "@om/platform-api";
import type { ConversationCopy } from "../copy";

const REASONS: readonly RejectReason[] = ["invalid", "unsupported", "unauthorized", "revoked", "expired", "stale_state", "conflict", "policy_denied", "quota", "too_large", "rate_limited", "integrity_failure", "unavailable", "not_found"];
const NOTICES: readonly SecurityNoticeCode[] = ["safety_number_changed", "device_added", "unverified_contact", "session_reset", "integrity_failure"];
const ACTIONS: readonly UserActionCode[] = ["authorize_with_device", "confirm_security_change", "reauthenticate", "update_client"];
const rec = <K extends string>(keys: readonly K[], f: (k: K) => string): Readonly<Record<K, string>> => Object.fromEntries(keys.map((k) => [k, f(k)])) as Record<K, string>;

export const testCopy: ConversationCopy = {
  list: { label: "Conversations", empty: "No conversations", noActivity: "No activity", unread: (n) => `${String(n)} unread`, you: "You:", draft: "Draft:", securityNotice: "Security notice" },
  view: { label: "Messages", empty: "No messages", loadFailed: "Could not load", incoming: "Received:", outgoing: "Sent:", retry: "Retry sending", back: "Back to conversations", formatTime: (ms) => `t${String(ms)}` },
  delivery: {
    pending: "Sending", pendingNetwork: "Waiting for network", pendingReconciliation: "Confirming delivery", sent: "Sent", delivered: "Delivered",
    rejected: rec(REASONS, (r) => `Not sent: ${r}`),
  },
  composer: { label: "Message", placeholder: "Write a message", send: "Send", sendFailed: "Send failed", rejected: rec(REASONS, (r) => `Rejected: ${r}`), needsAction: rec(ACTIONS, (a) => `Action: ${a}`) },
  relationship: { awaiting_acceptance: "Waiting for acceptance", accepted_unbound: "Not verified yet", establishing: "Establishing", restored: "Restored from a backup file" },
  pressure: { notice: "Many requests from strangers" },
  request: {
    heading: "Message request", unknownSender: "(not in your contacts)", hint: "Only accept if you know who this is.", unverified: "Unverified", accept: "Accept", ignore: "Ignore", block: "Block", blockConfirm: "Yes, block",
    blockCancel: "Cancel", blockPrompt: "Block this sender?", pending: "Pending", done: "Done", rejected: rec(REASONS, (r) => `Request rejected: ${r}`), failed: "Request failed",
  },
  notices: rec(NOTICES, (n) => `Notice: ${n}`),
  attachments: {
    attach: "Attach a file", recordVoice: "Record a voice note", shareLocation: "Share your location", removeStaged: (n) => `Remove ${n}`, tray: "Ready to send",
    size: (b) => `${String(b)} bytes`, tooBig: (m) => (m === null ? "This file is too big to send right now." : `This file is too big to send right now. Up to ${String(m)} bytes.`),
    refusal: (r) => (r.error === "too_large" ? `Too big: ${String(r.max)}` : `Refused: ${r.error}`),
    file: "File", uploading: "Uploading", keptUntil: (w) => `Kept until ${w}`, download: (n) => `Download ${n}`.trim(), downloading: "Downloading", downloaded: "Download started", formatDay: (ms) => `day${String(ms)}`,
    direct: {
      tooBigNotSent: (l) => `Too big for the service, not sent. Up to ${String(l)}.`, offer: "Send it straight to them while they are online.", send: "Send it straight to them", unavailable: "Straight to them: not available here yet.",
      alone: "Straight-to-them files go alone.", waiting: "Waiting for them", waitingNoWay: "Not sent: no way to send straight to them here.", sending: "Sending now", delivered: "They have it", notOffered: "No longer offered",
      withdraw: "Take it back", withdrawing: "Taking it back", availableWhileOnline: "Available while they are online", cannotGet: "Offered by them; cannot receive here yet.",
    },
    voice: { recording: "Recording", elapsed: (ms) => `${String(Math.round(ms / 1000))}s`, stop: "Stop recording", discard: "Discard", note: "Voice note", review: "Listen before you send", play: "Play voice note", loading: "Opening voice note", denied: "Microphone blocked", failed: "Recording failed", nothing: "Nothing recorded", duration: (ms) => `${String(Math.round(ms / 1000))}s` },
    location: { heading: "Share your location?", asking: "Finding where you are", confirm: "This sends where you are right now, once.", send: "Send location", cancel: "Cancel", denied: "Location blocked", unavailable: "Location unavailable", card: "Location", accuracy: (m) => `About ${String(m)} m`, point: (a, o) => `${a}, ${o}` },
  },
  richText: { opensExternally: "(opens external site)", unsupported: "[unsupported content]", truncated: "[content truncated]", attachment: "[attachment]", unsafeLink: "[link disabled]" },
};
