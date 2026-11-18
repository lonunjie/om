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
import { Composer, ConversationView, type ConversationServices } from "./index";

afterEach(cleanup);
const ME = asProfileId("me");
const CONV = asConversationId("conv-1");
const MARIA = "ab".repeat(32);

function Screen({ services, peerAccount = MARIA }: { readonly services: ConversationServices; readonly peerAccount?: string }) {
  const [draft, setDraft] = useState("");
  return <ConversationView services={services} conversationId={CONV} copy={testCopy} footer={<Composer services={services} conversationId={CONV} copy={testCopy} peerName="Maria" peerAccount={peerAccount} draft={draft} onDraftChange={setDraft} />} />;
}
function setup() {
  const h = createHarness({ env: {}, clock: () => 1000 });
  h.seedProfile({ profileId: ME, label: "Me", conversations: [{ id: CONV, personId: asPersonId("maria"), displayName: "Maria", messages: [{ text: "hello", direction: "incoming" }] }] });
  return { h, services: { client: h, registry: createActionRegistry({}), ctx: h.contextFor(ME) } satisfies ConversationServices };
}

describe("mentions in the composer", () => {
  it("offers the person after @, inserts the name, and sends the range with the person", async () => {
    const { h, services } = setup();
    const { container } = render(<Screen services={services} />);
    const box = await screen.findByRole("textbox", { name: "Message" });
    await userEvent.type(box, "hi @ma");
    expect(await screen.findByRole("listbox", { name: "People you can mention" })).toBeTruthy();
    expect(screen.getByRole("option", { name: "Maria" })).toBeTruthy();
    expect(box.getAttribute("aria-activedescendant")).toBeTruthy();
    const r = await axe.run(container, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
    expect(r.violations.map((v) => v.id)).toEqual([]);
    await userEvent.keyboard("{Enter}");
    expect((box as HTMLTextAreaElement).value).toBe("hi @Maria ");
    await userEvent.type(box, "lunch?");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => { expect((box as HTMLTextAreaElement).value).toBe(""); });
    const view = await h.query({ kind: "GetConversationView", ctx: services.ctx, conversationId: CONV });
    const sent = view.timeline.find((i) => i.text === "hi @Maria lunch?");
    expect(sent?.mentions).toEqual([{ start: 3, end: 9, accountId: MARIA, me: false }]);
    expect(document.querySelector(".om-richtext__mention")?.textContent).toBe("@Maria");
  });

  it("Escape closes the list and a mention whose words were edited away is sent as plain text", async () => {
    const { h, services } = setup();
    render(<Screen services={services} />);
    const box = await screen.findByRole("textbox", { name: "Message" });
    await userEvent.type(box, "@Mar");
    await userEvent.keyboard("{Enter}");
    await userEvent.clear(box);
    await userEvent.type(box, "never mind");
    await userEvent.type(box, "@x");
    expect(await screen.findByRole("option", { name: "Maria" }).catch(() => null)).toBeNull();
    await userEvent.type(box, "{Escape}");
    expect(screen.queryByRole("listbox")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(async () => { expect((await h.query({ kind: "GetConversationView", ctx: services.ctx, conversationId: CONV })).timeline.some((i) => i.text === "never mind@x")).toBe(true); });
    const view = await h.query({ kind: "GetConversationView", ctx: services.ctx, conversationId: CONV });
    expect(view.timeline.find((i) => i.text === "never mind@x")?.mentions).toBeUndefined();
  });

  it("offers no list when the person's account is not known", () => {
    const { services } = setup();
    render(<Composer services={services} conversationId={CONV} copy={testCopy} peerName="Maria" draft="@" onDraftChange={() => undefined} />);
    expect(screen.queryByRole("listbox")).toBeNull();
  });
});
