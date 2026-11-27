// SPDX-License-Identifier: MPL-2.0
import type {
  ConnectedServicesService,
  ConnectedServicesView,
  ServiceEntryView,
  ServiceInvocationView,
} from "../services";

export const servicePrincipal = {
  id: "example-service",
  name: "Agenda assistant",
};
const room = { kind: "room", name: "Weekend plans" } as const;

export const fullMemberOffer: ServiceEntryView = {
  id: "room-offer",
  principal: servicePrincipal,
  status: "offered",
  mode: "full_room_member",
  scope: room,
  triggerClasses: ["message.create"],
  capabilities: [{ id: "message.create", label: "Post messages" }],
  readAccess: { kind: "none" },
  actions: [
    {
      availability: "ready",
      intent: { kind: "install", target: "room-offer-1" },
    },
  ],
};

export const fullMember: ServiceEntryView = {
  ...fullMemberOffer,
  id: "room-installation",
  status: "installed",
  readAccess: {
    kind: "full_room_member",
    roster: {
      installed: true,
      canReadAllContent: true,
      endpoints: ["example-endpoint"],
      filterClasses: [1],
      admittedVersion: 1,
      heldVersion: null,
    },
  },
  installedBy: { name: "Ari", device: "Laptop", atMs: 1763884800000 },
  actions: [
    { availability: "ready", intent: { kind: "revoke", target: "grant-1" } },
    {
      availability: "ready",
      intent: { kind: "remove", target: "room-installation-1" },
    },
  ],
};

export const invocation: ServiceInvocationView = {
  id: "example-invocation",
  principal: servicePrincipal,
  command: "Summarize the plan",
  disclosure: {
    kind: "selected_context",
    destination: "Agenda assistant",
    expiresAtMs: 1893456000000,
    items: [
      {
        id: "trigger",
        kind: "trigger",
        value: "@Agenda assistant summarize this message",
      },
      { id: "selected", kind: "message", value: "Meet at the station at 9." },
    ],
  },
  flow: { tainted: true, egress: "blocked_by_conversation" },
  action: {
    availability: "ready",
    intent: { kind: "invoke", target: "example-disclosure-1" },
  },
};

export type ServicePreviewMode = "install" | "active" | "slow" | "failed";

/** Dev gallery and component tests use sample data only. */
export function createServicesPreview(
  mode: ServicePreviewMode,
): ConnectedServicesService {
  let view: ConnectedServicesView = {
    entries: [mode === "install" ? fullMemberOffer : fullMember],
    invocation: mode === "active" ? invocation : null,
    outcome: null,
  };
  return {
    state: () => {
      if (mode === "slow")
        return new Promise<ConnectedServicesView>(() => {
          /* Keep loading visible. */
        });
      if (mode === "failed")
        return Promise.reject(new Error("Service preview unavailable"));
      return Promise.resolve(view);
    },
    change: (_room, intent) => {
      switch (intent.kind) {
        case "install":
          view = { ...view, entries: [fullMember] };
          break;
        case "revoke":
          view = {
            ...view,
            entries: [
              {
                ...fullMember,
                status: "revoked",
                actions: fullMember.actions.filter(
                  (action) =>
                    action.availability === "ready" &&
                    action.intent.kind === "remove",
                ),
              },
            ],
            invocation: null,
          };
          break;
        case "remove":
          view = {
            ...view,
            entries: [
              {
                ...fullMember,
                status: "removed",
                readAccess: { kind: "none" },
                capabilities: [],
                actions: [],
              },
            ],
            invocation: null,
          };
          break;
        case "invoke":
          view = { ...view, invocation: null };
          break;
      }
      view = { ...view, outcome: { kind: "Committed" } };
      return Promise.resolve(view);
    },
  };
}
