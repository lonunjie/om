import type { RejectReason, SecurityNoticeCode, UserActionCode } from "@om/platform-api";
import type { ConversationCopy } from "../copy";

const REASONS: readonly RejectReason[] = ["invalid", "unsupported", "unauthorized", "revoked", "expired", "stale_state", "conflict", "policy_denied", "quota", "rate_limited", "integrity_failure", "unavailable", "not_found"];
const NOTICES: readonly SecurityNoticeCode[] = ["safety_number_changed", "device_added", "unverified_contact", "session_reset", "integrity_failure"];
const ACTIONS: readonly UserActionCode[] = ["authorize_with_device", "confirm_security_change", "reauthenticate", "update_client"];
const rec = <K extends string>(keys: readonly K[], f: (k: K) => string): Readonly<Record<K, string>> => Object.fromEntries(keys.map((k) => [k, f(k)])) as Record<K, string>;

export const testCopy: ConversationCopy = {
  list: { label: "Conversations", empty: "No conversations", noActivity: "No activity", unread: (n) => `${String(n)} unread`, securityNotice: "Security notice" },
  view: { label: "Messages", empty: "No messages", loadFailed: "Could not load", incoming: "Received:", outgoing: "Sent:", retry: "Retry sending", formatTime: (ms) => `t${String(ms)}` },
  delivery: {
    pending: "Sending", pendingNetwork: "Waiting for network", pendingReconciliation: "Confirming delivery", sent: "Sent", delivered: "Delivered",
    rejected: rec(REASONS, (r) => `Not sent: ${r}`),
  },
  composer: { label: "Message", placeholder: "Write a message", send: "Send", sendFailed: "Send failed", rejected: rec(REASONS, (r) => `Rejected: ${r}`), needsAction: rec(ACTIONS, (a) => `Action: ${a}`) },
  relationship: { awaiting_acceptance: "Waiting for acceptance", accepted_unbound: "Not verified yet", establishing: "Establishing" },
  pressure: { notice: "Many requests from strangers" },
  request: {
    heading: "Message request", unknownSender: "(not in your contacts)", unverified: "Unverified", accept: "Accept", ignore: "Ignore", block: "Block", blockConfirm: "Yes, block",
    blockCancel: "Cancel", blockPrompt: "Block this sender?", pending: "Pending", done: "Done", rejected: rec(REASONS, (r) => `Request rejected: ${r}`), failed: "Request failed",
  },
  notices: rec(NOTICES, (n) => `Notice: ${n}`),
  richText: { opensExternally: "(opens external site)", unsupported: "[unsupported content]", truncated: "[content truncated]", attachment: "[attachment]", unsafeLink: "[link disabled]" },
};
