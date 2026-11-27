// SPDX-License-Identifier: MPL-2.0
import { captureProfileContext, type PlatformAdapter, type ProfileExecutionContext } from "@om/platform-api";
import type { NearbyService } from "../../../../../packages/om-ui/src/conversations/nearby";
import type { ScopeEngines } from "../../../../../packages/om-core-wasm/src/scope-engines";

export interface NearbyServiceOptions {
  readonly ctx: ProfileExecutionContext;
  readonly engines: Pick<ScopeEngines, "lanNearby">;
  /** The host binds its probe policy to this profile before supplying the platform. */
  readonly platform: Pick<PlatformAdapter, "localDelivery">;
}

export function createNearbyService({ ctx: inputCtx, engines, platform }: NearbyServiceOptions): NearbyService {
  const ctx = captureProfileContext(inputCtx.profileId, inputCtx.generation);
  let probed: "available" | "unavailable" | undefined;

  function capabilityState(): NearbyService["platform"] {
    const { state } = platform.localDelivery();
    return probed === "unavailable" ? "unavailable" : state;
  }

  return {
    get platform() { return capabilityState(); },
    async state(conversationId) {
      const view = engines.lanNearby.state(ctx, conversationId);
      const capability = platform.localDelivery();
      // The contact setting is the first read. The platform owns the protection policy and probe caching.
      if (view.available && capability.state !== "unavailable") {
        try {
          probed = await capability.api.probe();
        } catch {
          probed = "unavailable";
        }
        return engines.lanNearby.state(ctx, conversationId);
      }
      return view;
    },
    set: (conversationId, on) => Promise.resolve().then(() => {
      if (on && capabilityState() === "unavailable") return engines.lanNearby.state(ctx, conversationId);
      return engines.lanNearby.set(ctx, conversationId, on);
    }),
  };
}
