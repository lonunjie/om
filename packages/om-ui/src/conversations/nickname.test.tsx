// @vitest-environment jsdom
// A person's own nickname for a contact (ADR-0196): set and cleared in the contact sheet, shown instead of the name the contact chose
// (which stays reachable), and used by the same-name cues.
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it } from "vitest";
import { asConversationId, asOperationId, asPersonId, asProfileId } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import { testCopy } from "./__fixtures__/copy";
import { ConversationList, ConversationView, shownName, useConversationList, type ConversationServices } from "./index";

afterEach(cleanup);
const axeViolations = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};

const ME = asProfileId("me");
const A = asConversationId("c-a");
const B = asConversationId("c-b");

function setup() {
  const h = createHarness({ env: {}, clock: () => 1000 });
  h.seedProfile({ profileId: ME, label: "Me", conversations: [
    { id: A, personId: asPersonId("pa"), displayName: "Alex Doe", messages: [{ text: "hello", direction: "incoming" }] },
    { id: B, personId: asPersonId("pb"), displayName: "Alex Doe", messages: [] },
  ] });
  let n = 0;
  const registry = createActionRegistry({ newOperationId: () => asOperationId(`op-${String(++n)}`) });
  const services: ConversationServices = { client: h, registry, ctx: h.contextFor(ME) };
  return { h, services };
}

function Screen({ services, open }: { readonly services: ConversationServices; readonly open: typeof A }) {
  const list = useConversationList(services);
  return (
    <div>
      {list.status === "ready" ? <ConversationList conversations={list.value} selectedId={null} onSelect={() => undefined} copy={testCopy} /> : null}
      <ConversationView services={services} conversationId={open} copy={testCopy} />
    </div>
  );
}

describe("shownName", () => {
  it("is the nickname when there is one, else the name the contact chose", () => {
    expect(shownName({ displayName: "Alex", nickname: "Al" })).toBe("Al");
    expect(shownName({ displayName: "Alex" })).toBe("Alex");
    expect(shownName({ displayName: "Alex", nickname: "" })).toBe("Alex");
  });
});

describe("contact sheet", () => {
  it("sets a nickname that wins in the list and header, keeps the original reachable, and clears it again", async () => {
    const { services } = setup();
    const { container } = render(<Screen services={services} open={A} />);
    await screen.findByRole("heading", { name: "Alex Doe" });
    await userEvent.click(screen.getByRole("button", { name: "About Alex Doe" }));
    const sheet = await screen.findByRole("dialog", { name: "About Alex Doe" });
    expect(within(sheet).getByText(/Their name:/)).toBeTruthy();
    expect(await axeViolations(sheet)).toEqual([]);
    expect(within(sheet).getByRole("button", { name: "Save" }).hasAttribute("disabled")).toBe(true);
    expect(within(sheet).queryByRole("button", { name: "Remove nickname" })).toBeNull();

    await userEvent.type(within(sheet).getByRole("textbox", { name: "Your name for them" }), "Alex from work");
    await userEvent.click(within(sheet).getByRole("button", { name: "Save" }));
    expect(await within(sheet).findByText("Saved.")).toBeTruthy();
    // The header, the list and the sheet title show the nickname; the original is secondary text.
    expect(await screen.findByRole("heading", { name: "Alex from work" })).toBeTruthy();
    expect(screen.getAllByRole("option").some((o) => o.textContent.includes("Alex from work"))).toBe(true);
    expect(container.querySelector(".om-conv-view__original")?.textContent).toBe("Alex Doe");
    expect(within(sheet).getByText(/Their name:/).textContent).toContain("Alex Doe");
    expect(await axeViolations(container)).toEqual([]);

    await userEvent.click(within(sheet).getByRole("button", { name: "Remove nickname" }));
    expect(await within(sheet).findByText("Removed.")).toBeTruthy();
    expect(await screen.findByRole("heading", { name: "Alex Doe" })).toBeTruthy();
    expect(container.querySelector(".om-conv-view__original")).toBeNull();
  });

  it("refuses a nickname with hidden direction characters in plain words and changes nothing", async () => {
    const { services } = setup();
    render(<Screen services={services} open={A} />);
    await userEvent.click(await screen.findByRole("button", { name: "About Alex Doe" }));
    const sheet = await screen.findByRole("dialog");
    await userEvent.type(within(sheet).getByRole("textbox", { name: "Your name for them" }), "a‮b");
    await userEvent.click(within(sheet).getByRole("button", { name: "Save" }));
    expect((await within(sheet).findByRole("alert")).textContent).toBe("That nickname could not be used.");
    expect(screen.getByRole("heading", { name: "Alex Doe" })).toBeTruthy();
  });

  it("the nickname is local: the harness intent carries it nowhere else (no other profile sees it)", async () => {
    const { h, services } = setup();
    render(<Screen services={services} open={A} />);
    await userEvent.click(await screen.findByRole("button", { name: "About Alex Doe" }));
    await userEvent.type(await screen.findByRole("textbox", { name: "Your name for them" }), "Private");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByText("Saved.");
    const list = await h.query({ kind: "GetConversationList", ctx: services.ctx });
    expect(list.find((c) => c.id === A)?.nickname).toBe("Private");
    expect(list.find((c) => c.id === B)?.nickname).toBeUndefined();
  });
});

it("changes Allow calls for only the contact in the sheet", async () => {
  const { h, services } = setup();
  render(<Screen services={services} open={A} />);
  await userEvent.click(await screen.findByRole("button", { name: "About Alex Doe" }));
  const sheet = await screen.findByRole("dialog");
  const control = within(sheet).getByRole<HTMLInputElement>("switch", { name: "Allow calls" });
  expect(control.checked).toBe(true);
  await userEvent.click(control);
  await within(sheet).findByRole("switch", { name: "Allow calls", checked: false });
  const list = await h.query({ kind: "GetConversationList", ctx: services.ctx });
  expect(list.find((c) => c.id === A)?.allowCalls).toBe(false);
  expect(list.find((c) => c.id === B)?.allowCalls).toBe(true);
  expect(await axeViolations(sheet)).toEqual([]);
});
