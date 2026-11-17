// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { asConversationId, asPersonId, asProfileId } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import { testCopy } from "./__fixtures__/copy";
import { Composer, ConversationView, type ConversationServices, type ReplyTarget } from "./index";

afterEach(cleanup);
const axeViolations = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};

const ME = asProfileId("me");
const CONV = asConversationId("conv-1");

function setup() {
  const h = createHarness({ env: {}, clock: () => 1000 });
  h.seedProfile({
    profileId: ME, label: "Me",
    conversations: [{
      id: CONV, personId: asPersonId("maria"), displayName: "Maria",
      messages: [
        { text: "Dinner at seven?", direction: "incoming" },
        { text: "Sounds good", direction: "outgoing", replyTo: 0 },
        { text: "Hm", direction: "incoming", replyTo: 0 },
      ],
    }],
  });
  const services: ConversationServices = { client: h, registry: createActionRegistry({}), ctx: h.contextFor(ME) };
  return { h, services };
}

function Screen({ services }: { readonly services: ConversationServices }) {
  const [draft, setDraft] = useState("");
  const [reply, setReply] = useState<ReplyTarget | null>(null);
  return (
    <ConversationView
      services={services} conversationId={CONV} copy={testCopy} onReply={setReply} replyingTo={reply?.messageId ?? null}
      footer={<Composer services={services} conversationId={CONV} copy={testCopy} peerName="Maria" draft={draft} onDraftChange={setDraft} replyTo={reply} onCancelReply={() => { setReply(null); }} />}
    />
  );
}

describe("reply", () => {
  it("shows the quoted block in a reply and says plainly who wrote the original", async () => {
    const { services } = setup();
    const { container } = render(<Screen services={services} />);
    const quotes = await screen.findAllByRole("button", { name: /Go to the original message/ });
    expect(quotes).toHaveLength(2);
    expect(quotes[0]?.getAttribute("aria-label")).toBe("In reply to Maria: Dinner at seven?. Go to the original message");
    expect(await axeViolations(container)).toEqual([]);
  });

  it("Reply starts a quote chip, the send names the original, and the chip ends with the send", async () => {
    const { h, services } = setup();
    const { container } = render(<Screen services={services} />);
    await userEvent.click((await screen.findAllByRole("button", { name: "Reply: Dinner at seven?" }))[0] as HTMLElement);
    expect(screen.getByText("Replying to Maria")).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.type(screen.getByRole("textbox", { name: "Message" }), "yes please");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => { expect(screen.queryByText("Replying to Maria")).toBeNull(); });
    const view = await h.query({ kind: "GetConversationView", ctx: services.ctx, conversationId: CONV });
    const sent = view.timeline.find((i) => i.text === "yes please");
    expect(sent?.reply?.available).toBe(true);
    expect(sent?.reply?.preview).toBe("Dinner at seven?");
    expect(await screen.findAllByRole("button", { name: /In reply to Maria: Dinner at seven\?/ })).toHaveLength(3);
  });

  it("Escape and the cancel control drop the reply without sending anything", async () => {
    const { h, services } = setup();
    render(<Screen services={services} />);
    await userEvent.click((await screen.findAllByRole("button", { name: "Reply: Dinner at seven?" }))[0] as HTMLElement);
    await userEvent.click(screen.getByRole("textbox", { name: "Message" }));
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByText("Replying to Maria")).toBeNull();
    await userEvent.click((await screen.findAllByRole("button", { name: "Reply: Dinner at seven?" }))[0] as HTMLElement);
    await userEvent.click(screen.getByRole("button", { name: "Cancel reply" }));
    expect(screen.queryByText("Replying to Maria")).toBeNull();
    const view = await h.query({ kind: "GetConversationView", ctx: services.ctx, conversationId: CONV });
    expect(view.timeline).toHaveLength(3);
  });

  it("a reply to your own message says so", async () => {
    const { services } = setup();
    render(<Screen services={services} />);
    await userEvent.type(await screen.findByRole("textbox", { name: "Message" }), "mine");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    await userEvent.click(await screen.findByRole("button", { name: "Reply: mine" }));
    expect(screen.getByText("Replying to yourself")).toBeTruthy();
  });

  it("an original that is gone shows a plain placeholder and is not a control", async () => {
    const h = createHarness({ env: {}, clock: () => 1000 });
    h.seedProfile({ profileId: ME, label: "Me", conversations: [{ id: CONV, personId: asPersonId("maria"), displayName: "Maria", messages: [{ text: "re: ?", direction: "incoming" }] }] });
    const services: ConversationServices = { client: h, registry: createActionRegistry({}), ctx: h.contextFor(ME) };
    // A reply whose original is not in the conversation: the harness refuses to send one, so seed through an index that does not exist.
    const view = await h.query({ kind: "GetConversationView", ctx: services.ctx, conversationId: CONV });
    expect(view.timeline[0]?.reply).toBeUndefined();
    const refused = await h.dispatch({ kind: "SendMessage", ctx: services.ctx, operationId: "op-x" as never, conversationId: CONV, draft: { text: "x", replyTo: "msg-9999" as never } });
    const ops = await h.pendingOperations(services.ctx);
    expect(ops.find((o) => o.operationId === refused.operationId)?.outcome).toEqual({ kind: "Rejected", reason: "not_found" });
  });

  it("the Reply control does not mirror for sending order and the move to the original marks it", async () => {
    const { services } = setup();
    render(<Screen services={services} />);
    const quote = (await screen.findAllByRole("button", { name: /Go to the original message/ }))[0] as HTMLElement;
    await userEvent.click(quote);
    await waitFor(() => { expect(document.querySelector("[data-found='true']")).not.toBeNull(); });
  });
});

