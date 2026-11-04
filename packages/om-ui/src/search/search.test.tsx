// @vitest-environment jsdom
import { act, cleanup, render, renderHook, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { asConversationId, asMessageId, asOperationId, asPersonId, asProfileId } from "@om/platform-api";
import type { ConversationTimelineItem } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import type { ConversationServices } from "../conversations/services";
import { LocalSearchIndex } from "./LocalSearchIndex";
import { useLocalSearch } from "./useLocalSearch";

afterEach(cleanup);

const C1 = asConversationId("c1");
const C2 = asConversationId("c2");
const item = (n: number, text: string, extra: Partial<ConversationTimelineItem> = {}): ConversationTimelineItem =>
  ({ direction: "incoming", messageId: asMessageId(`m${String(n)}`), text, sentAt: n * 1000, ...extra }) as ConversationTimelineItem;
const withFile = (name: string) => ({ attachments: [{ index: 0, blobId: "0".repeat(64), mediaType: "application/pdf", size: 10, filename: name, state: "remote" as const }] });

describe("LocalSearchIndex", () => {
  it("finds message text and file names, folding case and accents, and needs every typed word", () => {
    const ix = new LocalSearchIndex();
    ix.replace(C1, "Sam", [item(1, "Meet at the Café on Friday"), item(2, "see attached", withFile("Lease Agreement.pdf")), item(3, "unrelated")]);
    expect(ix.search("CAFE").map((h) => h.messageId)).toEqual(["m1"]);
    expect(ix.search("friday meet").map((h) => h.messageId)).toEqual(["m1"]);
    expect(ix.search("friday lease")).toEqual([]);
    const file = ix.search("lease")[0];
    expect(file).toMatchObject({ kind: "file", snippet: "Lease Agreement.pdf", displayName: "Sam", messageId: "m2" });
    expect(ix.search("   ")).toEqual([]);
  });

  it("ranks a whole phrase before scattered words, then newest first, and caps the result", () => {
    const ix = new LocalSearchIndex();
    ix.replace(C1, "A", [item(1, "red car"), item(2, "a car that is red"), item(3, "red car again")]);
    expect(ix.search("red car").map((h) => h.messageId)).toEqual(["m3", "m1", "m2"]);
    expect(ix.search("red car", 2)).toHaveLength(2);
    expect(ix.search("red car", 0)).toEqual([]);
  });

  it("keeps a one-line snippet around the match in long text", () => {
    const ix = new LocalSearchIndex();
    const long = `${"filler ".repeat(40)}needle${" tail".repeat(40)}`;
    ix.replace(C1, "A", [item(1, long)]);
    const s = ix.search("needle")[0]?.snippet ?? "";
    expect(s).toContain("needle");
    expect(s.length).toBeLessThanOrEqual(95);
    expect(s.startsWith("…") && s.endsWith("…")).toBe(true);
  });

  it("bounds what it holds, forgets a removed chat, and clear() leaves nothing", () => {
    const ix = new LocalSearchIndex({ perConversation: 2, conversations: 1 });
    ix.replace(C1, "A", [item(1, "old"), item(2, "newer"), item(3, "newest")]);
    ix.replace(C2, "B", [item(9, "ignored")]);
    expect(ix.size).toBe(2);
    expect(ix.search("old")).toEqual([]);
    expect(ix.search("ignored")).toEqual([]);
    ix.remove(C1);
    expect(ix.size).toBe(0);
    ix.replace(C1, "A", [item(1, "x")]);
    ix.clear();
    expect(ix.size).toBe(0);
    expect(ix.search("x")).toEqual([]);
  });

  it("never persists or uploads: no storage, network, or beacon call while indexing and searching", () => {
    const set = vi.spyOn(Storage.prototype, "setItem");
    const fetchSpy = vi.fn();
    vi.stubGlobal("fetch", fetchSpy);
    const beacon = vi.fn();
    Object.defineProperty(navigator, "sendBeacon", { value: beacon, configurable: true });
    const open = vi.spyOn(XMLHttpRequest.prototype, "open");
    const ix = new LocalSearchIndex();
    ix.replace(C1, "Sam", [item(1, "secret plan"), item(2, "x", withFile("tax.pdf"))]);
    ix.search("secret"); ix.search("tax");
    expect(set).not.toHaveBeenCalled();
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(beacon).not.toHaveBeenCalled();
    expect(open).not.toHaveBeenCalled();
    expect(globalThis.window.localStorage.length).toBe(0);
    expect(globalThis.window.sessionStorage.length).toBe(0);
    vi.unstubAllGlobals();
  });
});

describe("useLocalSearch", () => {
  const ALICE = asProfileId("alice");
  const BOB = asProfileId("bob");
  function setup() {
    const h = createHarness({ env: {}, clock: () => 5000 });
    h.seedProfile({ profileId: ALICE, label: "Alice", conversations: [{ id: C1, personId: asPersonId("sam"), displayName: "Sam", messages: [{ text: "lunch at noon", direction: "incoming", attachments: [{ filename: "menu.pdf", mediaType: "application/pdf", bytes: new Uint8Array(4) }] }] }] });
    h.seedProfile({ profileId: BOB, label: "Bob", conversations: [{ id: C2, personId: asPersonId("amy"), displayName: "Amy", messages: [{ text: "bobs private note", direction: "incoming" }] }] });
    let n = 0;
    const registry = createActionRegistry({ newOperationId: () => asOperationId(`op-${String(++n)}`) });
    const services = (id: typeof ALICE): ConversationServices => ({ client: h, registry, ctx: h.contextFor(id) });
    return { h, services, registry };
  }

  it("builds from Core's decrypted views after mount, follows new messages, and wipes on unmount", async () => {
    const { h, services } = setup();
    const s = services(ALICE);
    const { result, unmount } = renderHook(() => useLocalSearch(s));
    await waitFor(() => { expect(result.current.ready).toBe(true); });
    expect(result.current.search("noon").map((x) => x.messageId)).toHaveLength(1);
    expect(result.current.search("menu")[0]).toMatchObject({ kind: "file", snippet: "menu.pdf" });
    const dispatched = await h.dispatch({ kind: "SendMessage", ctx: s.ctx, operationId: asOperationId("new-1"), conversationId: C1, draft: { text: "dessert later" } });
    expect(dispatched.profileId).toBe(ALICE);
    await waitFor(() => { expect(result.current.search("dessert")).toHaveLength(1); });
    const lastSearch = result.current.search;
    unmount();
    expect(lastSearch("noon")).toEqual([]); // the index was wiped when the screen went away
    expect(lastSearch("dessert")).toEqual([]);
  });

  it("another identity never sees this one's text, and switching starts from an empty index", async () => {
    const { services } = setup();
    const a = services(ALICE);
    const b = services(BOB);
    const { result, rerender } = renderHook(({ s }) => useLocalSearch(s), { initialProps: { s: a } });
    await waitFor(() => { expect(result.current.search("noon")).toHaveLength(1); });
    const aliceSearch = result.current.search;
    rerender({ s: b });
    expect(aliceSearch("noon")).toEqual([]); // the old index was wiped at the switch
    await waitFor(() => { expect(result.current.search("private")).toHaveLength(1); });
    expect(result.current.search("noon")).toEqual([]);
    expect(result.current.search("menu")).toEqual([]);
  });

  it("builds the same index when used inside a component", async () => {
    const { services } = setup();
    const s = services(ALICE);
    function Probe() {
      const { search, ready } = useLocalSearch(s);
      return <p>{ready ? `ready ${String(search("lunch").length)}` : "building"}</p>;
    }
    render(<Probe />);
    await screen.findByText("ready 1");
    await act(async () => { await Promise.resolve(); });
  });
});
