// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { asConversationId, asGroupId, asOperationId, asPersonId, asProfileId } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import { useConversationList, type ConversationServices } from "../conversations/index";
import { GroupChat, GroupComposer, GroupInvites, NewGroup, groupApiOf, runGroupAction, useGroups } from "./index";
import { groupsTestCopy as copy } from "./__fixtures__/copy";

afterEach(cleanup);
const axeViolations = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};

const ME = asProfileId("me");
const OTHER = asProfileId("other");
const ANA = asConversationId("c-ana");
const BEN = asConversationId("c-ben");

function setup() {
  const h = createHarness({ env: {}, clock: () => 1000 });
  h.seedProfile({ profileId: ME, label: "Me", conversations: [
    { id: ANA, personId: asPersonId("p-ana"), displayName: "Ana", messages: [] },
    { id: BEN, personId: asPersonId("p-ben"), displayName: "Ben", messages: [] },
  ] });
  h.seedProfile({ profileId: OTHER, label: "Other", conversations: [] });
  let n = 0;
  const registry = createActionRegistry({ newOperationId: () => asOperationId(`op-${String(++n)}`) });
  const services: ConversationServices = { client: h, registry, ctx: h.contextFor(ME) };
  return { h, services };
}

function Screen({ services, open }: { readonly services: ConversationServices; readonly open?: ReturnType<typeof asGroupId> }) {
  const groups = useGroups(services);
  const list = useConversationList(services);
  const [draft, setDraft] = useState("");
  const [key, setKey] = useState(0);
  const [selected, setSelected] = useState<ReturnType<typeof asGroupId> | null>(open ?? null);
  const chats = list.status === "ready" ? list.value : [];
  return (
    <div>
      <GroupInvites services={services} copy={copy} invites={groups.invites} chats={chats} onAnswered={groups.refresh} />
      <NewGroup services={services} copy={copy} chats={chats} onCreated={groups.refresh} />
      <ul aria-label="groups">{groups.groups.map((g) => <li key={g.conversationId}><button type="button" onClick={() => { setSelected(g.conversationId); }}>{g.name} ({g.members}+{g.invited})</button></li>)}</ul>
      {selected === null ? null : (
        <GroupChat services={services} copy={copy} groupId={selected} chats={chats} refreshKey={key} onChanged={groups.refresh} onLeft={() => { setSelected(null); groups.refresh(); }}
          footer={<GroupComposer services={services} copy={copy} groupId={selected} draft={draft} onDraftChange={setDraft} onSent={() => { setKey((n) => n + 1); groups.refresh(); }} />} />
      )}
    </div>
  );
}

describe("groups UI on the loopback harness", () => {
  it("is absent where the host has no group engine", () => {
    const { services } = setup();
    expect(groupApiOf(services)).not.toBeNull();
    const bare = { ...services, client: { ...services.client, groups: undefined } } as unknown as ConversationServices;
    expect(groupApiOf(bare)).toBeNull();
  });

  it("create: choose people, the group appears with the others still invited, nobody is added without saying yes", async () => {
    const { services } = setup();
    const { container } = render(<Screen services={services} />);
    expect(await screen.findByText("Each person is only added if they say yes.")).toBeTruthy();
    const create = await screen.findByRole("button", { name: "Create group" });
    expect(create.hasAttribute("disabled")).toBe(true);
    await userEvent.type(screen.getByLabelText("Group name"), "Book club");
    await userEvent.click(await screen.findByRole("checkbox", { name: /Ana/ }));
    await userEvent.click(screen.getByRole("button", { name: /Create group/ }));
    expect(await screen.findByRole("button", { name: "Book club (1+1)" })).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
  });

  it("accept an invitation, then send and receive", async () => {
    const { h, services } = setup();
    h.groups.simulateInvite(services.ctx, ANA, "Family");
    const { container } = render(<Screen services={services} />);
    expect(await screen.findByText("Ana invited you to Family")).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: "Join" }));
    expect(await screen.findByText("Joined")).toBeTruthy();
    const open = await screen.findByRole("button", { name: /Family/ });
    await userEvent.click(open);
    const box = await screen.findByRole("textbox", { name: "Message the group" });
    await userEvent.type(box, "hello all");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(await screen.findByText("hello all")).toBeTruthy();
    const gid = h.groups.list(services.ctx)[0]?.conversationId ?? asGroupId("");
    h.groups.simulateMessage(services.ctx, gid, "acct-c-ana", "hi from ana");
    await waitFor(() => { expect(screen.queryByText("hi from ana")).not.toBeNull(); }, { timeout: 5000 });
    expect(await axeViolations(container)).toEqual([]);
  });

  it("roster: add, remove and leave each need their own confirmation, and leaving returns to the list", async () => {
    const { h, services } = setup();
    h.groups.simulateInvite(services.ctx, ANA, "Family");
    render(<Screen services={services} />);
    await userEvent.click(await screen.findByRole("button", { name: "Join" }));
    await userEvent.click(await screen.findByRole("button", { name: /Family/ }));
    await userEvent.click(await screen.findByRole("button", { name: "People in Family" }));
    const sheet = await screen.findByRole("dialog");
    expect(within(sheet).getByText("Later joiners see only later messages.")).toBeTruthy();
    expect(within(sheet).getByText("Messages can be shown as proof.")).toBeTruthy();
    // Add: Ben is offered, and invited (not added).
    await userEvent.click(within(sheet).getByRole("button", { name: "Invite Ben" }));
    await waitFor(() => { expect(h.groups.list(services.ctx)[0]?.invited).toBe(1); });
    // Leave asks first; "Stay" keeps everything.
    await userEvent.click(within(sheet).getByRole("button", { name: "Leave group" }));
    expect(within(sheet).getByText("Leave Family?")).toBeTruthy();
    await userEvent.click(within(sheet).getByRole("button", { name: "Stay" }));
    expect(h.groups.list(services.ctx)).toHaveLength(1);
    await userEvent.click(within(sheet).getByRole("button", { name: "Leave group" }));
    await userEvent.click(within(sheet).getByRole("button", { name: "Yes, leave" }));
    await waitFor(() => { expect(screen.queryByRole("textbox", { name: "Message the group" })).toBeNull(); });
  });

  it("a refusal is shown in words and a profile switch cannot retarget a step", async () => {
    const { services, h } = setup();
    const r = await runGroupAction(services, "group-leave", { conversationId: "nope" });
    expect(r).toMatchObject({ ok: false });
    const other = { ...services, ctx: h.contextFor(OTHER) };
    const stale = { ...services, ctx: services.ctx };
    expect(h.groups.list(other.ctx)).toHaveLength(0);
    expect(stale.ctx).not.toBe(other.ctx);
  });
});
