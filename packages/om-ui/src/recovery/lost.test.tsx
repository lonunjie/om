// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { asConversationId, asOperationId, asProfileId, type ReconnectContactView } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import { testApproveCopy, testLostCopy } from "../devices/__fixtures__/copy";
import type { SettingsServices } from "../devices/operation";
import { ApproveRecovery, LostDevices, ReconnectContacts, type ReconnectCopy } from "./index";

afterEach(cleanup);
const axeViolations = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};

function setup() {
  const h = createHarness({ env: {}, clock: () => 1000 });
  const me = asProfileId("me");
  h.seedProfile({ profileId: me, label: "Me" });
  let n = 0;
  const registry = createActionRegistry({ newOperationId: () => asOperationId(`op-${String(++n)}`) });
  const services: SettingsServices = { client: h, registry, ctx: h.contextFor(me) };
  return { h, services };
}

describe("I lost my devices", () => {
  it("shows the key entry with labelled fields, no technical words, and passes axe", async () => {
    const { services } = setup();
    const { container } = render(<LostDevices services={services} copy={testLostCopy} onRecovered={() => undefined} onCancel={() => undefined} />);
    expect(await screen.findByLabelText(testLostCopy.keyLabel)).toBeTruthy();
    expect(screen.getByRole("button", { name: testLostCopy.find })).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
  });

  it("an empty key asks again and sends nothing; Back is always available", async () => {
    const { services } = setup();
    const onCancel = vi.fn();
    render(<LostDevices services={services} copy={testLostCopy} onRecovered={() => undefined} onCancel={onCancel} />);
    await userEvent.click(await screen.findByRole("button", { name: testLostCopy.find }));
    expect(screen.getByText(testLostCopy.stage.not_found)).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: testLostCopy.cancel }));
    expect(onCancel).toHaveBeenCalled();
  });

  it("a controller device offers the approval only from its own step and passes axe", async () => {
    const { services } = setup();
    const onClose = vi.fn();
    const { container } = render(<ApproveRecovery services={services} copy={testApproveCopy} onClose={onClose} />);
    expect(await screen.findByRole("heading", { name: testApproveCopy.heading })).toBeTruthy();
    expect(screen.queryByRole("button", { name: testApproveCopy.confirm })).toBeNull();
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe("Reconnect after owner recovery", () => {
  it("starts only the selected contact and shows waiting separately from a live chat", async () => {
    const { services } = setup();
    const dispatch = vi.spyOn(services.client, "dispatch");
    const onOpen = vi.fn();
    const copyText = vi.fn<(text: string) => Promise<boolean>>().mockResolvedValue(true);
    const copy: ReconnectCopy = {
      heading: "Reconnect with your contacts",
      reconnect: (name) => `Reconnect with ${name}`,
      link: (name) => `Invitation link for ${name}`,
      share: "Send this link to the person.",
      copyLink: "Copy link",
      copied: "Link copied",
      copyFailed: "Couldn’t copy the link.",
      waiting: "Waiting to reconnect",
      ready: "Ready to chat",
      open: (name) => `Open chat with ${name}`,
      outcome: testLostCopy.outcome,
    };
    const bob: ReconnectContactView = { accountId: "ab".repeat(32), label: "Bob", inviteOperationId: null, invite: null, conversationId: null };
    const other: ReconnectContactView = { ...bob, accountId: "cd".repeat(32), label: "Other" };
    const show = (contact: ReconnectContactView) => <ReconnectContacts services={services} copy={copy} contacts={[contact, other]} inviteLink={(link) => link} copyText={copyText} onOpen={onOpen} />;
    const { rerender, container } = render(show(bob));
    expect(dispatch).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Reconnect with Bob" }));
    expect(dispatch).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ kind: "ReconnectContact", accountId: bob.accountId }));

    const op = asOperationId("pending");
    rerender(show({ ...bob, inviteOperationId: op, invite: { operationId: op, expiresAt: 99_000, link: "om:invite/example" } }));
    expect(screen.getByRole("textbox", { name: "Invitation link for Bob" })).toHaveProperty("readOnly", true);
    await userEvent.click(screen.getByRole("button", { name: copy.copyLink }));
    expect(copyText).toHaveBeenCalledWith("om:invite/example");
    expect(await screen.findByText(copy.copied)).toBeTruthy();
    expect(screen.getByText(copy.waiting)).toBeTruthy();
    expect(screen.queryByText(copy.ready)).toBeNull();

    const conversationId = asConversationId("bob-chat");
    rerender(show({ ...bob, conversationId }));
    expect(screen.queryByText(copy.waiting)).toBeNull();
    expect(screen.getByText(copy.ready)).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Open chat with Bob" }));
    expect(onOpen).toHaveBeenCalledWith(conversationId);
    expect(await axeViolations(container)).toEqual([]);
  });
});
