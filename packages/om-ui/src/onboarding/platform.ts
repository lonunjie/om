import type { PlatformAdapter } from "@om/platform-api";

/** The only platform surface first run may touch. There is deliberately no contactsLocal: no address-book upload. */
export type OnboardingPlatform = Pick<PlatformAdapter, "capabilities" | "share" | "camera" | "clipboard">;
