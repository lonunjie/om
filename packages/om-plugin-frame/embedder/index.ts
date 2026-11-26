// SPDX-License-Identifier: MPL-2.0
import config from "../frame/config.json";
import { mountOpaqueFrame } from "./broker";
import type { FrameOptions } from "./broker";

export type { FrameOptions, Observation } from "./broker";

/** Explicit engineering opt-in. The opaque-origin iframe cannot obtain an exact-origin port. */
export function mountPluginFrame(container: HTMLElement, options: FrameOptions & { readonly enabled?: boolean }) {
  if (options.enabled !== true) return null;
  if (!config.appOrigins.includes(location.origin)) throw new Error("Unconfigured OM app origin");
  return mountOpaqueFrame(container, config.frameOrigin, options);
}
