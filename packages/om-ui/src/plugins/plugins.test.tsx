// @vitest-environment jsdom
// SPDX-License-Identifier: MPL-2.0
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PluginSettings } from "./index";
import { pluginErrorCode, pluginUiLimits } from "./plugins";
import type { PluginSettingsView, PluginUiNodeView, PluginsService } from "./plugins";
import { pluginsCopy as copy } from "./__fixtures__/copy";
import { activePlugin, createPluginsPreview, pluginInstallation, pluginSend, updateReview } from "./__fixtures__/service";

afterEach(cleanup);

function fake(view = activePlugin) {
  return {
    state: vi.fn<PluginsService["state"]>().mockResolvedValue(view),
    change: vi.fn<PluginsService["change"]>().mockResolvedValue(view),
  };
}

async function violations(element: Element) {
  const report = await axe.run(element, { rules: { "color-contrast": { enabled: false } } });
  return report.violations.map(({ id }) => id);
}

function dialog(name: string) {
  return screen.getByRole("dialog", { name });
}

function permission(list: HTMLElement, label: string) {
  const row = within(list).getAllByRole("listitem").find((item) => item.textContent.startsWith(label));
  if (row === undefined) throw new Error(`Permission row missing: ${label}`);
  return within(row);
}

describe("plugin settings", () => {
  it("renders only when the host injects a service", () => {
    const { container } = render(<PluginSettings copy={copy} />);
    expect(container.firstChild).toBeNull();
  });

  it("reviews and installs a release with unapproved permissions disabled", async () => {
    const user = userEvent.setup();
    const preview = createPluginsPreview("install");
    const change = vi.spyOn(preview, "change");
    render(<PluginSettings service={preview} copy={copy} />);
    await user.click(await screen.findByRole("button", { name: copy.review.install }));
    const sheet = dialog(copy.title.install);
    expect(change).not.toHaveBeenCalled();
    expect(within(sheet).getByText(copy.readDoesNotAllowEgress)).toBeDefined();
    const send = permission(sheet, copy.capability["conversation.send_message"]);
    expect(send.getByText(copy.disabled)).toBeDefined();
    expect(send.getByText(copy.approvalRequired)).toBeDefined();
    expect(await violations(sheet)).toEqual([]);
    await user.click(within(sheet).getByRole("button", { name: copy.action.install }));
    expect(change).toHaveBeenCalledExactlyOnceWith({ kind: "install", target: "note-tools-v1" });
    expect(await screen.findByText(copy.runtime.active)).toBeDefined();
    expect(screen.queryByText(copy.granted)).toBeNull();
  });

  it("carries forward existing grants and requires a separate approval for a new permission", async () => {
    const user = userEvent.setup();
    const preview = createPluginsPreview("active");
    const change = vi.spyOn(preview, "change");
    render(<PluginSettings service={preview} copy={copy} />);
    await user.click(await screen.findByRole("button", { name: copy.review.update }));
    const sheet = dialog(copy.title.update);
    const network = copy.capability["net.fetch_origin_scoped"];
    expect(permission(sheet, copy.capability["content.read_selected"]).getByText(copy.granted)).toBeDefined();
    for (const label of [network, copy.capability["service.invoke"]]) {
      expect(permission(sheet, label).getByText(copy.disabled)).toBeDefined();
      expect(permission(sheet, label).getByText(copy.beyondCeiling)).toBeDefined();
    }
    expect(within(sheet).getByText("https://notes.example.org")).toBeDefined();
    expect(await violations(sheet)).toEqual([]);
    await user.click(within(sheet).getByRole("button", { name: copy.action.update }));
    expect(change).toHaveBeenCalledExactlyOnceWith({ kind: "update", target: updateReview.release.id });
    const settings = screen.getByRole("region", { name: copy.heading });
    const review = await screen.findByRole("button", { name: copy.reviewGrant(network) });
    expect(permission(settings, network).getByText(copy.disabled)).toBeDefined();
    await user.click(review);
    const grant = dialog(copy.title.approve);
    expect(within(grant).getByText("https://notes.example.org")).toBeDefined();
    expect(await violations(grant)).toEqual([]);
    await user.click(within(grant).getByRole("button", { name: copy.action.approve }));
    expect(change).toHaveBeenLastCalledWith({ kind: "approve", target: "note-tools-v2-website" });
    await waitFor(() => { expect(permission(settings, network).getByText(copy.granted)).toBeDefined(); });
  });

  it("shows separately why a host-blocked update and its permissions are unavailable", async () => {
    const user = userEvent.setup();
    const service = fake({ ...activePlugin, releaseReview: {
      ...updateReview, action: { availability: "blocked", kind: "update", reason: pluginErrorCode.releaseNotAuthorized },
    } });
    render(<PluginSettings service={service} copy={copy} />);
    await user.click(await screen.findByRole("button", { name: copy.review.update }));
    const sheet = dialog(copy.title.update);
    const button = within(sheet).getByRole<HTMLButtonElement>("button", { name: copy.action.update, description: copy.errors[11] });
    expect(button.disabled).toBe(true);
    expect(within(sheet).getAllByText(copy.beyondCeiling)).toHaveLength(2);
    await user.click(button);
    expect(service.change).not.toHaveBeenCalled();
  });

  it("shows host taint policy for a 1:1 and platform isolation refusal", async () => {
    const service = fake();
    const { container, rerender } = render(<PluginSettings service={service} copy={copy} />);
    expect(await screen.findByText(copy.tainted)).toBeDefined();
    expect(screen.getByText(copy.egress.blocked)).toBeDefined();
    expect(screen.getByText(copy.conversationPolicy.one_to_one)).toBeDefined();
    expect(await violations(container)).toEqual([]);
    const disabled = fake({ ...activePlugin, send: null, installation: {
      ...pluginInstallation, runtime: { kind: "isolation_unavailable" },
    } });
    rerender(<PluginSettings service={disabled} copy={copy} />);
    expect(await screen.findByText(copy.runtime.isolation_unavailable)).toBeDefined();
    expect(screen.queryByRole("region", { name: copy.pluginControls })).toBeNull();
    expect(await violations(container)).toEqual([]);
  });

  it("keeps exact destination and text in the host sheet and confirms each send separately", async () => {
    const user = userEvent.setup();
    const preview = createPluginsPreview("active");
    const change = vi.spyOn(preview, "change");
    render(<PluginSettings service={preview} copy={copy} />);
    const open = await screen.findByRole("button", { name: copy.review.confirm_send });
    open.focus();
    await user.keyboard("{Enter}");
    const sheet = dialog(copy.title.confirm_send);
    expect(document.activeElement).toBe(within(sheet).getByRole("heading", { name: activePlugin.name }));
    expect(within(sheet).getByText(pluginSend.destination)).toBeDefined();
    expect(within(sheet).getByText(pluginSend.content, { normalizer: (value) => value }).textContent).toBe(pluginSend.content);
    expect(change).not.toHaveBeenCalled();
    expect(await violations(sheet)).toEqual([]);
    await user.keyboard("{Escape}");
    expect(document.activeElement).toBe(open);
    expect(change).not.toHaveBeenCalled();
    await user.keyboard("{Enter}");
    await user.click(within(dialog(copy.title.confirm_send)).getByRole("button", { name: copy.action.confirm_send }));
    expect(change).toHaveBeenCalledExactlyOnceWith({ kind: "confirm_send", target: "send-1" });
    await waitFor(() => { expect(screen.queryByRole("button", { name: copy.review.confirm_send })).toBeNull(); });
    await user.click(screen.getByRole("button", { name: "Prepare a message" }));
    const next = await screen.findByRole("button", { name: copy.review.confirm_send });
    expect(change).toHaveBeenCalledTimes(2);
    await user.click(next);
    await user.click(within(dialog(copy.title.confirm_send)).getByRole("button", { name: copy.action.confirm_send }));
    expect(change).toHaveBeenLastCalledWith({ kind: "confirm_send", target: "send-2" });
  });

  it("cancels a pending send through its separate host intent", async () => {
    const user = userEvent.setup();
    const service = fake();
    render(<PluginSettings service={service} copy={copy} />);
    await user.click(await screen.findByRole("button", { name: copy.review.confirm_send }));
    await user.click(within(dialog(copy.title.confirm_send)).getByRole("button", { name: copy.action.cancel_send }));
    expect(service.change).toHaveBeenCalledExactlyOnceWith({ kind: "cancel_send", target: "send-1" });
  });

  it("uses separate typed intents and returned state to revoke access and remove a plugin", async () => {
    const user = userEvent.setup();
    const preview = createPluginsPreview("active");
    const change = vi.spyOn(preview, "change");
    render(<PluginSettings service={preview} copy={copy} />);
    await user.click(await screen.findByRole("button", { name: copy.action.revoke }));
    const revoke = dialog(copy.title.revoke);
    expect(within(revoke).getByText(copy.consequence.revoke)).toBeDefined();
    expect(change).not.toHaveBeenCalled();
    expect(await violations(revoke)).toEqual([]);
    await user.click(within(revoke).getByRole("button", { name: copy.action.revoke }));
    expect(change).toHaveBeenCalledExactlyOnceWith({ kind: "revoke", target: "note-tools-grants" });
    expect(await screen.findByText(copy.runtime.revoked)).toBeDefined();
    expect(screen.queryByRole("region", { name: copy.pluginControls })).toBeNull();
    await user.click(screen.getByRole("button", { name: copy.action.remove }));
    const remove = dialog(copy.title.remove);
    expect(within(remove).getByText(copy.consequence.remove)).toBeDefined();
    await user.click(within(remove).getByRole("button", { name: copy.action.remove }));
    expect(change).toHaveBeenLastCalledWith({ kind: "remove", target: "note-tools-installation" });
    expect(await screen.findByText(copy.empty)).toBeDefined();
  });

  it.each(Object.values(pluginErrorCode))("announces host error %i with dedicated copy", async (code) => {
    render(<PluginSettings service={fake({ ...activePlugin, outcome: { kind: "Rejected", code } })} copy={copy} />);
    const alert = await screen.findByRole("alert");
    expect((alert).textContent).toBe(copy.errors[code]);
    expect(alert.getAttribute("data-reason-code")).toBe(String(code));
  });

  it("announces a slow check and allows a failed read to be checked again", async () => {
    const user = userEvent.setup();
    const slow = createPluginsPreview("slow");
    const { container, rerender } = render(<PluginSettings service={slow} copy={copy} />);
    expect((screen.getByRole("status")).textContent).toBe(copy.loading);
    expect(await violations(container)).toEqual([]);
    const service = fake();
    service.state.mockRejectedValueOnce(new Error("fixture read failure"));
    rerender(<PluginSettings service={service} copy={copy} />);
    expect((await screen.findByRole("alert")).textContent).toBe(copy.loadFailed);
    expect(await violations(container)).toEqual([]);
    await user.click(screen.getByRole("button", { name: copy.refresh }));
    expect(await screen.findByText(copy.runtime.active)).toBeDefined();
    expect(service.state).toHaveBeenCalledTimes(2);
    expect(service.change).not.toHaveBeenCalled();
  });

  it("does not reload or resubmit a change with an unknown or pending outcome", async () => {
    const user = userEvent.setup();
    const service = fake();
    service.change.mockRejectedValueOnce(new Error("fixture change acknowledgement lost"));
    render(<PluginSettings service={service} copy={copy} />);
    await user.click(await screen.findByRole("button", { name: copy.review.confirm_send }));
    await user.click(within(dialog(copy.title.confirm_send)).getByRole("button", { name: copy.action.confirm_send }));
    expect((await screen.findByRole("alert")).textContent).toBe(copy.changeUnknown);
    await user.click(screen.getByRole("button", { name: copy.review.confirm_send }));
    expect(within(dialog(copy.title.confirm_send)).getByRole<HTMLButtonElement>("button", { name: copy.action.confirm_send, description: copy.changeUnknown }).disabled).toBe(true);
    await user.keyboard("{Escape}");
    service.state.mockResolvedValueOnce({ ...activePlugin, outcome: { kind: "PendingReconciliation" } });
    await user.click(screen.getByRole("button", { name: copy.refresh }));
    await waitFor(() => {
      expect(screen.getAllByRole("status").some((status) => status.textContent === copy.outcome.PendingReconciliation)).toBe(true);
    });
    expect(service.state).toHaveBeenCalledTimes(2);
    expect(service.change).toHaveBeenCalledTimes(1);
  });
});

