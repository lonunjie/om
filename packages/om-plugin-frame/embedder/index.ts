// SPDX-License-Identifier: MPL-2.0
import config from "../frame/config.json";
import { mountSiteFrame } from "./broker";
import type { FrameOptions } from "./broker";

export type { FrameOptions, Observation } from "./broker";

/** Explicit engineering opt-in (off by default). The frame is cross-site and bound to its exact origin. */
export function mountPluginFrame(container: HTMLElement, options: FrameOptions & { readonly enabled?: boolean }) {
  if (options.enabled !== true) return null;
  if (!config.appOrigins.includes(location.origin)) throw new Error("Unconfigured OM app origin");
  return mountSiteFrame(container, config.frameOrigin, options);
}
