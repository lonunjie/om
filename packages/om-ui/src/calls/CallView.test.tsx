// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { asConversationId, type CallView as CoreCallView } from "@om/platform-api";
import { CallTimelineNotice, CallView, type CallCopy } from "./CallView";

afterEach(cleanup);

const copy: CallCopy = {
  audio: "Audio call", video: "Video call", incoming: (name, media) => `${media} from ${name}`,
  withContact: (name) => `Call with ${name}`, identity: (name) => `For ${name}`,
  state: { incoming: "Incoming", starting: "Starting", ringing: "Ringing", connecting: "Connecting", active: "Connected",
    reconnecting: "Reconnecting", ending: "Ending", ended: "Ended", failed: "Failed" },
  reason: { busy: "Busy", declined: "Declined", answered_elsewhere: "Answered elsewhere", timeout: "No answer",
    failed: "Failed", hangup: "Ended", canceled: "Canceled" },
  answerAudio: "Answer audio", answerVideo: "Answer video", decline: "Decline", hangUp: "End call",
  mute: "Mute microphone", unmute: "Unmute microphone", cameraOn: "Turn camera on", cameraOff: "Turn camera off",
  dismiss: "Dismiss", cancel: "Cancel", preparing: "Preparing", noDirect: "A private route is unavailable",
  missed: "Missed call", missedSeveral: "Missed calls", time: () => "12:00",
};
const incoming: CoreCallView = {
  callSessionId: "1", conversationId: asConversationId("1"), media: "video", state: "incoming", outgoing: false,
  ringDeadlineMs: 45_000, reason: null, localMediaFlags: 0, peerMediaFlags: 3, route: null,
};

function props() {
  return {
    copy, call: incoming, name: "Maria", identity: "Work", pending: false, privacyAllowsDirect: true,
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

  it("offers only decline when its privacy policy needs a relay", () => {
    render(<CallView {...props()} privacyAllowsDirect={false} />);
    expect(screen.getByRole("status").textContent).toContain(copy.noDirect);
    expect(screen.queryByRole("button", { name: "Answer audio" })).toBeNull();
    expect(screen.getByRole("button", { name: "Decline" })).toBeTruthy();
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