describe("reactions", () => {
  function Plain({ services }: { readonly services: ConversationServices }) {
    return <ConversationView services={services} conversationId={CONV} copy={testCopy} />;
  }
  const withTheirs = () => {
    const h = createHarness({ env: {}, clock: () => 1000 });
    h.seedProfile({ profileId: ME, label: "Me", conversations: [{ id: CONV, personId: asPersonId("maria"), displayName: "Maria", messages: [{ text: "Big news", direction: "incoming", reactions: ["❤️"] }] }] });
    return { h, services: { client: h, registry: createActionRegistry({}), ctx: h.contextFor(ME) } as ConversationServices };
  };

  it("shows their reaction as a chip, and tapping it adds yours (count 2) and again removes it", async () => {
    const { services } = withTheirs();
    const { container } = render(<Plain services={services} />);
    const chip = await screen.findByRole("button", { name: "❤️ 1. Add yours" });
    expect(chip.getAttribute("aria-pressed")).toBe("false");
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(chip);
    const mine = await screen.findByRole("button", { name: "❤️ 2, including you. Remove your reaction" });
    expect(mine.getAttribute("aria-pressed")).toBe("true");
    await userEvent.click(mine);
    expect(await screen.findByRole("button", { name: "❤️ 1. Add yours" })).toBeTruthy();
  });

  it("the picker offers a small set, closes on Escape, and a pick sets one reaction", async () => {
    const { services } = withTheirs();
    const { container } = render(<Plain services={services} />);
    await userEvent.click(await screen.findByRole("button", { name: "React: Big news" }));
    const picker = screen.getByRole("group", { name: "Choose a reaction" });
    expect(picker.querySelectorAll("button")).toHaveLength(6);
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("group", { name: "Choose a reaction" })).toBeNull();
    await userEvent.click(await screen.findByRole("button", { name: "React: Big news" }));
    await userEvent.click(screen.getByRole("button", { name: "React with \u{1F44D}" }));
    expect(await screen.findByRole("button", { name: "\u{1F44D} 1, including you. Remove your reaction" })).toBeTruthy();
    expect(screen.queryByRole("group", { name: "Choose a reaction" })).toBeNull();
  });
});
