// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Sheet } from "../shell/layout/Sheet";
import { Omnibar, OmnibarTrigger } from "./Omnibar";
import type { OmnibarCopy } from "./copy";
import { rankEntries, scoreEntry, type OmniEntry } from "./match";

afterEach(cleanup);

const copy: OmnibarCopy = {
  name: "Omnibar", triggerText: "Find or do…", openAction: "Open Omnibar", placeholder: "Find or do…", close: "Close Omnibar", results: "Results", suggested: "Suggested",
  kinds: { conversation: "Chat", action: "Action", setting: "Setting", message: "Message", file: "File" }, none: (q) => `Nothing matches ${q}`, count: (n) => `${String(n)} results`,
};
const entry = (id: string, kind: OmniEntry["kind"], label: string, run: () => void = () => undefined, extra: Partial<OmniEntry> = {}): OmniEntry => ({ id, kind, label, run, ...extra });

describe("Omnibar ranking", () => {
  const maria = entry("c1", "conversation", "Maria");
  const marketing = entry("c2", "conversation", "Marketing team");
  const scan = entry("a1", "action", "Scan QR code", undefined, { keywords: "camera invite" });
  const delivery = entry("s1", "setting", "Message delivery");

  it("a better match always wins, then chat before action before setting", () => {
    const all = [delivery, scan, marketing, maria];
    expect(rankEntries(all, "mar").map((e) => e.id)).toEqual(["c2", "c1"]); // equal matches keep the host's order
    expect(rankEntries(all, "maria")[0]?.id).toBe("c1");
    expect(rankEntries(all, "deliv").map((e) => e.id)).toEqual(["s1"]);
  });
  it("matches hidden keywords and every typed word, ignoring case and accents", () => {
    expect(rankEntries([scan, maria], "CAMERA")).toEqual([scan]);
    expect(rankEntries([entry("x", "conversation", "José")], "jose")).toHaveLength(1);
    expect(rankEntries([scan], "scan nothing")).toEqual([]);
  });
  it("shows only the suggested entries before anything is typed, in the host's order", () => {
    const a = entry("a", "action", "Invite", undefined, { suggested: true });
    const b = entry("b", "conversation", "Bob", undefined, { suggested: true });
    expect(rankEntries([a, entry("z", "action", "Hidden"), b], "  ").map((e) => e.id)).toEqual(["a", "b"]);
  });
  it("an exact match is not buried by order (nothing is learned)", () => {
    const many = Array.from({ length: 30 }, (_, i) => entry(`n${String(i)}`, "conversation", `Chat ${String(i)} Ana`));
    expect(rankEntries([...many, entry("ana", "conversation", "Ana")], "ana")[0]?.id).toBe("ana");
    expect(scoreEntry(scan, "qr")).not.toBeNull();
  });
});

function Host({ entries, dynamic }: { readonly entries: readonly OmniEntry[]; readonly dynamic?: (q: string) => readonly OmniEntry[] }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <OmnibarTrigger copy={copy} onOpen={() => { setOpen(true); }} />
      {open ? <Omnibar entries={entries} {...(dynamic ? { dynamic } : {})} copy={copy} onClose={() => { setOpen(false); }} /> : null}
    </>
  );
}

