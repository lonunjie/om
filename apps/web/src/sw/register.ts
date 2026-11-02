// Page-side registration: feature-detected, a no-op when unsupported or in dev.
export interface RegisterOptions {
  /** Called when a verified bundle is staged. Invoke `apply` only at a user-visible or idle boundary. */
  readonly onUpdateReady?: (apply: () => void) => void;
}

const SW_URL = "/sw.js";

const scriptUrl = (): string | null => {
  const tt = (globalThis as { trustedTypes?: { createPolicy(n: string, p: { createScriptURL(u: string): string }): { createScriptURL(u: string): unknown } } }).trustedTypes;
  if (!tt) return SW_URL;
  try {
    const policy = tt.createPolicy("om-bundle", {
      createScriptURL: (u) => {
        if (u !== SW_URL) throw new Error("untrusted script url");
        return u;
      },
    });
    return policy.createScriptURL(SW_URL) as string;
  } catch {
    return null; // policy refused: fail closed, run without a worker
  }
};

const IDLE_MS = 20_000;
const RELOAD_KEY = "om.sw/reloaded-at";
/** Two takeovers inside this window would be a reload loop: the second one is skipped (the page runs the bundle it has). */
const RELOAD_GAP_MS = 60_000;

/** Something typed and not yet sent or saved (a name, a draft): a reload would lose it, so an update waits for a calmer moment. */
const hasUnsavedInput = (): boolean => {
  for (const el of document.querySelectorAll("input, textarea")) {
    if (!(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) continue;
    if (el instanceof HTMLInputElement && ["hidden", "checkbox", "radio", "button", "submit"].includes(el.type)) continue;
    if (el.value !== "") return true;
  }
  return false;
};
const CHECK_MS = 60 * 60 * 1000;

/** Apply a staged update without asking: as soon as the page is hidden, or after IDLE_MS with no input. The new worker takes
 *  over, then the page reloads (see `reloadOnTakeover`). Returns a stop function. */
export const applyWhenIdle = (apply: () => void): (() => void) => {
  let last = Date.now();
  const touch = (): void => { last = Date.now(); };
  const events = ["pointerdown", "keydown", "touchstart", "scroll"] as const;
  for (const e of events) addEventListener(e, touch, { passive: true });
  const tick = (): void => {
    if (hasUnsavedInput()) return;
    if (document.visibilityState === "hidden" || Date.now() - last >= IDLE_MS) { stop(); apply(); }
  };
  const timer = setInterval(tick, 2000);
  document.addEventListener("visibilitychange", tick);
  const stop = (): void => {
    clearInterval(timer);
    document.removeEventListener("visibilitychange", tick);
    for (const e of events) removeEventListener(e, touch);
  };
  tick();
  return stop;
};

/** A new worker took over a page that was already controlled: reload once so the page runs the bundle it now serves. */
const reloadOnTakeover = (): void => {
  if (!navigator.serviceWorker.controller) return;
  let done = false;
  navigator.serviceWorker.addEventListener("controllerchange", () => {
    if (done) return;
    done = true;
    try {
      const at = Number(sessionStorage.getItem(RELOAD_KEY) ?? "0");
      if (Date.now() - at < RELOAD_GAP_MS) return;
      sessionStorage.setItem(RELOAD_KEY, String(Date.now()));
    } catch { /* no session storage: reload once, as before */ }
    location.reload();
  });
};

export const registerServiceWorker = async (opts: RegisterOptions = {}): Promise<void> => {
  if (import.meta.env.DEV || !("serviceWorker" in navigator)) return;
  const url = scriptUrl();
  if (url === null) return;
  try {
    const reg = await navigator.serviceWorker.register(url, { scope: "/", updateViaCache: "none" });
    const offer = (): void => {
      const waiting = reg.waiting;
      if (!waiting || !navigator.serviceWorker.controller) return;
      opts.onUpdateReady?.(() => {
        waiting.postMessage({ type: "om.sw/activate" });
      });
    };
    reloadOnTakeover();
    // A long-lived installed copy looks for a new release on its own, not only at a cold start.
    const check = (): void => { void reg.update().catch(() => undefined); };
    setInterval(check, CHECK_MS);
    document.addEventListener("visibilitychange", () => { if (document.visibilityState === "visible") check(); });
    offer();
    reg.addEventListener("updatefound", () => {
      reg.installing?.addEventListener("statechange", offer);
    });
  } catch {
    // Registration failure must never affect the app: the shell works without a worker.
  }
};
