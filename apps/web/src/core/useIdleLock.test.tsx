// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useIdleLock } from "./useIdleLock";

beforeEach(() => { vi.useFakeTimers(); });
afterEach(() => { cleanup(); vi.useRealTimers(); });

const minutes = (n: number): number => n * 60_000;

describe("useIdleLock", () => {
  it("fires once after the time away, and never when switched off", () => {
    const onIdle = vi.fn();
    renderHook(() => { useIdleLock({ minutes: 5, enabled: true, onIdle }); });
    act(() => { vi.advanceTimersByTime(minutes(4)); });
    expect(onIdle).not.toHaveBeenCalled();
    act(() => { vi.advanceTimersByTime(minutes(2)); });
    expect(onIdle).toHaveBeenCalledTimes(1);
    act(() => { vi.advanceTimersByTime(minutes(30)); });
    expect(onIdle).toHaveBeenCalledTimes(1);

    const off = vi.fn();
    renderHook(() => { useIdleLock({ minutes: null, enabled: true, onIdle: off }); });
    renderHook(() => { useIdleLock({ minutes: 1, enabled: false, onIdle: off }); });
    act(() => { vi.advanceTimersByTime(minutes(10)); });
    expect(off).not.toHaveBeenCalled();
  });

  it("touching the page starts the wait again", () => {
    const onIdle = vi.fn();
    renderHook(() => { useIdleLock({ minutes: 5, enabled: true, onIdle }); });
    for (let i = 0; i < 4; i++) {
      act(() => { vi.advanceTimersByTime(minutes(4)); window.dispatchEvent(new Event("keydown")); });
    }
    expect(onIdle).not.toHaveBeenCalled();
    act(() => { vi.advanceTimersByTime(minutes(6)); });
    expect(onIdle).toHaveBeenCalledTimes(1);
  });

  it("checks the clock when the tab comes back, because timers sleep in the background", () => {
    const onIdle = vi.fn();
    renderHook(() => { useIdleLock({ minutes: 5, enabled: true, onIdle }); });
    vi.setSystemTime(Date.now() + minutes(20)); // the tab slept: no timer ran
    act(() => { document.dispatchEvent(new Event("visibilitychange")); });
    expect(onIdle).toHaveBeenCalledTimes(1);
  });
});