describe("Omnibar", () => {
  it("opens with focus in a combobox, suggestions known before the first key", async () => {
    render(<Host entries={[entry("a", "action", "Invite someone", undefined, { suggested: true })]} />);
    await userEvent.click(screen.getByRole("button", { name: /Omnibar/ }));
    const box = screen.getByRole("combobox", { name: "Omnibar" });
    expect(document.activeElement).toBe(box);
    expect(screen.getByText("Suggested")).toBeTruthy();
    expect(within(screen.getByRole("listbox")).getAllByRole("option")).toHaveLength(1);
    expect(box.getAttribute("aria-activedescendant")).toBe(screen.getByRole("option").id);
  });

  it("types, arrows and Enter run the armed result; the Omnibar closes first", async () => {
    const runA = vi.fn();
    const runB = vi.fn();
    render(<Host entries={[entry("a", "conversation", "Maria", runA), entry("b", "conversation", "Marco", runB)]} />);
    await userEvent.click(screen.getByRole("button", { name: /Omnibar/ }));
    await userEvent.keyboard("mar");
    expect(screen.getAllByRole("option")).toHaveLength(2);
    await userEvent.keyboard("{ArrowDown}{Enter}");
    expect(runB).toHaveBeenCalledOnce();
    expect(runA).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("Escape closes it and returns focus to the control that opened it", async () => {
    render(<Host entries={[]} />);
    const trigger = screen.getByRole("button", { name: /Omnibar/ });
    await userEvent.click(trigger);
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it("says plainly when nothing matches, and offers dynamic (explicit) results first", async () => {
    const run = vi.fn();
    render(<Host entries={[entry("a", "conversation", "Maria")]} dynamic={(q) => (q.startsWith("om:") ? [entry("link", "action", "Use this invite link", run)] : [])} />);
    await userEvent.click(screen.getByRole("button", { name: /Omnibar/ }));
    await userEvent.keyboard("zzz");
    expect(screen.getByText("Nothing matches zzz")).toBeTruthy();
    await userEvent.clear(screen.getByRole("combobox"));
    await userEvent.type(screen.getByRole("combobox"), "om:invite/abc");
    expect(screen.getAllByRole("option")[0]?.textContent).toContain("Use this invite link");
    await userEvent.keyboard("{Enter}");
    expect(run).toHaveBeenCalledOnce();
  });

  it("the armed result does not move when the host passes the same entries again", async () => {
    const entries = [entry("a", "conversation", "Ana"), entry("b", "conversation", "Anna")];
    const { rerender } = render(<Omnibar entries={entries} copy={copy} onClose={() => undefined} />);
    await userEvent.keyboard("an{ArrowDown}");
    const armed = (): string => screen.getAllByRole("option").find((o) => o.getAttribute("aria-selected") === "true")?.textContent ?? "";
    const before = armed();
    rerender(<Omnibar entries={[...entries]} copy={copy} onClose={() => undefined} />);
    expect(armed()).toBe(before);
  });

  it("clicking a result runs it; clicking the backdrop closes", () => {
    const run = vi.fn();
    const onClose = vi.fn();
    const { container } = render(<Omnibar entries={[entry("a", "action", "Invite someone", run, { suggested: true })]} copy={copy} onClose={onClose} />);
    fireEvent.click(screen.getByRole("option"));
    expect(run).toHaveBeenCalledOnce();
    onClose.mockClear();
    fireEvent.pointerDown(container.querySelector(".om-omni__scrim") as Element);
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("has no axe violations open or closed, and the compact trigger has a stable name", async () => {
    const { container } = render(<Host entries={[entry("a", "action", "Invite someone", undefined, { suggested: true })]} />);
    const rules = { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } };
    expect((await axe.run(container, rules)).violations.map((v) => v.id)).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: /Omnibar/ }));
    expect((await axe.run(container, rules)).violations.map((v) => v.id)).toEqual([]);
    cleanup();
    render(<OmnibarTrigger copy={copy} variant="icon" onOpen={() => undefined} />);
    expect(screen.getByRole("button", { name: "Open Omnibar" })).toBeTruthy();
  });
});

describe("Sheet", () => {
  function SheetHost() {
    const [open, setOpen] = useState(false);
    return (
      <>
        <button type="button" onClick={() => { setOpen(true); }}>open</button>
        {open ? <Sheet title="New conversation" closeLabel="Close" onClose={() => { setOpen(false); }}><button type="button" data-sheet-initial="">first</button><button type="button">last</button></Sheet> : null}
      </>
    );
  }
  it("takes focus on the requested control, holds Tab inside, closes on Escape and gives focus back", async () => {
    render(<SheetHost />);
    const opener = screen.getByRole("button", { name: "open" });
    await userEvent.click(opener);
    expect(screen.getByRole("dialog", { name: "New conversation" }).getAttribute("aria-modal")).toBe("true");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "first" }));
    await userEvent.tab(); await userEvent.tab(); await userEvent.tab();
    expect(screen.getByRole("dialog").contains(document.activeElement)).toBe(true);
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(opener);
  });
});

describe("Omnibar local search hits", () => {
  it("come after the fixed results (a typed name still lands on the chat first), are announced, and open on Enter or click", async () => {
    const opened: string[] = [];
    const found = (q: string): readonly OmniEntry[] => (q.trim() === "" ? [] : [
      entry("f0", "message", "we met at the Sam café", () => { opened.push("f0"); }, { detail: "Sam" }),
      entry("f1", "file", "sam-lease.pdf", () => { opened.push("f1"); }, { detail: "Sam" }),
    ]);
    render(<Host2 entries={[entry("c1", "conversation", "Sam", () => { opened.push("c1"); }, { suggested: true })]} found={found} />);
    await userEvent.click(screen.getByRole("button", { name: /Omnibar/ }));
    expect(screen.queryByText("we met at the Sam café")).toBeNull(); // nothing is searched before something is typed
    await userEvent.type(screen.getByRole("combobox", { name: "Omnibar" }), "sam");
    const options = screen.getAllByRole("option").map((o) => o.textContent);
    expect(options).toEqual(["SamChat", "we met at the Sam caféSamMessage", "sam-lease.pdfSamFile"]);
    expect(screen.getByRole("status").textContent).toBe("3 results");
    expect(await axeResult(document.body)).toEqual([]);
    await userEvent.keyboard("{Enter}");
    expect(opened).toEqual(["c1"]);
  });
});

function Host2({ entries, found }: { readonly entries: readonly OmniEntry[]; readonly found: (q: string) => readonly OmniEntry[] }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <OmnibarTrigger copy={copy} onOpen={() => { setOpen(true); }} />
      {open ? <Omnibar entries={entries} found={found} copy={copy} onClose={() => { setOpen(false); }} /> : null}
    </>
  );
}
const axeResult = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};
