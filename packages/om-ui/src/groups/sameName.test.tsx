// @vitest-environment jsdom
import { cleanup, render, screen, within } from "@testing-library/react";
import axe from "axe-core";
import { afterEach, describe, expect, it } from "vitest";
import { asConversationId, asGroupId, asPersonId, type ConversationSummary, type GroupDetailView, type GroupInviteView } from "@om/platform-api";
import type { ConversationServices } from "../conversations/index";
import { GroupInvites, NewGroup } from "./index";
import { GroupMembers } from "./GroupMembers";
import { PatternAvatar, patternCells } from "./PatternAvatar";
import { findSameNames, nameKey } from "./sameName";
import { usePeopleNames } from "./hooks";
import { groupsTestCopy as copy } from "./__fixtures__/copy";

afterEach(cleanup);
const axeViolations = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};
const services = {} as unknown as ConversationServices;
const chat = (id: string, displayName: string): ConversationSummary => ({ id: asConversationId(id), personId: asPersonId(id), displayName, lastMessageAt: null, notices: [], relationship: "active" } as unknown as ConversationSummary);

describe("same-name rule (GROUPS_AND_SPACES_UX section 10)", () => {
  it("is exact equality after NFKC, case folding and trimming, and nothing looser", () => {
    expect(nameKey("  Maria ")).toBe(nameKey("MARIA"));
    expect(nameKey("Ｍaria")).toBe(nameKey("Maria")); // full-width M folds under NFKC
    expect(nameKey("Straße")).toBe(nameKey("STRASSE"));
    expect(nameKey("Maria")).not.toBe(nameKey("Marіa")); // Cyrillic i: lookalike matching is not done here
    const c = findSameNames([{ id: "b", name: "maria" }, { id: "a", name: "Maria" }, { id: "c", name: "Ana" }, { id: "d", name: "  " }, { id: "e", name: "" }]);
    expect(c.of("a")).toMatchObject({ index: 1, of: 2 });
    expect(c.of("b")).toMatchObject({ index: 2, of: 2 }); // order is by id, not by list position
    expect(c.of("c")).toBeNull();
    expect(c.of("d")).toBeNull();
    expect(c.groups).toEqual([{ name: "Maria", count: 2 }]);
  });

  it("one person under two ids of the same list entry is not a collision with themselves", () => {
    expect(findSameNames([{ id: "a", name: "Ana" }, { id: "a", name: "Ana" }]).of("a")).toBeNull();
  });
});

describe("pattern avatar", () => {
  it("is stable for a seed, differs between seeds, and is hidden from assistive technology", () => {
    expect(patternCells("acct-1")).toEqual(patternCells("acct-1"));
    expect(patternCells("acct-1")).not.toEqual(patternCells("acct-2"));
    const { container } = render(<PatternAvatar seed="acct-1" />);
    const svg = container.querySelector("svg");
    expect(svg?.getAttribute("aria-hidden")).toBe("true");
    expect(svg?.getAttribute("focusable")).toBe("false");
    expect(svg?.textContent).toBe("");
  });
});

const detail = (people: GroupDetailView["people"]): GroupDetailView => ({
  summary: { conversationId: asGroupId("g1"), name: "Book club", members: people.length, invited: 0, epoch: 1 },
  people, former: [], canSend: true,
  protection: { profile: 1, custom: false, joinRule: 0, newDevicePolicy: 0, historyPolicy: 0, membersMayInvite: false, disappearingTimer: null },
});
const person = (account: string, display: string | null, you = false): GroupDetailView["people"][number] => ({ account, roles: ["member"], status: "member", display, you });

function Members({ d, chats = [], invites = [] }: { d: GroupDetailView; chats?: ConversationSummary[]; invites?: GroupInviteView[] }) {
  const names = usePeopleNames(d);
  return <GroupMembers services={services} copy={copy} detail={d} names={names} invites={invites} chats={chats} onChanged={() => undefined} onLeft={() => undefined} />;
}

