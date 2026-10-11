// @vitest-environment jsdom
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { scannerCopy } from "../copy/en";
import { QrCode } from "./QrCode";
import { useQrScanner } from "./Scanner";

HTMLCanvasElement.prototype.getContext = vi.fn().mockReturnValue(null); // jsdom has no canvas: the scanner then reports a camera failure

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

function Host({ onResult }: { readonly onResult: (t: string | null) => void }) {
  const { scanQr, dialog } = useQrScanner(scannerCopy);
  return (
    <>
      <button type="button" onClick={() => { void scanQr().then(onResult); }}>start</button>
      {dialog}
    </>
  );
}

describe("QrCode", () => {
  it("draws SVG modules for a payload and nothing for an oversized one", () => {
    const { container, rerender } = render(<QrCode payload="om:invite/abc123" />);
    expect(container.querySelector("svg path")?.getAttribute("d")?.length).toBeGreaterThan(50);
    rerender(<QrCode payload={"x".repeat(5000)} />);
    expect(container.querySelector("svg")).toBeNull();
  });
});

describe("useQrScanner", () => {
  it("opens a dialog, resolves null on Cancel, and stops every camera track", async () => {
    const stop = vi.fn();
    const getUserMedia = vi.fn().mockResolvedValue({ getTracks: () => [{ stop }] });
    vi.stubGlobal("navigator", { mediaDevices: { getUserMedia } });
    HTMLMediaElement.prototype.play = vi.fn().mockResolvedValue(undefined);
    const onResult = vi.fn();
    render(<Host onResult={onResult} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "start" }));
    expect(await screen.findByRole("dialog", { name: scannerCopy.heading })).toBeTruthy();
    await waitFor(() => { expect(getUserMedia).toHaveBeenCalledTimes(1); });
    await user.click(screen.getByRole("button", { name: scannerCopy.cancel }));
    await waitFor(() => { expect(onResult).toHaveBeenCalledWith(null); });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(stop).toHaveBeenCalled();
  });

  it("shows a plain message when the camera cannot start and still lets the user cancel", async () => {
    vi.stubGlobal("navigator", { mediaDevices: { getUserMedia: vi.fn().mockRejectedValue(new Error("denied")) } });
    const onResult = vi.fn();
    render(<Host onResult={onResult} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "start" }));
    expect((await screen.findByRole("alert")).textContent).toBe(scannerCopy.cameraFailed);
    await act(async () => { await user.keyboard("{Escape}"); });
    await waitFor(() => { expect(onResult).toHaveBeenCalledWith(null); });
  });
});
