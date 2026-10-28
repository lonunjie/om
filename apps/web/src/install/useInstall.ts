import { useCallback, useEffect, useState } from "react";

/** Chromium's install prompt event (not in lib.dom). */
interface BeforeInstallPromptEvent extends Event {
  prompt: () => Promise<void>;
  readonly userChoice: Promise<{ readonly outcome: "accepted" | "dismissed" }>;
}

export type InstallState =
  | { readonly kind: "installed" }
  | { readonly kind: "promptable"; readonly install: () => void }
  | { readonly kind: "ios-manual" }
  | { readonly kind: "none" };

const isStandalone = (): boolean => {
  const nav = navigator as Navigator & { standalone?: boolean };
  return window.matchMedia("(display-mode: standalone)").matches || nav.standalone === true;
};

// iPadOS 13+ reports a Mac user agent; touch points tell them apart.
const isIos = (): boolean =>
  /iphone|ipad|ipod/i.test(navigator.userAgent) || (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1);

/** Where the browser supports it, hands out a one-shot install action; on iOS Safari there is none, so the caller shows a hint. */
export function useInstall(): InstallState {
  const [event, setEvent] = useState<BeforeInstallPromptEvent | null>(null);
  const [installed, setInstalled] = useState<boolean>(() => isStandalone());
  useEffect(() => {
    const onPrompt = (e: Event): void => { e.preventDefault(); setEvent(e as BeforeInstallPromptEvent); };
    const onInstalled = (): void => { setInstalled(true); setEvent(null); };
    window.addEventListener("beforeinstallprompt", onPrompt);
    window.addEventListener("appinstalled", onInstalled);
    return () => {
      window.removeEventListener("beforeinstallprompt", onPrompt);
      window.removeEventListener("appinstalled", onInstalled);
    };
  }, []);
  const install = useCallback(() => {
    if (event === null) return;
    setEvent(null); // the event can be used once
    void event.prompt().catch(() => undefined);
  }, [event]);
  if (installed) return { kind: "installed" };
  if (event !== null) return { kind: "promptable", install };
  if (isIos()) return { kind: "ios-manual" };
  return { kind: "none" };
}
