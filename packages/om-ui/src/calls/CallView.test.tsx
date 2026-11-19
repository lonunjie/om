// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { asConversationId, type CallView as CoreCallView } from "@om/platform-api";
import { CallTimelineNotice, CallView, type CallCopy } from "./CallView";
import { ThemeProvider } from "../shell/theme/ThemeProvider";

afterEach(cleanup);

const copy: CallCopy = {
  audio: "Audio call", video: "Video call", incoming: (name, media) => `${media} from ${name}`,
  withContact: (name) => `Call with ${name}`, identity: (name) => `For ${name}`,
  state: { incoming: "Incoming", starting: "Starting", ringing: "Ringing", connecting: "Connecting", active: "Connected",
    reconnecting: "Reconnecting", ending: "Ending", ended: "Ended", failed: "Failed" },
  stillTrying: (name) => `Connecting, still trying to reach ${name}`,
  reason: { busy: "Busy", declined: "Declined", answered_elsewhere: "Answered elsewhere", timeout: "No answer",
    failed: "Failed", hangup: "Ended", canceled: "Canceled" },
  answerAudio: "Answer audio", answerVideo: "Answer video", decline: "Decline", hangUp: "End call",
  mute: "Mute microphone", unmute: "Unmute microphone", cameraOn: "Turn camera on", cameraOff: "Turn camera off",
  dismiss: "Dismiss", retry: "Try again", cancel: "Cancel", preparing: "Preparing", noDirect: "A private route is unavailable",
  missed: "Missed call", missedSeveral: "Missed calls", time: () => "12:00",
};
const incoming: CoreCallView = {
  callSessionId: "1", conversationId: asConversationId("1"), media: "video", state: "incoming", outgoing: false,
  ringDeadlineMs: 45_000, reason: null, localMediaFlags: 0, peerMediaFlags: 3, route: null,
  deliveryUnsure: false, restartAttempt: 0, wakeAtMs: null,
};

function props() {
  return {
    copy, call: incoming, name: "Maria", identity: "Work", pending: false, canUseCallRoute: true,
    cameraAvailable: false, error: null, onAnswer: vi.fn(), onEnd: vi.fn(), onToggle: vi.fn(), onDismiss: vi.fn(),
  };
}

describe("live calls", () => {
  it("announces the caller and profile, and answers audio only when chosen", () => {
    const p = props();
    render(<CallView {...p} />);
    const region = screen.getByRole("region", { name: "Video call from Maria" });
    expect(within(region).getByRole("status").textContent).toContain("For Work");
    expect(p.onAnswer).not.toHaveBeenCalled();
    fireEvent.click(within(region).getByRole("button", { name: "Answer audio" }));
    expect(p.onAnswer).toHaveBeenCalledWith("audio");
    fireEvent.click(within(region).getByRole("button", { name: "Decline" }));
    expect(p.onEnd).toHaveBeenCalledWith(true);
  });

  it("reads still trying, never a refusal, while the answer's delivery is unknown", () => {
    const connecting: CoreCallView = { ...incoming, state: "connecting", deliveryUnsure: true };
    const { rerender } = render(<CallView {...props()} call={connecting} />);
    expect(screen.getByRole("status").textContent).toContain("Connecting, still trying to reach Maria");
    expect(screen.getByRole("status").textContent).not.toContain("Failed");
    rerender(<CallView {...props()} call={{ ...connecting, deliveryUnsure: false }} />);
    expect(screen.getByRole("status").textContent).toContain("Connecting");
    expect(screen.getByRole("status").textContent).not.toContain("still trying");
  });

  it("offers only decline when its privacy policy needs a relay", () => {
    render(<CallView {...props()} canUseCallRoute={false} />);
    expect(screen.getByRole("status").textContent).toContain(copy.noDirect);
    expect(screen.queryByRole("button", { name: "Answer audio" })).toBeNull();
    expect(screen.getByRole("button", { name: "Decline" })).toBeTruthy();
  });

  it("keeps the incoming announcement and actions with reduced motion", () => {
    const p = props();
    const { container, rerender } = render(<ThemeProvider motion="full"><CallView {...p} /></ThemeProvider>);
    expect(container.querySelector(".om-call__pulse")).toBeTruthy();
    rerender(<ThemeProvider motion="reduced"><CallView {...p} /></ThemeProvider>);
    expect(container.querySelector(".om-call__pulse")).toBeNull();
    expect(screen.getByRole("status").textContent).toContain("Video call from Maria");
    fireEvent.click(screen.getByRole("button", { name: "Answer video" }));
    expect(p.onAnswer).toHaveBeenCalledWith("video");
  });

  it("offers the audio alternative after camera permission fails", () => {
    const p = props();
    render(<CallView {...p} error="The camera is unavailable." />);
    expect(screen.getByRole("alert").textContent).toBe("The camera is unavailable.");
    expect(screen.queryByRole("button", { name: /listen only|open settings/i })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Answer audio" }));
    expect(p.onAnswer).toHaveBeenCalledWith("audio");
  });

  it("keeps mute, camera and call state independent of the transport route", () => {
    const p = props();
    const call = { ...incoming, state: "active", localMediaFlags: 0, route: "direct" } as const;
    const { container, rerender } = render(<CallView {...p} call={call} cameraAvailable />);
    const html = container.innerHTML;
    rerender(<CallView {...p} call={{ ...call, route: "relay" }} cameraAvailable />);
    expect(container.innerHTML).toBe(html);
    expect(screen.getByRole("button", { name: copy.unmute })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: copy.cameraOn }));
    expect(p.onToggle).toHaveBeenCalledWith("video");
  });

  it("offers an explicit retry for a failed call when a permitted route and target exist", () => {
    const p = props();
    const call = { ...incoming, outgoing: true, state: "failed" } as const;
    const retry = vi.fn();
    const rendered = render(<CallView {...p} call={call} onRetry={retry} />);
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(retry).toHaveBeenCalledOnce();
    rendered.rerender(<CallView {...p} call={call} onRetry={retry} canUseCallRoute={false} />);
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  });

  it("has accessible controls and announcements through each Core state", async () => {
    const p = props();
    const rendered = render(<CallView {...p} />);
    for (const state of Object.keys(copy.state) as CoreCallView["state"][]) {
      rendered.rerender(<CallView {...p} call={{ ...incoming, state }} />);
      expect(await axe.run(rendered.container, { rules: { "color-contrast": { enabled: false } } })).toMatchObject({ violations: [] });
      expect(screen.getByRole("status").textContent).toContain(state === "incoming" ? "Video call from Maria" : copy.state[state]);
    }
    rendered.rerender(<CallView {...p} call={{ ...incoming, state: "active", localMediaFlags: 1 }} />);
    fireEvent.click(screen.getByRole("button", { name: "Mute microphone" }));
    expect(p.onToggle).toHaveBeenCalledWith("audio");
    fireEvent.click(screen.getByRole("button", { name: "End call" }));
    expect(p.onEnd).toHaveBeenCalled();
  });

  it("renders a local missed-call timeline item from Core", () => {
    render(<ul><CallTimelineNotice copy={copy} notice={{
      callSessionId: incoming.callSessionId, conversationId: incoming.conversationId,
      reason: "timeout", atMs: 1000, folded: false,
    }} /></ul>);
    expect(screen.getByRole("listitem").textContent).toContain("Missed call");
  });
});
