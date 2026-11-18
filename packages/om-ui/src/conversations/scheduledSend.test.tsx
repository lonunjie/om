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
import { Composer, ScheduledList, useScheduledSender, type ConversationServices, type ScheduleControl, type ScheduledItem } from "./index";

afterEach(cleanup);
const ME = asProfileId("me");
const CONV = asConversationId("conv-1");

function setup() {
  const h = createHarness({ env: {}, clock: () => 1000 });
  h.seedProfile({ profileId: ME, label: "Me", conversations: [{ id: CONV, personId: asPersonId("maria"), displayName: "Maria", messages: [] }] });
  return { h, services: { client: h, registry: createActionRegistry({}), ctx: h.contextFor(ME) } satisfies ConversationServices };
}
const item = (id: string, text: string, dueAt: number, conversationId = CONV): ScheduledItem => ({ id, conversationId, text, dueAt, createdAt: 0 });

function Releasing({ services, initial, now }: { readonly services: ConversationServices; readonly initial: readonly ScheduledItem[]; readonly now: () => number }) {
  const [items, setItems] = useState(initial);
  const sender = useScheduledSender({ services, items, onItems: setItems, now, intervalMs: 20 });
  return <p data-testid="left">{items.map((i) => i.id).join(",")}|{sender.notices.map((n) => `${n.kind}:${n.text}`).join(",")}</p>;
}

describe("scheduled send", () => {
  it("sends a due item as an ordinary message once, keeps one that is not due, and says nothing when it was on time", async () => {
    const { h, services } = setup();
    render(<Releasing services={services} now={() => 10_000} initial={[item("a", "on time", 9_900), item("b", "tomorrow", 90_000_000)]} />);
    await waitFor(async () => { expect((await h.query({ kind: "GetConversationView", ctx: services.ctx, conversationId: CONV })).timeline.map((i) => i.text)).toEqual(["on time"]); });
    await waitFor(() => { expect(screen.getByTestId("left").textContent).toBe("b|"); });
    await new Promise((r) => setTimeout(r, 80));
    expect((await h.query({ kind: "GetConversationView", ctx: services.ctx, conversationId: CONV })).timeline).toHaveLength(1);
  });

  it("an item that went out long after its time is marked sent late for its owner; one that can no longer be sent is dropped with a notice", async () => {
    const { h, services } = setup();
    render(<Releasing services={services} now={() => 10_000_000} initial={[item("late", "was due", 1_000), item("gone", "nowhere", 2_000, asConversationId("no-such-chat"))]} />);
    await waitFor(() => { expect(screen.getByTestId("left").textContent).toBe("|late:was due,dropped:nowhere"); });
    expect((await h.query({ kind: "GetConversationView", ctx: services.ctx, conversationId: CONV })).timeline.map((i) => i.text)).toEqual(["was due"]);
  });

  it("the composer schedules the draft at a chosen time, says what the device must be doing, and lists, edits and cancels it", async () => {
    const { services } = setup();
    let items: readonly ScheduledItem[] = [];
    const control = (set: (n: readonly ScheduledItem[]) => void): ScheduleControl => ({
      copy: testCopy.schedule, items, notices: [], now: () => 1_000_000, dismiss: () => undefined,
      add: (conversationId, text, dueAt) => { items = [...items, item("n1", text, dueAt, asConversationId(conversationId))]; set(items); },
      cancel: (id) => { items = items.filter((x) => x.id !== id); set(items); },
    });
    function Screen() {
      const [, setN] = useState(0);
      const [draft, setDraft] = useState("");
      return <Composer services={services} conversationId={CONV} copy={testCopy} peerName="Maria" draft={draft} onDraftChange={setDraft} schedule={control(() => { setN((n) => n + 1); })} />;
    }
    const { container } = render(<Screen />);
    const box = await screen.findByRole("textbox", { name: "Message" });
    expect(screen.getByRole("button", { name: "Schedule send" }).hasAttribute("disabled")).toBe(true);
    await userEvent.type(box, "happy birthday");
    await userEvent.click(screen.getByRole("button", { name: "Schedule send" }));
    const form = screen.getByRole("form", { name: "Schedule this message" });
    expect(form.textContent).toContain("only while OM is open on this device");
    const r = await axe.run(container, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
    expect(r.violations.map((v) => v.id)).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: "Schedule" }));
    expect(items).toHaveLength(1);
    expect(items[0]?.text).toBe("happy birthday");
    expect((box as HTMLTextAreaElement).value).toBe("");
    expect(screen.getByRole("region", { name: "Scheduled messages" })).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Edit scheduled message: happy birthday" }));
    expect((box as HTMLTextAreaElement).value).toBe("happy birthday");
    expect(items).toHaveLength(0);
  });

  it("lists nothing for another chat", () => {
    const control: ScheduleControl = { copy: testCopy.schedule, items: [item("x", "hi", 5)], notices: [], now: () => 0, add: () => undefined, cancel: () => undefined, dismiss: () => undefined };
    const { container } = render(<ScheduledList control={control} conversationId="other" onEdit={() => undefined} />);
    expect(container.textContent).toBe("");
  });
});
