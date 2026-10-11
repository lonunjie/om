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
    offer();
    reg.addEventListener("updatefound", () => {
      reg.installing?.addEventListener("statechange", offer);
    });
  } catch {
    // Registration failure must never affect the app: the shell works without a worker.
  }
};
