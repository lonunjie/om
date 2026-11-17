// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { asConversationId, asGroupId, asOperationId, asPersonId, asProfileId } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import type { ConversationServices, ReplyTarget } from "../conversations/index";
import { GroupChat, GroupComposer, useGroupEvents, useGroups } from "./index";
import { groupsTestCopy as copy } from "./__fixtures__/copy";

afterEach(cleanup);
const axeViolations = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};

const ME = asProfileId("me");
const ANA = asConversationId("c-ana");

function setup() {
  const h = createHarness({ env: {}, clock: () => 1000 });
  h.seedProfile({ profileId: ME, label: "Me", conversations: [{ id: ANA, personId: asPersonId("p-ana"), displayName: "Ana", messages: [] }] });
  let n = 0;
  const registry = createActionRegistry({ newOperationId: () => asOperationId(`op-${String(++n)}`) });
  const services: ConversationServices = { client: h, registry, ctx: h.contextFor(ME) };
  h.groups.simulateInvite(services.ctx, ANA, "Family");
  return { h, services };
}

function Room({ services, gid }: { readonly services: ConversationServices; readonly gid: ReturnType<typeof asGroupId> }) {
  const groups = useGroups(services);
  const events = useGroupEvents(services, () => { groups.refresh(); });
  const [draft, setDraft] = useState("");
  const [key, setKey] = useState(0);
  const [reply, setReply] = useState<ReplyTarget | null>(null);
  return (
    <GroupChat
      services={services} copy={copy} groupId={gid} chats={[]} refreshKey={key} events={events} onChanged={() => { setKey((k) => k + 1); }} onLeft={() => undefined}
      onReply={setReply} replyingTo={reply?.messageId ?? null}
      footer={<GroupComposer services={services} copy={copy} groupId={gid} draft={draft} onDraftChange={setDraft} onSent={() => { setKey((k) => k + 1); }} replyTo={reply} onCancelReply={() => { setReply(null); }} />}
    />
  );
}

async function joined() {
  const { h, services } = setup();
  const invites = h.groups.invites(services.ctx);
  const inv = invites[0];
  if (inv === undefined) throw new Error("no invitation");
  await h.groups.do(services.ctx, { kind: "accept", inviteId: inv.id });
  const gid = h.groups.list(services.ctx)[0]?.conversationId ?? asGroupId("");
  h.groups.simulateMessage(services.ctx, gid, "acct-c-ana", "Dinner at seven?");
  return { h, services, gid };
}

describe("everyday chat in a group (D143)", () => {
  it("reply: the chip names who is answered, the send names the original, and the quote moves to it", async () => {
    const { services, gid } = await joined();
    const { container } = render(<Room services={services} gid={gid} />);
    await userEvent.click(await screen.findByRole("button", { name: "Reply: Dinner at seven?" }));
    const chip = await screen.findByRole("group", { name: "Replying to Ana" });
    expect(within(chip).getByText("Dinner at seven?")).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.type(screen.getByRole("textbox", { name: "Message the group" }), "yes please");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    const quote = await screen.findByRole("button", { name: "In reply to Ana: Dinner at seven?. Go to the original message" });
    expect(quote).toBeTruthy();
    expect(screen.queryByRole("group", { name: "Replying to Ana" })).toBeNull();
    expect(await axeViolations(container)).toEqual([]);
  });

  it("Escape ends a reply without sending", async () => {
    const { services, gid } = await joined();
    render(<Room services={services} gid={gid} />);
    await userEvent.click(await screen.findByRole("button", { name: "Reply: Dinner at seven?" }));
    await screen.findByRole("group", { name: "Replying to Ana" });
    await userEvent.type(screen.getByRole("textbox", { name: "Message the group" }), "{Escape}");
    expect(screen.queryByRole("group", { name: "Replying to Ana" })).toBeNull();
  });

  it("reactions: pick one, see it counted as yours, take it back; names say count and ownership", async () => {
    const { services, gid } = await joined();
    const { container } = render(<Room services={services} gid={gid} />);
    await userEvent.click(await screen.findByRole("button", { name: "React: Dinner at seven?" }));
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(await screen.findByRole("button", { name: "React with \u{1F44D}" }));
    const chip = await screen.findByRole("button", { name: "\u{1F44D} 1, including you. Remove your reaction" });
    expect(chip.getAttribute("aria-pressed")).toBe("true");
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(chip);
    await waitFor(() => { expect(screen.queryByRole("button", { name: /Remove your reaction/ })).toBeNull(); });
  });

  it("edit and delete are offered on your own messages only, delete says its scope first, and an edit is marked", async () => {
    const { services, gid } = await joined();
    const { container } = render(<Room services={services} gid={gid} />);
    await userEvent.type(await screen.findByRole("textbox", { name: "Message the group" }), "see you there");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    await screen.findByText("see you there");
    expect(screen.queryByRole("button", { name: "Edit: Dinner at seven?" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Delete: Dinner at seven?" })).toBeNull();
    await userEvent.click(await screen.findByRole("button", { name: "Edit: see you there" }));
    const box = await screen.findByRole("textbox", { name: "Edit message" });
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.clear(box);
    await userEvent.type(box, "see you at eight");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByText("see you at eight");
    expect(screen.getByText("Edited")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Delete: see you at eight" }));
    const ask = await screen.findByRole("group", { name: "Delete for everyone?" });
    expect(within(ask).getByText(/everyone in this chat/)).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(within(ask).getByRole("button", { name: "Delete for everyone" }));
    await waitFor(() => { expect(screen.queryByText("see you at eight")).toBeNull(); });
    expect(screen.getByText("Dinner at seven?")).toBeTruthy();
  });

  it("a reply whose original was deleted says so plainly and moves nowhere", async () => {
    const { h, services, gid } = await joined();
    await h.groups.do(services.ctx, { kind: "send_text", conversationId: gid, text: "first" });
    const first = h.groups.messages(services.ctx, gid).find((m) => m.text === "first");
    if (first === undefined) throw new Error("no message");
    await h.groups.do(services.ctx, { kind: "send_reply", conversationId: gid, text: "second", replyTo: first.id });
    await h.groups.do(services.ctx, { kind: "delete_message", conversationId: gid, messageId: first.id });
    const { container } = render(<Room services={services} gid={gid} />);
    expect(await screen.findByText("Original message unavailable")).toBeTruthy();
    expect(screen.queryByText("first")).toBeNull();
    expect(await axeViolations(container)).toEqual([]);
  });
});
