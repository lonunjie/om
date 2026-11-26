// SPDX-License-Identifier: MPL-2.0
import type {
  PluginAction, PluginActionKind, PluginInstallationView, PluginPermissionsView, PluginReleaseReview,
  PluginReleaseView, PluginSendView, PluginSettingsView, PluginUiTreeView, PluginsService,
} from "../plugins";

const ready = <Kind extends PluginActionKind>(kind: Kind, target: string): PluginAction<Kind> => ({
  availability: "ready", intent: { kind, target },
});

const release: PluginReleaseView = {
  id: "note-tools-v1", seq: 1, label: "1.0",
  requested: ["ui.settings_page", "content.read_selected", "conversation.send_message"],
};

const settings: PluginUiTreeView = {
  id: "note-tools-settings", surface: "settings",
  nodes: [
    { id: "root", kind: "group" },
    { id: "card", kind: "card", parent: "root", text: "Message template" },
    { id: "info", kind: "text", parent: "card", text: "Enter a sign-off to use in the template." },
    { id: "form", kind: "form", parent: "card", text: "Sign-off settings", action: "save-signoff" },
    { id: "input", kind: "input", parent: "form", text: "Sign-off" },
    { id: "preview", kind: "button", parent: "card", text: "Prepare a message", action: "prepare" },
  ],
};

const revoke = ready("revoke", "note-tools-grants");
const remove = ready("remove", "note-tools-installation");
const permissionRequired = { kind: "disabled", reason: "approval_required" } as const;
const permissions: PluginPermissionsView = {
  permissions: {
    "ui.settings_page": { kind: "granted" },
    "content.read_selected": { kind: "granted" },
    "conversation.send_message": { kind: "granted" },
  },
  requestedOrigins: [],
};

export const pluginInstallation: PluginInstallationView = {
  ...permissions, release,
  runtime: { kind: "active", taint: { conversation: "one_to_one", egress: "blocked" }, settings },
  grantReviews: [], actions: [revoke, remove],
};

export const installReview: PluginReleaseReview = {
  kind: "install", release,
  permissions: {
    "ui.settings_page": permissionRequired,
    "content.read_selected": permissionRequired,
    "conversation.send_message": permissionRequired,
  },
  requestedOrigins: [], action: ready("install", release.id),
};

export const updateReview: Extract<PluginReleaseReview, { kind: "update" }> = {
  kind: "update",
  release: {
    id: "note-tools-v2", seq: 2, label: "2.0",
    requested: [...release.requested, "net.fetch_origin_scoped", "service.invoke"],
  },
  carry: { ceiling: release.requested, disabledUntilGranted: ["net.fetch_origin_scoped", "service.invoke"] },
  permissions: {
    ...permissions.permissions,
    "net.fetch_origin_scoped": permissionRequired,
    "service.invoke": permissionRequired,
  },
  requestedOrigins: ["https://notes.example.org"], action: ready("update", "note-tools-v2"),
};

export const pluginSend: PluginSendView = {
  id: "send-1", destination: "Weekend plans · Ari and Jo", content: "Meet at 9.\n\nBring the notes.  ",
  confirm: ready("confirm_send", "send-1"), cancel: ready("cancel_send", "send-1"),
};

export const activePlugin: PluginSettingsView = {
  name: "Note tools", publisher: "Example Studio", installation: pluginInstallation,
  releaseReview: updateReview, send: pluginSend, outcome: null,
};

export type PluginsPreviewMode = "active" | "install" | "slow" | "failed";

/** Dev gallery and component tests use sample views only. */
export function createPluginsPreview(mode: PluginsPreviewMode): PluginsService {
  let view: PluginSettingsView = mode === "install"
    ? { ...activePlugin, installation: null, releaseReview: installReview, send: null }
    : activePlugin;
  let sendNumber = 1;
  const websiteGrant = {
    id: "note-tools-v2-website",
    capability: "net.fetch_origin_scoped",
    scope: "https://notes.example.org",
    action: ready("approve", "note-tools-v2-website"),
  } as const;
  return {
    state: () => {
      if (mode === "slow") return new Promise(() => { /* Keep loading visible. */ });
      if (mode === "failed") return Promise.reject(new Error("Plugin preview unavailable"));
      return Promise.resolve(view);
    },
    change: (intent) => {
      switch (intent.kind) {
        case "install":
          view = {
            ...view, releaseReview: null,
            installation: {
              ...pluginInstallation, permissions: installReview.permissions,
              runtime: { kind: "active", taint: null, settings: null },
              grantReviews: release.requested.map((capability) => ({
                id: capability, capability,
                scope: capability === "conversation.send_message" ? pluginSend.destination : "This device",
                action: ready("approve", capability),
              })),
            },
          };
          break;
        case "update":
          view = {
            ...view, releaseReview: null, send: null,
            installation: {
              ...pluginInstallation, release: updateReview.release, carry: updateReview.carry,
              permissions: updateReview.permissions, requestedOrigins: updateReview.requestedOrigins,
              grantReviews: [websiteGrant],
            },
          };
          break;
        case "approve": {
          const installation = view.installation;
          const grant = installation?.grantReviews.find(({ id }) => id === intent.target);
          if (installation !== null && grant !== undefined) view = {
            ...view, installation: {
              ...installation,
              permissions: { ...installation.permissions, [grant.capability]: { kind: "granted" } },
              grantReviews: installation.grantReviews.filter(({ id }) => id !== intent.target),
              runtime: grant.capability === "ui.settings_page" ? { kind: "active", taint: null, settings } : installation.runtime,
            },
          };
          break;
        }
        case "revoke": {
          const installation = view.installation;
          if (installation !== null) view = { ...view, releaseReview: null, send: null, installation: {
            ...installation, runtime: { kind: "revoked" },
            permissions: Object.fromEntries(installation.release.requested.map((capability) => [capability, { kind: "disabled", reason: 3 }])),
            actions: [remove], grantReviews: [],
          } };
          break;
        }
        case "remove": view = { ...view, installation: null, releaseReview: null, send: null }; break;
        case "confirm_send":
        case "cancel_send": view = { ...view, send: null }; break;
        case "ui_interaction": {
          if (intent.event === "activate" && intent.nodeId === "preview" && view.send === null &&
              view.installation?.permissions["conversation.send_message"]?.kind === "granted") {
            const id = `send-${String(++sendNumber)}`;
            view = { ...view, send: { ...pluginSend, id, confirm: ready("confirm_send", id), cancel: ready("cancel_send", id) } };
          }
          break;
        }
      }
      view = { ...view, outcome: { kind: "Committed" } };
      return Promise.resolve(view);
    },
  };
}
