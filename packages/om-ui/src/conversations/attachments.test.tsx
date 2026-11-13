// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { asConversationId, asOperationId, asPersonId, asProfileId } from "@om/platform-api";
import type { AudioRecording, CoreClient, FilesApi, LocationApi, MicrophoneApi, PickedFile } from "@om/platform-api";
import { createHarness, type HarnessOptions, type SeedMessage } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import { testCopy } from "./__fixtures__/copy";
import { Composer, ConversationView, type ComposerCapture, type ConversationServices } from "./index";

afterEach(cleanup);
const axeViolations = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};

const ALICE = asProfileId("alice");
const CONV = asConversationId("conv-ab");
const bytes = (n: number): Uint8Array => new Uint8Array(n).fill(7);

function setup(messages: readonly SeedMessage[] = [], options: HarnessOptions = {}) {
  const h = createHarness({ env: {}, clock: () => 1_700_000_000_000, ...options });
  h.seedProfile({ profileId: ALICE, label: "Alice", conversations: [{ id: CONV, personId: asPersonId("bob-person"), displayName: "Bob", messages }] });
  let n = 0;
  const registry = createActionRegistry({ newOperationId: () => asOperationId(`op-${String(++n)}`) });
  const make = (client: CoreClient): ConversationServices => ({ client, registry, ctx: h.contextFor(ALICE) });
  return { h, make };
}

const picked = (name: string, size: number, mimeType = "text/plain"): PickedFile => ({ name, size, mimeType, bytes: () => Promise.resolve(bytes(size)) });
const files = (...f: PickedFile[]): FilesApi => ({ pick: vi.fn(() => Promise.resolve(f)) });

function recorder(): { readonly mic: MicrophoneApi; readonly stop: ReturnType<typeof vi.fn>; readonly cancel: ReturnType<typeof vi.fn>; readonly started: ReturnType<typeof vi.fn> } {
  const stop = vi.fn(() => Promise.resolve({ bytes: bytes(120), mimeType: "audio/webm", durationMs: 4200 }));
  const cancel = vi.fn();
  const rec: AudioRecording = { stop, cancel };
  const started = vi.fn(() => Promise.resolve(rec));
  return { mic: { requestAccess: () => Promise.resolve("granted" as const), startRecording: started }, stop, cancel, started };
}

function Screen({ services, capture }: { readonly services: ConversationServices; readonly capture: ComposerCapture }) {
  const [draft, setDraft] = useState("");
  return (
    <ConversationView services={services} conversationId={CONV} copy={testCopy}
      footer={<Composer services={services} conversationId={CONV} copy={testCopy} draft={draft} onDraftChange={setDraft} capture={capture} />} />
  );
}

beforeEach(() => {
  // jsdom has no object URLs and does not navigate on a download click.
  URL.createObjectURL = vi.fn(() => "blob:test");
  URL.revokeObjectURL = vi.fn();
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => undefined);
  vi.spyOn(HTMLMediaElement.prototype, "play").mockImplementation(() => Promise.resolve());
});

