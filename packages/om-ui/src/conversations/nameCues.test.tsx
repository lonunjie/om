// @vitest-environment jsdom
// The same-name and lookalike cues on the one-to-one surfaces (ADR-0197 extended, relay status D139): message requests, the conversation list and the header.
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { asConversationId, asOperationId, asPersonId, asProfileId, type ConversationSummary } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import { groupsTestCopy } from "../groups/__fixtures__/copy";
import { testCopy } from "./__fixtures__/copy";
import { ConversationList, ConversationView, MessageRequest, contactCollisions, requestCollision, type ConversationServices } from "./index";

afterEach(cleanup);
const axeViolations = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};
const names = groupsTestCopy.sameName;
const row = (id: string, displayName: string, nickname?: string): ConversationSummary => ({ id: asConversationId(id), personId: asPersonId(id), displayName, ...(nickname === undefined ? {} : { nickname }), lastMessageAt: null, notices: [], relationship: "active" } as unknown as ConversationSummary);
const services = {} as unknown as ConversationServices;
const CYR = "Mariа"; // Cyrillic a: looks like "Maria"

describe("requestCollision", () => {
  const contacts = [{ id: "c1", name: "Maria" }, { id: "c2", name: "Ben" }];
  it("is null for a different name, equal for the same name, alike for a lookalike", () => {
    expect(requestCollision("r", "Stranger", contacts)).toBeNull();
    expect(requestCollision("r", " maria ", contacts)).toMatchObject({ kind: "same", of: 2 });
    expect(requestCollision("r", CYR, contacts)).toMatchObject({ kind: "alike", of: 2 });
  });
  it("compares against the shown name, so a nickname ends the collision", () => {
    expect(requestCollision("r", "Maria", [{ id: "c1", name: "Mum" }])).toBeNull();
  });
});

describe("MessageRequest cue", () => {
  const req = (displayName: string) => ({ id: "req-1", from: asPersonId("s"), displayName, verification: "unverified" as const });
  it("shows a calm line, pattern and 'n of m' for a name equal to a contact's, and accept stays one tap", async () => {
    const { container } = render(<MessageRequest services={services} request={req("maria")} copy={testCopy} contacts={[{ id: "c1", name: "Maria" }]} sameNameCopy={names} onIgnore={() => undefined} />);
    expect(screen.getByText(/2 people here are named/)).toBeTruthy();
    expect(container.querySelector(".om-avatar--pattern")).not.toBeNull();
    expect(screen.getByText(/\(\d of 2\)/)).toBeTruthy();
    expect(screen.getByRole<HTMLButtonElement>("button", { name: testCopy.request.accept }).disabled).toBe(false);
    expect(await axeViolations(container)).toEqual([]);
  });
  it("uses the lookalike line for a lookalike name and never blocks ignoring", async () => {
    const ignored: string[] = [];
    const { container } = render(<MessageRequest services={services} request={req(CYR)} copy={testCopy} contacts={[{ id: "c1", name: "Maria" }]} sameNameCopy={names} onIgnore={(id) => ignored.push(id)} />);
    expect(screen.getByText(/look similar to/)).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: testCopy.request.ignore }));
    expect(ignored).toEqual(["req-1"]);
    expect(await axeViolations(container)).toEqual([]);
  });
  it("shows nothing extra for an unrelated name, without contacts, or without the copy", () => {
    const { container, rerender } = render(<MessageRequest services={services} request={req("Stranger")} copy={testCopy} contacts={[{ id: "c1", name: "Maria" }]} sameNameCopy={names} onIgnore={() => undefined} />);
    expect(container.querySelector(".om-avatar--pattern, [data-samename]")).toBeNull();
    rerender(<MessageRequest services={services} request={req("Maria")} copy={testCopy} onIgnore={() => undefined} />);
    expect(container.querySelector("[data-samename]")).toBeNull();
  });
});

describe("conversation list cue", () => {
  it("marks two contacts that show the same or lookalike names, leaves others and group rows alone", async () => {
    const rows = [row("a", "Maria"), row("b", CYR), row("c", "Ben"), row("g", "Maria")];
    const { container } = render(<ConversationList conversations={rows} selectedId={null} onSelect={vi.fn()} copy={testCopy} marks={new Map([[asConversationId("g"), "Group"]])} sameNameCopy={names} />);
    expect(screen.getAllByText(/\(\d of 2\)/)).toHaveLength(2);
    expect(screen.getByText(/have names that look similar to/)).toBeTruthy();
    expect(container.querySelectorAll(".om-avatar--pattern")).toHaveLength(2);
    expect(screen.getByText("Ben")).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
  });
  it("a nickname ends the collision, and without the copy nothing changes", () => {
    expect(contactCollisions([row("a", "Maria", "Mum"), row("b", "Maria")]).of("a")).toBeNull();
    const { container } = render(<ConversationList conversations={[row("a", "Maria"), row("b", "Maria")]} selectedId={null} onSelect={vi.fn()} copy={testCopy} />);
    expect(container.querySelector("[data-samename]")).toBeNull();
  });
});

describe("conversation header cue", () => {
  it("shows the place among same-named contacts and one line", async () => {
    const h = createHarness({ env: {}, clock: () => 1000 });
    const ME = asProfileId("me");
    h.seedProfile({ profileId: ME, label: "Me", conversations: [{ id: asConversationId("c-a"), personId: asPersonId("pa"), displayName: "Maria", messages: [] }] });
    let n = 0;
    const s: ConversationServices = { client: h, registry: createActionRegistry({ newOperationId: () => asOperationId(`op-${String(++n)}`) }), ctx: h.contextFor(ME) };
    const { container } = render(<ConversationView services={s} conversationId={asConversationId("c-a")} copy={testCopy} sameName={{ name: "Maria", index: 2, of: 2, kind: "alike" }} sameNameCopy={names} />);
    await screen.findByRole("heading", { name: "Maria (2 of 2)" });
    expect(screen.getByText(/look similar to/)).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
  });
});
