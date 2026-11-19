// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { asConversationId, asPersonId, asProfileId, type CoreIntent } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import { testCopy } from "./__fixtures__/copy";
import { Composer, ScheduledList, scheduledOperationId, useScheduledSender, type ConversationServices, type ScheduleControl, type ScheduledItem } from "./index";

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
    expect(screen.getByRole("button", { name: "Schedule for later" }).hasAttribute("disabled")).toBe(true);
    await userEvent.type(box, "happy birthday");
    await userEvent.click(screen.getByRole("button", { name: "Schedule for later" }));
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

/** A crash is modelled as: unmount, then mount again with whatever the sealed queue held at that moment (`stored`). */
function Crashy({ services, stored, persist, now }: { readonly services: ConversationServices; readonly stored: readonly ScheduledItem[]; readonly persist: (next: readonly ScheduledItem[]) => void; readonly now: () => number }) {
  const [items, setItems] = useState(stored);
  useScheduledSender({ services, items, onItems: (next) => { persist(next); setItems(next); }, now, intervalMs: 20 });
  return <p data-testid="queue">{items.map((i) => i.id).join(",")}</p>;
}
const sentTexts = async (h: ReturnType<typeof setup>["h"], services: ConversationServices) => (await h.query({ kind: "GetConversationView", ctx: services.ctx, conversationId: CONV })).timeline.map((i) => i.text);

describe("scheduled send: crash cuts", () => {
  it("crash before the dispatch: the restart sends it once", async () => {
    const { h, services } = setup();
    const stored = [item("s1", "once", 100)];
    render(<Crashy services={services} stored={stored} persist={() => undefined} now={() => 10_000} />);
    await waitFor(async () => { expect(await sentTexts(h, services)).toEqual(["once"]); });
    await new Promise((r) => setTimeout(r, 80));
    expect(await sentTexts(h, services)).toEqual(["once"]);
  });

  it("crash after the dispatch and before the item left the queue: the restart repeats the same operation and nothing is sent twice", async () => {
    const { h, services } = setup();
    const stored = [item("s1", "once", 100)]; // the removal never reached the sealed store
    const first = render(<Crashy services={services} stored={stored} persist={() => undefined} now={() => 10_000} />);
    await waitFor(async () => { expect(await sentTexts(h, services)).toEqual(["once"]); });
    first.unmount(); // the crash
    h.simulateRestart();
    let persisted: readonly ScheduledItem[] = stored;
    render(<Crashy services={{ ...services, ctx: h.contextFor(ME) }} stored={stored} persist={(n) => { persisted = n; }} now={() => 20_000} />);
    await waitFor(() => { expect(persisted).toEqual([]); }); // the item was finally released from the queue ...
    expect(await sentTexts(h, services)).toEqual(["once"]); // ... without a second message
  });

  it("crash after the item left the queue: nothing is sent again", async () => {
    const { h, services } = setup();
    let persisted: readonly ScheduledItem[] = [item("s1", "once", 100)];
    const first = render(<Crashy services={services} stored={persisted} persist={(n) => { persisted = n; }} now={() => 10_000} />);
    await waitFor(() => { expect(persisted).toEqual([]); });
    first.unmount();
    h.simulateRestart();
    render(<Crashy services={{ ...services, ctx: h.contextFor(ME) }} stored={persisted} persist={() => undefined} now={() => 20_000} />);
    await new Promise((r) => setTimeout(r, 80));
    expect(await sentTexts(h, services)).toEqual(["once"]);
  });

  it("the operation id is stable for an item and attempt, and a send that needs the person is retried as a new operation", async () => {
    expect(scheduledOperationId({ id: "s1" })).toBe(scheduledOperationId({ id: "s1", attempt: 0 }));
    expect(scheduledOperationId({ id: "s1", attempt: 1 })).not.toBe(scheduledOperationId({ id: "s1" }));
    expect(scheduledOperationId({ id: "s1" })).not.toBe(scheduledOperationId({ id: "s2" }));
    const { h, services } = setup();
    const ops: string[] = [];
    const client = {
      ...h,
      dispatch: (intent: CoreIntent) => { ops.push(intent.operationId); return h.dispatch(intent); },
      observe: (_ctx: unknown, _op: unknown, listener: (o: { kind: "NeedsUserAction"; action: "reauthenticate" }) => void) => { listener({ kind: "NeedsUserAction", action: "reauthenticate" }); return () => undefined; },
    } as unknown as ConversationServices["client"];
    let persisted: readonly ScheduledItem[] = [];
    render(<Crashy services={{ ...services, client }} stored={[item("s1", "later", 100)]} persist={(n) => { persisted = n; }} now={() => 10_000} />);
    await waitFor(() => { expect(persisted[0]?.attempt).toBeGreaterThanOrEqual(1); });
    await waitFor(() => { expect(new Set(ops).size).toBeGreaterThanOrEqual(2); });
    expect(ops[0]).toBe("sched:s1:0");
    expect(ops[1]).toBe("sched:s1:1");
  });
});