describe("people sheet with two people named alike", () => {
  it("shows the pattern, '1 of 2', one calm line and an unambiguous remove name; no modal", async () => {
    const d = detail([person("acct-me", null, true), person("acct-b", "Maria"), person("acct-a", "maria"), person("acct-c", "Ana")]);
    const { container } = render(<Members d={d} />);
    expect(screen.getByText("2 people here are named maria. A name does not show who someone is.")).toBeTruthy();
    const list = screen.getByRole("list", { name: "People in this group" });
    expect(within(list).getByText("maria (1 of 2)")).toBeTruthy();
    expect(within(list).getByText("Maria (2 of 2)")).toBeTruthy();
    expect(within(list).getByText("Ana")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Remove maria (1 of 2)" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Remove Maria (2 of 2)" })).toBeTruthy();
    expect(container.querySelectorAll("svg.om-avatar--pattern")).toHaveLength(2);
    expect(container.querySelectorAll("[aria-hidden='true'].om-avatar:not(svg)")).toHaveLength(2); // Ana and You keep the initial
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(await axeViolations(container)).toEqual([]);
  });

  it("says nothing when names differ", () => {
    const { container } = render(<Members d={detail([person("acct-me", null, true), person("acct-a", "Ana"), person("acct-b", "Ben")])} />);
    expect(container.querySelector(".om-group-samename")).toBeNull();
    expect(container.querySelector("svg")).toBeNull();
  });

  it("counts a member who is also a contact once (same account)", () => {
    const d = detail([person("acct-me", null, true), person("acct-a", "Alice")]);
    const same = { ...chat("rel-a", "Alice"), peerAccount: "acct-a" } as ConversationSummary;
    const { container } = render(<Members d={d} chats={[same]} />);
    expect(container.querySelector(".om-group-samename")).toBeNull();
    expect(container.textContent).not.toMatch(/\d of \d/);
  });

  it("still counts a different account that shares the name of a member who is also a contact", () => {
    const d = detail([person("acct-me", null, true), person("acct-a", "Alice")]);
    const chats = [{ ...chat("rel-a", "Alice"), peerAccount: "acct-a" }, { ...chat("rel-b", "Alice"), peerAccount: "acct-b" }] as ConversationSummary[];
    const { container } = render(<Members d={d} chats={chats} />);
    expect(container.textContent).toMatch(/Alice \(\d of 2\)/);
    expect(container.textContent).not.toMatch(/of 3/);
  });

  it("flags a contact you could add who shares a name with someone already here", async () => {
    const d = detail([person("acct-me", null, true), person("acct-a", "Maria")]);
    const { container } = render(<Members d={d} chats={[chat("rel-1", "Maria")]} />);
    expect(screen.getByRole("button", { name: /^Invite Maria \(\d of 2\)$/ })).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe("contact pick list and invitations", () => {
  it("new group: one line and the pattern for contacts who share a name", async () => {
    const { container } = render(<NewGroup services={services} copy={copy} chats={[chat("r1", "Maria"), chat("r2", "MARIA"), chat("r3", "Ana")]} onCreated={() => undefined} />);
    expect(screen.getByText("2 of your contacts are named Maria.")).toBeTruthy();
    expect(screen.getByRole("checkbox", { name: /Maria \(1 of 2\)/ })).toBeTruthy();
    expect(screen.getByRole("checkbox", { name: /MARIA \(2 of 2\)/ })).toBeTruthy();
    expect(container.querySelectorAll("svg.om-avatar--pattern")).toHaveLength(2);
    expect(await axeViolations(container)).toEqual([]);
  });

  it("an invitation from someone whose name a second contact shares says so before you answer", async () => {
    const inv: GroupInviteView = { id: "i1", conversationId: asGroupId("g9"), groupName: "Family", dir: "incoming", state: "offered", relationshipId: asConversationId("r2"), peerAccount: "acct-2", createdMs: 0 };
    const { container } = render(<GroupInvites services={services} copy={copy} invites={[inv]} chats={[chat("r1", "Maria"), chat("r2", "Maria")]} onAnswered={() => undefined} />);
    expect(screen.getByText("2 of your contacts are named Maria.")).toBeTruthy();
    expect(screen.getByText(/Maria \(2 of 2\)/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Join" })).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
  });
});