function treeView(nodes: readonly PluginUiNodeView[]): PluginSettingsView {
  return { ...activePlugin, installation: { ...pluginInstallation, runtime: {
    kind: "active", taint: null, settings: { id: "test-tree", surface: "settings", nodes },
  } } };
}

describe("declarative plugin controls", () => {
  it("renders inert text and dispatches only typed node interactions under plugin-labelled chrome", async () => {
    const user = userEvent.setup();
    const content = "<strong>Identity warning</strong>";
    const service = fake(treeView([
      { id: "root", kind: "group" },
      { id: "card", kind: "card", parent: "root", text: "Sample card" },
      { id: "text", kind: "text", parent: "card", text: content },
      { id: "control", kind: "button", parent: "card", text: copy.action.confirm_send, action: "prepare" },
      { id: "disabled", kind: "button", parent: "card", text: "Unavailable action" },
      { id: "form", kind: "form", parent: "card", text: "Form settings", action: "save" },
      { id: "input", kind: "input", parent: "form", text: "Name to use" },
    ]));
    render(<PluginSettings service={service} copy={copy} />);
    const panel = await screen.findByRole("region", { name: copy.pluginControls });
    expect(within(panel).getByText(content)).toBeDefined();
    expect(panel.querySelector("strong")).toBeNull();
    expect(within(panel).queryByRole("alert")).toBeNull();
    expect(within(panel).queryByRole("dialog")).toBeNull();
    const disabled = within(panel).getByRole<HTMLButtonElement>("button", { name: "Unavailable action", description: copy.pluginControlUnavailable });
    expect(disabled.disabled).toBe(true);
    expect(await violations(panel)).toEqual([]);
    await user.click(within(panel).getByRole("button", { name: copy.action.confirm_send }));
    expect(service.change).toHaveBeenLastCalledWith({ kind: "ui_interaction", event: "activate", nodeId: "control", target: "test-tree" });
    await user.type(within(panel).getByRole("textbox", { name: "Name to use" }), "Jo");
    expect(service.change).toHaveBeenCalledTimes(1);
    await user.click(within(panel).getByRole("button", { name: copy.pluginSubmit }));
    expect(service.change).toHaveBeenLastCalledWith({ kind: "ui_interaction", event: "submit", nodeId: "form", target: "test-tree", fields: [{ nodeId: "input", value: "Jo" }] });
  });

  it.each([
    ["cycles", [{ id: "root", kind: "group" }, { id: "cycle", parent: "cycle", kind: "group" }]],
    ["too many nodes", Array.from({ length: pluginUiLimits.nodes + 1 }, (_, index) => ({ id: String(index), kind: "text" }))],
    ["too much nesting", Array.from({ length: pluginUiLimits.depth + 1 }, (_, index) => index === 0
      ? { id: "0", kind: "group" } : { id: String(index), parent: String(index - 1), kind: "group" })],
  ] as const satisfies readonly (readonly [string, readonly PluginUiNodeView[]])[])("shows a visible error for %s in a returned tree", async (_label, nodes) => {
    render(<PluginSettings service={fake(treeView(nodes))} copy={copy} />);
    expect((await screen.findByRole("alert")).textContent).toBe(copy.errors[12]);
    expect(screen.queryByRole("region", { name: copy.pluginControls })).toBeNull();
  });
});
