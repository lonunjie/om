import { useEffect, useRef } from "react";

const CHECK_MS = 5_000;
const ACTIVITY = ["pointerdown", "pointermove", "keydown", "wheel", "touchstart", "scroll"] as const;

/**
 * Calls `onIdle` once when `minutes` have passed without the person touching the page. Timers sleep in a background tab, so the
 * check compares clock times (and runs again the moment the tab is shown) instead of trusting one long timeout.
 */
export function useIdleLock({ minutes, enabled, onIdle }: { readonly minutes: number | null; readonly enabled: boolean; readonly onIdle: () => void }): void {
  const callback = useRef(onIdle);
  callback.current = onIdle;
  useEffect(() => {
    if (!enabled || minutes === null) return;
    const limit = minutes * 60_000;
    let last = Date.now();
    let fired = false;
    const touch = (): void => { last = Date.now(); };
    const check = (): void => {
      if (fired || Date.now() - last < limit) return;
      fired = true;
      callback.current();
    };
    const onVisible = (): void => { if (document.visibilityState === "visible") check(); };
    for (const type of ACTIVITY) globalThis.addEventListener(type, touch, { passive: true, capture: true });
    document.addEventListener("visibilitychange", onVisible);
    const timer = setInterval(check, CHECK_MS);
    return () => {
      for (const type of ACTIVITY) globalThis.removeEventListener(type, touch, { capture: true });
      document.removeEventListener("visibilitychange", onVisible);
      clearInterval(timer);
    };
  }, [minutes, enabled]);
}
