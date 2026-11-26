// SPDX-License-Identifier: MPL-2.0
import { createElement, Fragment } from "react";
import type { ServicesCopy } from "../copy";

const serviceTime = new Intl.DateTimeFormat("en", {
  dateStyle: "medium",
  timeStyle: "short",
});

export const servicesCopy: ServicesCopy = {
  heading: "Connected services",
  empty: "No connected services.",
  loading: "Checking services…",
  changing: "Waiting for the service action to finish…",
  loadFailed: "Couldn’t check connected services.",
  changeUnknown: "Couldn’t confirm what happened. Check the current state.",
  refresh: "Check current state",
  close: "Close",
  review: "Review",
  reviewName: (name) => `Review ${name}`,
  reviewTitle: "Review service",
  invocationTitle: "Review what will be shared",
  invocationReview: (name) => `Review what to share with ${name}`,
  status: {
    offered: "Available to install",
    installed: "Installed",
    revoked: "Permission revoked",
    removed: "Removed",
  },
  scopeLabel: "Where it’s installed",
  requestedScopeLabel: "Where it will be available",
  scope: {
    personal: "Personal",
    room: "Room",
    space: "Space",
  },
  spaceBoundary: "Installing in a Space alone gives no access to its Rooms.",
  modeLabel: "Data access",
  mode: {
    selected_context: "Invocation only",
    full_room_member: "Full Room member",
    local_personal: "Local component",
  },
  modeConsequence: {
    selected_context:
      "This service can read only what you choose to share when you use it.",
    full_room_member:
      "This service can read all Room content while it is a member.",
    local_personal:
      "This service runs on your device. Reading content and sending it off this device need separate permission.",
  },
  requested: "Requested permissions",
  granted: "Granted permissions",
  noCapabilities: "None",
  access: "What can this service read?",
  noRead: "This service has no read access here.",
  localRead: "This service can read the following on this device.",
  noLocalRead: "This service has no permission to read content on this device.",
  triggerFilter:
    "The trigger filter only limits when the service acts. It can still read all Room content.",
  previouslyShared: "The service may keep anything it already received.",
  addedBy: "Added by",
  addedFrom: "Added from",
  addedAt: "Added on",
  formatTime: (ms) => serviceTime.format(ms),
  author: {
    automation: "Automation",
    via: (person, service) =>
      createElement(Fragment, null, person, " · via ", service),
  },
  action: {
    install: "Install service",
    invoke: "Run service",
    revoke: "Revoke access",
    remove: "Remove service",
  },
  consequence: {
    revoke: "Revoke this service’s permission to act.",
    remove: "Remove this service from this installation.",
  },
  destination: "Who receives this",
  disclosure: "What leaves this device",
  disclosureExpires: "Request expires",
  shareAndRun: "Share and run",
  localDisclosure:
    "This invocation runs on this device. It sends no selected content to a remote service.",
  discloseOnce: "If you continue, you’ll send the listed information once.",
  item: {
    trigger: "Triggering message",
    message: "Selected message",
    attachment: "Selected attachment",
    draft: "Selected draft",
    thread_or_topic: "Selected thread or topic",
    text: "Text you entered",
    participant: "Participant information",
    scope: "Conversation information",
  },
  tainted: "This invocation has read conversation content.",
  untainted: "This invocation hasn’t read conversation content.",
  egress: {
    blocked_by_conversation:
      "The conversation’s rules block the service from sending data off this device.",
    blocked_by_capability:
      "The service has no permission to send data off this device.",
    allowed: "The service has permission to send data off this device.",
    not_requested: "The service hasn’t asked to send data off this device.",
  },
  outcome: {
    Committed: "Service action complete.",
    PendingAuthorization: "Waiting for approval.",
    PendingNetwork: "Waiting for a connection.",
    PendingReconciliation: "Checking whether the service action went through.",
  },
  rejected: {
    invalid: "That service request was invalid.",
    unsupported: "This version of OM can’t do that yet.",
    unauthorized: "You don’t have permission to do that here.",
    revoked: "That access was withdrawn.",
    expired: "That request expired.",
    stale_state: "The service state changed. Review the current details.",
    conflict: "That conflicts with an earlier change.",
    policy_denied: "The conversation’s rules don’t allow this action.",
    quota: "A usage limit was reached.",
    rate_limited: "Too many attempts. Wait a moment before trying again.",
    integrity_failure: "A safety check failed, so this action stopped.",
    unavailable: "This action is unavailable right now.",
    not_found: "OM couldn’t find that service request.",
    too_large: "The selected content is too large to share.",
    peer_recovering: "This person is recovering their account.",
  },
  needsAction: {
    authorize_with_device: "Approve this on another device you already use.",
    confirm_security_change:
      "Review and confirm the security change to continue.",
    reauthenticate: "Unlock OM again to continue.",
    update_client: "Update OM to continue.",
  },
};
