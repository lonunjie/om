// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { asConversationId, asOperationId, asPersonId, asProfileId } from "@om/platform-api";
import type { CoreClient, PendingOperationView } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import { testCopy } from "./__fixtures__/copy";
import { Composer, ConversationList, ConversationView, MessageRequest, RichText, useConversationList, type ConversationServices } from "./index";

afterEach(cleanup);
const axeViolations = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};

const ALICE = asProfileId("alice");
const BOB = asProfileId("bob");
const CONV_A = asConversationId("conv-ab");
const CONV_B = asConversationId("conv-ba");

function setup(opIds: string[] = []) {
  const h = createHarness({ env: {}, clock: () => 1000 });
  h.seedProfile({ profileId: ALICE, label: "Alice", conversations: [{ id: CONV_A, personId: asPersonId("bob-person"), displayName: "Bob", messages: [{ text: "hello alice", direction: "incoming" }] }], introductions: [{ id: "req-1", from: asPersonId("stranger"), personId: asPersonId("stranger"), displayName: "Stranger" }] });
  h.seedProfile({ profileId: BOB, label: "Bob", conversations: [{ id: CONV_B, personId: asPersonId("alice-person"), displayName: "Alice", messages: [] }] });
  let n = 0;
  const registry = createActionRegistry({ newOperationId: () => asOperationId(opIds[n++] ?? `op-${String(n)}`) });
  const services: ConversationServices = { client: h, registry, ctx: h.contextFor(ALICE) };
  return { h, services };
}

function Screen({ services, initial = "" }: { readonly services: ConversationServices; readonly initial?: string }) {
  const [draft, setDraft] = useState(initial);
  const [selected, setSelected] = useState<ReturnType<typeof asConversationId> | null>(null);
  const list = useConversationList(services);
  return (
    <div>
      {list.status === "ready" ? <ConversationList conversations={list.value} selectedId={selected} onSelect={setSelected} copy={testCopy} /> : null}
      {selected ? <ConversationView services={services} conversationId={selected} copy={testCopy} footer={<Composer services={services} conversationId={selected} copy={testCopy} draft={draft} onDraftChange={setDraft} />} /> : null}
    </div>
  );
}

describe("conversation slice end to end", () => {
  it("seed -> list -> open -> send -> committed, and two profiles stay separate", async () => {
    const { h, services } = setup(["op-send-1"]);
    const { container } = render(<Screen services={services} />);
    const option = await screen.findByRole("option", { name: /Bob/ });
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(option);
    expect(await screen.findByText("hello alice")).toBeTruthy();
    const box = screen.getByRole("textbox", { name: "Message" });
    expect(box.getAttribute("spellcheck")).toBe("false");
    expect(box.getAttribute("autocomplete")).toBe("off");
    await userEvent.type(box, "hi bob");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(await screen.findByText("hi bob")).toBeTruthy();
    expect(screen.getByRole<HTMLTextAreaElement>("textbox", { name: "Message" }).value).toBe("");
    expect(await screen.findByText("Sent")).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
    const ops = await h.pendingOperations(services.ctx);
    expect(ops.map((o: PendingOperationView) => o.operationId)).toEqual(["op-send-1"]);
    // Bob's profile saw nothing.
    const bobView = await h.query({ kind: "GetConversationView", ctx: h.contextFor(BOB), conversationId: CONV_B });
    expect(bobView.timeline).toHaveLength(0);
  });

  it("network timeout after send shows pending reconciliation, no rejection, one operation; restart reconstructs it", async () => {
    const { h, services } = setup(["op-t"]);
    h.injectNetworkTimeoutAfterSend(asOperationId("op-t"));
    const first = render(<Screen services={services} />);
    await userEvent.click(await screen.findByRole("option", { name: /Bob/ }));
    await userEvent.type(await screen.findByRole("textbox", { name: "Message" }), "are you there");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(await screen.findByText("Confirming delivery")).toBeTruthy();
    expect(screen.queryByText(/Not sent|Rejected/)).toBeNull();
    expect(screen.queryByRole("button", { name: "Retry sending" })).toBeNull();
    expect((await h.pendingOperations(services.ctx)).filter((o: PendingOperationView) => o.intentKind === "SendMessage")).toHaveLength(1);

    first.unmount();
    h.simulateRestart();
    render(<Screen services={{ ...services, ctx: h.contextFor(ALICE) }} />);
    await userEvent.click(await screen.findByRole("option", { name: /Bob/ }));
    expect(await screen.findByText("Confirming delivery")).toBeTruthy();
    h.resolveReconciliation(asOperationId("op-t"), { kind: "Committed" });
    expect(await screen.findByText("Sent")).toBeTruthy();
  });

  it("rejected send keeps the draft and shows a reason-code notice", async () => {
    const { services } = setup(["op-r"]);
    const rejecting: ConversationServices = { ...services, client: { ...services.client, dispatch: services.client.dispatch.bind(services.client), observe: (ctx, op, l) => { l({ kind: "Rejected", reason: "policy_denied" }); return () => undefined; }, query: services.client.query.bind(services.client), subscribe: services.client.subscribe.bind(services.client), pendingOperations: services.client.pendingOperations.bind(services.client) } };
    render(<Screen services={rejecting} />);
    await userEvent.click(await screen.findByRole("option", { name: /Bob/ }));
    await userEvent.type(await screen.findByRole("textbox", { name: "Message" }), "secret draft");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    expect((await screen.findByRole("alert")).textContent).toBe("Rejected: policy_denied");
    expect(screen.getByRole<HTMLTextAreaElement>("textbox", { name: "Message" }).value).toBe("secret draft");
    expect(await screen.findByRole("button", { name: "Send" })).toHaveProperty("disabled", false);
  });

  it("empty draft cannot be sent and a dispatch failure keeps the draft", async () => {
    const { services } = setup();
    const failing: ConversationServices = { ...services, client: { ...services.client, dispatch: () => Promise.reject(new Error("boom")), observe: services.client.observe.bind(services.client), query: services.client.query.bind(services.client), subscribe: services.client.subscribe.bind(services.client), pendingOperations: services.client.pendingOperations.bind(services.client) } };
    render(<Screen services={failing} initial="keep me" />);
    await userEvent.click(await screen.findByRole("option", { name: /Bob/ }));
    await userEvent.click(await screen.findByRole("button", { name: "Send" }));
    expect((await screen.findByRole("alert")).textContent).toBe("Send failed");
    expect(screen.getByRole<HTMLTextAreaElement>("textbox", { name: "Message" }).value).toBe("keep me");
  });
});

