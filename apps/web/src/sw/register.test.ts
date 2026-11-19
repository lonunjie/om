// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { addBusyProbe, sessionBusy } from "./busy";
import { applyWhenIdle } from "./register";

const setVisibility = (state: "visible" | "hidden"): void => {
  Object.defineProperty(document, "visibilityState", { configurable: true, get: () => state });
  document.dispatchEvent(new Event("visibilitychange"));
};

beforeEach(() => { vi.useFakeTimers(); setVisibility("visible"); });
afterEach(() => { vi.useRealTimers(); setVisibility("visible"); });

describe("staged update activation", () => {
  it("applies after the idle time when nothing is in progress, and at once when the page is hidden", () => {
    const apply = vi.fn();
    const stop = applyWhenIdle(apply);
    vi.advanceTimersByTime(10_000);
    expect(apply).not.toHaveBeenCalled();
    vi.advanceTimersByTime(12_000);
    expect(apply).toHaveBeenCalledTimes(1);
    stop();
    const hidden = vi.fn();
    applyWhenIdle(hidden);
    setVisibility("hidden");
    expect(hidden).toHaveBeenCalledTimes(1);
  });

  it("never applies while a call is in progress, however idle the keyboard or hidden the tab, and applies once it ends", () => {
    let inCall = true;
    const off = addBusyProbe(() => inCall);
    const apply = vi.fn();
    applyWhenIdle(apply);
    vi.advanceTimersByTime(60_000);
    setVisibility("hidden");
    vi.advanceTimersByTime(60_000);
    expect(apply).not.toHaveBeenCalled();
    inCall = false;
    vi.advanceTimersByTime(2_000);
    expect(apply).toHaveBeenCalledTimes(1);
    off();
  });

  it("waits while a probe reports unsaved work and ignores a probe that has been removed", () => {
    const off = addBusyProbe(() => true);
    expect(sessionBusy()).toBe(true);
    off();
    expect(sessionBusy()).toBe(false);
    const apply = vi.fn();
    applyWhenIdle(apply);
    setVisibility("hidden");
    expect(apply).toHaveBeenCalledTimes(1);
  });

  it("treats a probe that throws as busy", () => {
    const off = addBusyProbe(() => { throw new Error("broken"); });
    expect(sessionBusy()).toBe(true);
    off();
  });
});
