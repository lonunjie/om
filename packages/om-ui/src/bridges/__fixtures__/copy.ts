import { createElement, Fragment } from "react";
import type { BridgeCopy } from "../copy";

export const bridgeTestCopy: BridgeCopy = {
  heading: "Bridges",
  empty: "No bridges in this conversation.",
  loading: "Checking bridge status…",
  loadFailed: "Couldn’t check bridge status.",
  changing: "Requesting the bridge change…",
  changeUnknown: "OM couldn’t confirm the bridge change. Check its status.",
  refresh: "Check bridge status",
  review: "Review",
  reviewName: (bridge) => `Review ${bridge}`,
  reviewTitle: "Review bridge",
  close: "Close bridge review",
  action: { admit: "Add bridge", remove: "Remove bridge" },
  consequence: {
    admit:
      "The bridge can read all content shared while it is a member. Export limits only control what it can send to the other network.",
    remove:
      "Once removal is complete, the bridge loses access to new messages and future sharing stops. The bridge and the other network may keep copies they already received.",
  },
  membership: {
    proposed: "Bridge proposed",
    member: "Bridge is a full member",
    removed: "Bridge removed",
  },
  mapping: {
    active: "Bridging is active",
    revoked: "Bridge connection revoked",
    stale:
      "Sharing is paused. OM can’t confirm that the bridge connection is current.",
    endpoint_stopped: "Sharing through this bridge connection has stopped.",
  },
  approversAtInstall: "Approvers reported at install",
  approversAtProposal: "Approvers reported in the proposal",
  principal: "Bridge ID",
  network: "Connected network",
  scope: "Connected conversation",
  directionLabel: "Direction",
  direction: {
    outbound: "From OM to the other network",
    inbound: "From the other network to OM",
    both: "Both ways",
  },
  exports: "What OM shares with the other network",
  noExports: "Nothing",
  eventClass: {
    "message.create": "Messages and their attachments",
    "message.edit": "Message edits",
    "message.delete": "Message deletion requests",
    "reaction.set": "Reactions",
    "receipt.update": "Delivery and read receipts",
    "pin.set": "Changes to pinned messages",
    "moderation.redact": "Moderator deletion requests",
    "poll.create": "New polls",
    "poll.vote": "Poll votes",
    "poll.close": "Poll closures",
    "topic.create": "New topics",
    "topic.update": "Topic edits",
    "topic.status.set": "Topic status changes",
    "message.topic.set": "Messages moved between topics",
    "group.invite": "Group invitations",
    "group.accept": "Accepted group invitations",
    "group.decline": "Declined group invitations",
    "group.rejoin_request": "Requests to rejoin a group",
  },
  admissionBoundary:
    "For messages the bridge sends out, OM encryption will end at the bridge.",
  encryptionTo: (bridge) =>
    createElement(
      Fragment,
      null,
      "OM encryption protects this message to ",
      bridge,
      ".",
    ),
  externalSeparate: "Security on the other network is separate from OM.",
  externalClaim: "What the other network claims",
  noExternalClaim: "OM has no security claim for the other network.",
  outcome: {
    Committed: "Bridge change confirmed.",
    PendingAuthorization: "Waiting for approval.",
    PendingNetwork: "Waiting for a connection.",
    PendingReconciliation: "Checking whether the bridge change went through.",
  },
  rejected: {
    invalid: "OM couldn’t use these bridge details.",
    unsupported: "This version of OM doesn’t support this bridge change.",
    unauthorized: "You don’t have permission to make this bridge change.",
    revoked: "The authorization for this bridge change was withdrawn.",
    expired: "This bridge change has expired.",
    stale_state: "The bridge details changed. Review the current details.",
    conflict: "This bridge change conflicts with another change.",
    policy_denied: "The conversation’s rules prevent this bridge change.",
    quota: "This conversation has reached its bridge limit.",
    rate_limited: "Too many bridge changes were requested. Try again later.",
    integrity_failure: "OM couldn’t verify the bridge details.",
    unavailable: "This bridge change is unavailable right now.",
    not_found: "OM couldn’t find the bridge for this change.",
    too_large: "The bridge details exceed the size limit.",
    peer_recovering: "A participant is recovering their OM identity.",
  },
  needsAction: {
    authorize_with_device: "Approve this change on an authorized device.",
    confirm_security_change: "This bridge change needs security confirmation.",
    reauthenticate: "Unlock OM to continue this bridge change.",
    update_client: "Update OM to continue this bridge change.",
  },
  message: {
    author: "Message sent by bridge",
    foreignActor: "Name reported by the bridge",
    unknownActor: "Unknown sender",
    linkedIdentity: "Linked OM identity",
    noIdentityLink: "No current link",
    fidelity: "How the bridge carried this",
    flag: {
      exact: "Carried without changes.",
      lossy_formatting: "Some details could not be carried as written.",
      reply_target_unavailable: "The original message could not be linked.",
      edit_represented_as_new_message:
        "The bridge carried this edit as a new message.",
      delete_not_propagated:
        "The bridge couldn’t pass on the deletion request.",
      reaction_not_supported: "The bridge couldn’t pass on the reaction.",
      attachment_transcoded: "The bridge converted the attachment.",
      foreign_actor_unverified:
        "The sender’s identity on the other network is unverified.",
      origin_unverified: "The message’s original source is unverified.",
    },
    carry: {
      flattened:
        "The bridge carried this without its original thread or topic.",
      read_only_summary:
        "The bridge carried this as a read-only summary or link.",
      not_carried: "The bridge couldn’t carry this action.",
    },
  },
};