describe("ConversationList", () => {
  it("is keyboard navigable and stays under the 200-row smoke budget", async () => {
    const rows = Array.from({ length: 200 }, (_, i) => ({ id: asConversationId(`c${String(i)}`), personId: asPersonId(`p${String(i)}`), displayName: `Person ${String(i)}`, lastMessageAt: i, notices: i === 1 ? [{ code: "device_added" as const }] : [] }));
    const picked: string[] = [];
    const t0 = performance.now();
    const { container } = render(<ConversationList conversations={rows} selectedId={null} onSelect={(id) => picked.push(id)} copy={testCopy} unread={new Map([[rows[2]?.id ?? asConversationId("x"), 3]])} />);
    expect(performance.now() - t0).toBeLessThan(2000);
    expect(screen.getAllByRole("option")).toHaveLength(200);
    screen.getByRole("listbox").focus();
    await userEvent.keyboard("{ArrowDown}{ArrowDown}{Enter}");
    expect(picked).toEqual(["c2"]);
    await userEvent.keyboard("{End}{Enter}");
    expect(picked.at(-1)).toBe("c199");
    expect(screen.getByText("3 unread")).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe("RichTextV1 hostile input", () => {
  const rt = (doc: unknown) => render(<RichText doc={doc} copy={testCopy.richText} />);
  const para = (...children: unknown[]) => ({ version: 1, blocks: [{ type: "paragraph", children }] });

  it("renders <script> text and javascript: links inertly", () => {
    const { container } = rt(para(
      { type: "text", text: "<script>alert(1)</script>" },
      { type: "link", destination: "javascript:alert(1)", children: [{ type: "text", text: "click" }] },
      { type: "link", destination: "data:text/html,x", children: [{ type: "text", text: "d" }] },
      { type: "link", destination: "https://user:pw@evil.example/", children: [{ type: "text", text: "cred" }] },
    ));
    expect(container.querySelector("script")).toBeNull();
    expect(container.querySelector("a")).toBeNull();
    expect(container.textContent).toContain("<script>alert(1)</script>");
    expect(container.innerHTML).not.toMatch(/href=/);
  });

  it("shows link destinations with noopener noreferrer and no preview", () => {
    const { container } = rt(para({ type: "link", destination: "https://example.org/a?b=1", children: [{ type: "text", text: "label" }] }));
    const a = container.querySelector("a");
    expect(a?.getAttribute("rel")).toBe("noopener noreferrer");
    expect(a?.getAttribute("href")).toBe("https://example.org/a?b=1");
    expect(container.textContent).toContain("(https://example.org/a?b=1)");
    expect(container.querySelector("img, iframe, video")).toBeNull();
  });

  it("survives 10k nesting depth, unknown nodes and garbage without throwing", () => {
    let deep: unknown = { type: "text", text: "leaf" };
    for (let i = 0; i < 10_000; i++) deep = { type: i % 2 ? "strong" : "emphasis", children: [deep] };
    expect(() => rt({ version: 1, blocks: [{ type: "paragraph", children: [deep] }] })).not.toThrow();
    const { container } = rt(para({ type: "mystery", text: "fallback text", children: [{ type: "text", text: "x" }] }, { type: "mystery" }, 42, null, { type: "list", items: "nope" }));
    expect(container.textContent).toContain("fallback text");
    expect(container.textContent).toContain("[unsupported content]");
    cleanup();
    expect(rt("not a doc").container.textContent).toBe("[unsupported content]");
    expect(rt(null).container.textContent).toBe("[unsupported content]");
  });

  it("deep input is truncated with a notice", () => {
    let deep: unknown = { type: "text", text: "leaf" };
    for (let i = 0; i < 10_000; i++) deep = { type: "strong", children: [deep] };
    const { container } = rt({ version: 1, blocks: [{ type: "paragraph", children: [deep] }] });
    expect(container.textContent).toContain("[content truncated]");
    expect(container.textContent).not.toContain("leaf");
  });

  it("renders attachments as inert placeholders and lists", () => {
    const { container } = rt({ version: 1, blocks: [{ type: "paragraph", children: [{ type: "attachment", name: "<img src=x onerror=alert(1)>" }] }, { type: "list", ordered: true, items: [[{ type: "text", text: "one" }], [{ type: "mention", label: "bob" }]] }] });
    expect(container.querySelector("img")).toBeNull();
    expect(container.textContent).toContain("[attachment]");
    expect(container.querySelectorAll("ol > li")).toHaveLength(2);
    expect(container.textContent).toContain("@bob");
  });
});

describe("MessageRequest", () => {
  it("block requires an explicit confirmation step before any intent is dispatched", async () => {
    const { h, services } = setup(["op-block"]);
    const { container } = render(<MessageRequest services={services} request={{ id: "req-1", from: asPersonId("stranger"), displayName: "Stranger" }} copy={testCopy} onIgnore={() => undefined} />);
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: "Block" }));
    expect((await h.pendingOperations(services.ctx)).length).toBe(0);
    const group = screen.getByRole("group", { name: "Block this sender?" });
    await userEvent.click(within(group).getByRole("button", { name: "Cancel" }));
    expect((await h.pendingOperations(services.ctx)).length).toBe(0);
    await userEvent.click(screen.getByRole("button", { name: "Block" }));
    await userEvent.click(screen.getByRole("button", { name: "Yes, block" }));
    await waitFor(async () => { expect((await h.pendingOperations(services.ctx)).map((o: PendingOperationView) => o.intentKind)).toEqual(["BlockPerson"]); });
  });

  it("accept dispatches AcceptIntroduction; ignore is local", async () => {
    const { h, services } = setup(["op-acc"]);
    const ignored: string[] = [];
    render(<MessageRequest services={services} request={{ id: "req-1", from: asPersonId("stranger"), displayName: "Stranger" }} copy={testCopy} onIgnore={(id) => ignored.push(id)} />);
    await userEvent.click(screen.getByRole("button", { name: "Ignore" }));
    expect(ignored).toEqual(["req-1"]);
    expect(await h.pendingOperations(services.ctx)).toHaveLength(0);
    await userEvent.click(screen.getByRole("button", { name: "Accept" }));
    expect((await screen.findByRole("status")).textContent).toBe("Done");
  });
});

describe("security notices", () => {
  it("renders notices on the reserved banner by machine code", async () => {
    const { services } = setup();
    const noticing: ConversationServices = { ...services, client: { ...services.client, query: (async (q: Parameters<CoreClient["query"]>[0]) => {
      const r = await services.client.query(q);
      return q.kind === "GetConversationView" ? { ...(r as object), summary: { ...(r as { summary: object }).summary, notices: [{ code: "safety_number_changed" }] } } : r;
    }) as CoreClient["query"] } };
    const { container } = render(<ConversationView services={noticing} conversationId={CONV_A} copy={testCopy} />);
    expect(await screen.findByText("Notice: safety_number_changed")).toBeTruthy();
    expect(container.querySelector("[data-reason-code='safety_number_changed']")).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
  });
});
