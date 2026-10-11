import { useSyncExternalStore } from "react";

// Subscribes to a media query; false where matchMedia is unavailable (fail to the compact/no-preference baseline).
export const useMediaQuery = (query: string): boolean => {
  const subscribe = (notify: () => void): (() => void) => {
    if (typeof window.matchMedia !== "function") return () => undefined;
    const mql = window.matchMedia(query);
    mql.addEventListener("change", notify);
    return () => { mql.removeEventListener("change", notify); };
  };
  const snapshot = (): boolean => typeof window.matchMedia === "function" && window.matchMedia(query).matches;
  return useSyncExternalStore(subscribe, snapshot, () => false);
};