describe("attaching files", () => {
  it("shows what is picked, sends it, and the message carries name, size and how long it is kept", async () => {
    const { h, make } = setup();
    const services = make(h);
    const { container } = render(<Screen services={services} capture={{ files: files(picked("notes.txt", 300)) }} />);
    await userEvent.click(await screen.findByRole("button", { name: "Attach a file" }));
    const tray = await screen.findByRole("list", { name: "Ready to send" });
    expect(within(tray).getByText("notes.txt")).toBeTruthy();
    expect(within(tray).getByText("300 bytes")).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
    // A file alone is enough to send.
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    const log = await screen.findByRole("log");
    await waitFor(() => { expect(within(log).getByText("notes.txt")).toBeTruthy(); });
    expect(within(log).getByText(/300 bytes · Kept until day/)).toBeTruthy();
    expect(screen.queryByRole("list", { name: "Ready to send" })).toBeNull();
    // Nothing waits to be downloaded on our own copy: it is ours.
    expect(within(log).queryByRole("button", { name: /Download/ })).toBeNull();
    expect(await axeViolations(container)).toEqual([]);
  });

  it("refuses a file over the delivery service's ceiling before reading it, in plain words, with no retry", async () => {
    const { h, make } = setup([], { attachmentLimits: { maxBlobBytes: 116 } });
    const big = picked("huge.bin", 500);
    const readBytes = vi.spyOn(big, "bytes");
    render(<Screen services={make(h)} capture={{ files: files(big) }} />);
    await userEvent.click(await screen.findByRole("button", { name: "Attach a file" }));
    // Core says it may still go straight to them, but this host has no way to do that: say both, promise nothing.
    expect((await screen.findByRole("alert")).textContent).toBe("Too big for the service, not sent. Up to 100. Straight to them: not available here yet.");
    expect(readBytes).not.toHaveBeenCalled();
    expect(screen.queryByRole("list", { name: "Ready to send" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Retry sending" })).toBeNull();
  });

  it("when Core refuses a file as too large at send time it leaves the tray and nothing offers a retry; the words stay", async () => {
    const { h, make } = setup();
    const client: CoreClient = { ...h, attachments: { ...h.attachments, limits: () => null, prepare: () => Promise.resolve({ ok: false, refusal: { error: "too_large", max: 50 } }) } };
    render(<Screen services={make(client)} capture={{ files: files(picked("big.bin", 900)) }} />);
    await userEvent.type(await screen.findByRole("textbox", { name: "Message" }), "here you go");
    await userEvent.click(screen.getByRole("button", { name: "Attach a file" }));
    await screen.findByRole("list", { name: "Ready to send" });
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    expect((await screen.findByRole("alert")).textContent).toBe("Too big: 50");
    expect(screen.queryByRole("list", { name: "Ready to send" })).toBeNull();
    expect(screen.getByRole<HTMLTextAreaElement>("textbox", { name: "Message" }).value).toBe("here you go");
    expect(screen.queryByRole("button", { name: "Retry sending" })).toBeNull();
  });

  it("another refusal keeps the file in the tray so the same send can be tried again", async () => {
    const { h, make } = setup();
    let calls = 0;
    const client: CoreClient = { ...h, attachments: { ...h.attachments, prepare: (ctx, b, m) => (++calls === 1 ? Promise.resolve({ ok: false, refusal: { error: "unavailable" } }) : h.attachments.prepare(ctx, b, m)) } };
    render(<Screen services={make(client)} capture={{ files: files(picked("a.txt", 20)) }} />);
    await userEvent.click(await screen.findByRole("button", { name: "Attach a file" }));
    await userEvent.click(await screen.findByRole("button", { name: "Send" }));
    expect((await screen.findByRole("alert")).textContent).toBe("Refused: unavailable");
    expect(screen.getByRole("list", { name: "Ready to send" })).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => { expect(screen.queryByRole("list", { name: "Ready to send" })).toBeNull(); });
    expect(within(screen.getByRole("log")).getByText("a.txt")).toBeTruthy();
  });

  it("the tray item can be removed, and controls appear only for what the device and Core support", async () => {
    const { h, make } = setup();
    const { rerender } = render(<Screen services={make(h)} capture={{}} />);
    expect(screen.queryByRole("button", { name: "Attach a file" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Record a voice note" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Share your location" })).toBeNull();
    // A Core with no file path offers neither files nor voice, but location still works (it is text).
    const rest: Record<string, unknown> = { ...h };
    delete rest["attachments"];
    const noFiles = rest as unknown as CoreClient;
    rerender(<Screen services={make(noFiles)} capture={{ files: files(), microphone: recorder().mic, location: { current: () => Promise.resolve("unavailable") } }} />);
    expect(await screen.findByRole("button", { name: "Share your location" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Attach a file" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Record a voice note" })).toBeNull();
    rerender(<Screen services={make(h)} capture={{ files: files(picked("x.txt", 5)) }} />);
    await userEvent.click(await screen.findByRole("button", { name: "Attach a file" }));
    await userEvent.click(await screen.findByRole("button", { name: "Remove x.txt" }));
    expect(screen.queryByRole("list", { name: "Ready to send" })).toBeNull();
  });
});

describe("voice notes", () => {
  it("record, review, then an explicit send; the microphone is released and nothing is sent before Send", async () => {
    const { h, make } = setup();
    const r = recorder();
    const dispatch = vi.spyOn(h, "dispatch");
    const { container } = render(<Screen services={make(h)} capture={{ microphone: r.mic }} />);
    expect(r.started).not.toHaveBeenCalled(); // no microphone request until the person starts
    await userEvent.click(await screen.findByRole("button", { name: "Record a voice note" }));
    const bar = await screen.findByRole("group", { name: "Recording" });
    expect(within(bar).getByRole("status").textContent).toBe("Recording");
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Send" }).disabled).toBe(true);
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Record a voice note" }).disabled).toBe(true);
    await userEvent.click(within(bar).getByRole("button", { name: "Stop recording" }));
    expect(r.stop).toHaveBeenCalledOnce();
    const tray = await screen.findByRole("list", { name: "Ready to send" });
    expect(within(tray).getByText("Listen before you send")).toBeTruthy();
    expect(tray.querySelector("audio")).not.toBeNull();
    expect(screen.queryByRole("group", { name: "Recording" })).toBeNull();
    expect(dispatch).not.toHaveBeenCalled();
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    const log = screen.getByRole("log");
    await waitFor(() => { expect(within(log).getByText("Voice note")).toBeTruthy(); });
    expect(dispatch).toHaveBeenCalledOnce();
    expect(dispatch.mock.calls[0]?.[0]).toMatchObject({ kind: "SendAttachments", attachments: [{ voiceNote: {} }] });
    expect(r.cancel).not.toHaveBeenCalled();
  });

  it("discarding drops the recording and releases the microphone; a refusal to record says why in plain words", async () => {
    const { h, make } = setup();
    const r = recorder();
    const { rerender } = render(<Screen services={make(h)} capture={{ microphone: r.mic }} />);
    await userEvent.click(await screen.findByRole("button", { name: "Record a voice note" }));
    await userEvent.click(await screen.findByRole("button", { name: "Discard" }));
    expect(r.cancel).toHaveBeenCalledOnce();
    expect(screen.queryByRole("group", { name: "Recording" })).toBeNull();
    expect(screen.queryByRole("list", { name: "Ready to send" })).toBeNull();
    rerender(<Screen services={make(h)} capture={{ microphone: { requestAccess: () => Promise.resolve("denied"), startRecording: () => Promise.resolve("denied") } }} />);
    await userEvent.click(await screen.findByRole("button", { name: "Record a voice note" }));
    expect((await screen.findByRole("alert")).textContent).toBe("Microphone blocked");
  });

  it("leaving the conversation while recording ends the recording", async () => {
    const { h, make } = setup();
    const r = recorder();
    const { unmount } = render(<Screen services={make(h)} capture={{ microphone: r.mic }} />);
    await userEvent.click(await screen.findByRole("button", { name: "Record a voice note" }));
    await screen.findByRole("group", { name: "Recording" });
    unmount();
    expect(r.cancel).toHaveBeenCalledOnce();
  });
});

describe("location cards", () => {
  const where: LocationApi = { current: () => Promise.resolve({ latE7: 515_074_000, lonE7: -1_278_000, accuracyM: 24.4 }) };

  it("asks first, sends nothing until confirmed, and then sends one fixed point (not live)", async () => {
    const { h, make } = setup();
    const dispatch = vi.spyOn(h, "dispatch");
    const { container } = render(<Screen services={make(h)} capture={{ location: where }} />);
    await userEvent.click(await screen.findByRole("button", { name: "Share your location" }));
    const panel = await screen.findByRole("group", { name: "Share your location?" });
    expect(panel.textContent).toContain("51.5074, -0.1278");
    expect(panel.textContent).toContain("About 24 m");
    expect(panel.textContent).toContain("once");
    expect(document.activeElement).toBe(panel);
    expect(dispatch).not.toHaveBeenCalled();
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(within(panel).getByRole("button", { name: "Cancel" }));
    expect(dispatch).not.toHaveBeenCalled();
    expect(screen.queryByRole("group", { name: "Share your location?" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Share your location" }));
    await userEvent.click(await screen.findByRole("button", { name: "Send location" }));
    const log = screen.getByRole("log");
    await waitFor(() => { expect(within(log).getByRole("group", { name: "Location" })).toBeTruthy(); });
    expect(dispatch).toHaveBeenCalledOnce();
    expect(dispatch.mock.calls[0]?.[0]).toMatchObject({ kind: "SendLocation", latE7: 515_074_000, lonE7: -1_278_000, accuracyM: 24 });
    expect(screen.queryByRole("group", { name: "Share your location?" })).toBeNull();
    expect(await axeViolations(container)).toEqual([]);
  });

  it("explains a refusal or a failure to find the position", async () => {
    const { h, make } = setup();
    const { rerender } = render(<Screen services={make(h)} capture={{ location: { current: () => Promise.resolve("denied") } }} />);
    await userEvent.click(await screen.findByRole("button", { name: "Share your location" }));
    expect((await screen.findByRole("alert")).textContent).toBe("Location blocked");
    rerender(<Screen services={make(h)} capture={{ location: { current: () => Promise.resolve("unavailable") } }} />);
    await userEvent.click(await screen.findByRole("button", { name: "Share your location" }));
    await waitFor(() => { expect(screen.getByRole("alert").textContent).toBe("Location unavailable"); });
  });
});

describe("opening what someone sent", () => {
  const incoming: SeedMessage[] = [
    { text: "the report", direction: "incoming", attachments: [{ filename: "../../report.pdf", mediaType: "application/pdf", bytes: bytes(2048) }] },
    { text: "", direction: "incoming", attachments: [{ mediaType: "audio/webm", bytes: bytes(300), durationMs: 5000 }], voiceNote: 0 },
    { text: "geo:51.5,-0.1", direction: "incoming", locationCard: { latE7: 515_000_000, lonE7: -1_000_000, accuracyM: 10, label: "Cafe" } },
  ];

  it("fetches only when asked, saves under a safe name, and never renders the file", async () => {
    const { h, make } = setup(incoming);
    const open = vi.spyOn(h.attachments, "open");
    const { container } = render(<Screen services={make(h)} capture={{}} />);
    const log = await screen.findByRole("log");
    await waitFor(() => { expect(within(log).getByText("report.pdf")).toBeTruthy(); });
    expect(open).not.toHaveBeenCalled(); // opening the chat fetches nothing
    expect(within(log).getByText("2048 bytes")).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
    const anchors: HTMLAnchorElement[] = [];
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (this: HTMLAnchorElement) { anchors.push(this); });
    await userEvent.click(within(log).getByRole("button", { name: "Download report.pdf" }));
    await waitFor(() => { expect(within(log).getByText("Download started")).toBeTruthy(); });
    expect(open).toHaveBeenCalledOnce();
    expect(anchors[0]?.download).toBe("report.pdf");
    expect(log.querySelector("img, iframe, object, embed")).toBeNull();
  });

  it("a voice note plays on request only, and a damaged or missing file is said plainly", async () => {
    const { h, make } = setup(incoming);
    render(<Screen services={make(h)} capture={{}} />);
    const log = await screen.findByRole("log");
    expect(log.querySelector("audio")).toBeNull();
    await userEvent.click(await within(log).findByRole("button", { name: "Play voice note" }));
    await waitFor(() => { expect(log.querySelector("audio")).not.toBeNull(); });
    cleanup();
    const failing: CoreClient = { ...h, attachments: { ...h.attachments, open: () => Promise.resolve({ ok: false, refusal: { error: "integrity_failure" } }) } };
    render(<Screen services={make(failing)} capture={{}} />);
    await userEvent.click(await screen.findByRole("button", { name: "Download report.pdf" }));
    expect((await screen.findAllByRole("status")).some((s) => s.textContent === "Refused: integrity_failure")).toBe(true);
  });

  it("a location card shows the point, label and accuracy as text, without fetching a map", async () => {
    const { h, make } = setup(incoming);
    render(<Screen services={make(h)} capture={{}} />);
    const card = await screen.findByRole("group", { name: "Location" });
    expect(card.textContent).toContain("Cafe");
    expect(card.textContent).toContain("51.5000, -0.1000");
    expect(card.textContent).toContain("About 10 m");
    expect(card.querySelector("img, iframe")).toBeNull();
  });
});

describe("a file too big for the delivery service, sent straight to them", () => {
  const LIMITS = { attachmentLimits: { maxBlobBytes: 116 } } as const;

  it("with a way to send it, the refusal says so, the file waits in the tray and goes in a message of its own: waiting, nothing kept for later, then taken back", async () => {
    const { h, make } = setup([], LIMITS);
    const services = { ...make(h), directChannel: true };
    const { container } = render(<Screen services={services} capture={{ files: files(picked("film.mov", 500)) }} />);
    await userEvent.click(await screen.findByRole("button", { name: "Attach a file" }));
    expect((await screen.findByRole("alert")).textContent).toBe("Too big for the service, not sent. Up to 100. Send it straight to them while they are online.");
    const tray = await screen.findByRole("list", { name: "Ready to send" });
    expect(within(tray).getByText(/500 bytes · Send it straight to them/)).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    const log = await screen.findByRole("log");
    await waitFor(() => { expect(within(log).getByText(/500 bytes · Waiting for them/)).toBeTruthy(); });
    expect(within(log).queryByText(/Kept until/)).toBeNull();
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(within(log).getByRole("button", { name: "Take it back" }));
    await waitFor(() => { expect(within(log).getByText(/No longer offered/)).toBeTruthy(); });
    expect(within(log).queryByRole("button", { name: "Take it back" })).toBeNull();
    expect(await h.query({ kind: "GetDirectOffers", ctx: services.ctx })).toMatchObject([{ state: "not_offered", size: 500 }]);
  });

  it("a file that fits the service is untouched by all this; a file too big for anything is refused outright", async () => {
    const { h, make } = setup([], { attachmentLimits: { maxBlobBytes: 116 } });
    const services = { ...make(h), directChannel: true };
    render(<Screen services={services} capture={{ files: files(picked("ok.txt", 50), picked("enormous.bin", 17 * 1024 * 1024)) }} />);
    await userEvent.click(await screen.findByRole("button", { name: "Attach a file" }));
    const tray = await screen.findByRole("list", { name: "Ready to send" });
    expect(within(tray).getByText("ok.txt")).toBeTruthy();
    expect(within(tray).queryByText("enormous.bin")).toBeNull();
    expect((await screen.findByRole("alert")).textContent).toBe("This file is too big to send right now. Up to 100 bytes.");
  });

  it("a straight-to-them file cannot share a message with ordinary files", async () => {
    const { h, make } = setup([], LIMITS);
    render(<Screen services={{ ...make(h), directChannel: true }} capture={{ files: files(picked("small.txt", 50), picked("film.mov", 500)) }} />);
    await userEvent.click(await screen.findByRole("button", { name: "Attach a file" }));
    await screen.findByRole("list", { name: "Ready to send" });
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    expect((await screen.findByRole("alert")).textContent).toBe("Straight-to-them files go alone.");
    expect(await h.query({ kind: "GetDirectOffers", ctx: make(h).ctx })).toEqual([]);
  });

  const seeded = (state: "waiting_for_recipient" | "transferring" | "delivered_locally" | "not_offered" | "remote_direct", direct: boolean) => {
    const { h, make } = setup([{ direction: state === "remote_direct" ? "incoming" : "outgoing", text: "big", attachments: [{ filename: "film.mov", mediaType: "video/mp4", bytes: bytes(500) }] }], LIMITS);
    // The harness shows seeded files as remote or stored; show the state under test the way Core reports it.
    const client: CoreClient = {
      ...h,
      query: async (q: { readonly kind: string }) => {
        const r: unknown = await h.query(q as never);
        if (q.kind !== "GetConversationView") return r;
        const v = r as { timeline: { attachments?: { state: string }[] }[] };
        return { ...v, timeline: v.timeline.map((t) => ({ ...t, attachments: t.attachments?.map((a) => ({ ...a, state })) })) };
      },
    } as unknown as CoreClient;
    return { services: { ...make(client), directChannel: direct } };
  };

  it.each([
    ["waiting_for_recipient", true, /Waiting for them/],
    ["waiting_for_recipient", false, /Not sent: no way to send straight to them here\./],
    ["transferring", true, /Sending now/],
    ["delivered_locally", true, /They have it/],
    ["not_offered", true, /No longer offered/],
    ["remote_direct", true, /Available while they are online/],
    ["remote_direct", false, /cannot receive here yet/],
  ] as const)("states plainly: %s (way to send: %s)", async (state, direct, words) => {
    const { services } = seeded(state, direct);
    const { container } = render(<Screen services={services} capture={{}} />);
    const log = await screen.findByRole("log");
    await waitFor(() => { expect(within(log).getByText(words)).toBeTruthy(); });
    // Never a download for a direct file here, and never a withdraw for one that is not ours or no longer waiting.
    expect(within(log).queryByRole("button", { name: /Download/ })).toBeNull();
    expect(!!within(log).queryByRole("button", { name: "Take it back" })).toBe(state === "waiting_for_recipient" || state === "transferring");
    expect(await axeViolations(container)).toEqual([]);
  });
});
