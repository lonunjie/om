// SPDX-License-Identifier: MPL-2.0
import type { WasmCore } from "@om/core-wasm";
import { createScopeEngines } from "../../../../../packages/om-core-wasm/src/scope-engines";
import { captureProfileContext, type ProfileExecutionContext } from "@om/platform-api";
import { createPortabilityEngine } from "../../../../../packages/om-core-wasm/src/portability";
import type { BridgeService } from "../../../../../packages/om-ui/src/bridges/bridges";
import type { CommunitiesService } from "../../../../../packages/om-ui/src/communities/communities";
import type { NearbyService } from "../../../../../packages/om-ui/src/conversations/nearby";
import type { PortabilityService } from "../../../../../packages/om-ui/src/portability/portability";
import type { ConnectedServicesService } from "../../../../../packages/om-ui/src/services/services";
import { createConnectedServicesService, type ConnectedServicesOptions } from "./services";
import { createBridgeService, type BridgeServiceOptions } from "./bridge";
import { createCommunitiesService, type CommunitiesServiceOptions } from "./communities";
import { createNearbyService, type NearbyServiceOptions } from "./nearby";
import { createPortabilityService, type PortabilityServiceOptions } from "./portability";

export { bridgeRoomHandle, denyAll, type PendingBridgeOperation, type PendingStore, type RoomAuthority } from "./bridge";
export {
  communityChildLinkHandle, communitiesRoomLinkHandle, createCommunitiesService, type CommunitiesRoomDetails, type CommunitiesRoomViews,
  type CommunitiesServiceOptions, type FollowOperation, type FollowRecord, type FollowStore,
} from "./communities";
export type { FileHost, HistorySource } from "./portability";
export {
  createConnectedServicesService, serviceRoomHandle, type ConnectedServicesOptions,
  type PendingServiceOperation, type PendingServiceStore,
} from "./services";

export interface ScopeServices {
  readonly bridge?: BridgeService;
  readonly portability?: PortabilityService;
  readonly nearby?: NearbyService;
  readonly connectedServices?: ConnectedServicesService;
  readonly communities?: CommunitiesService;
}

export interface LoadedScopeServices {
  readonly core: Pick<WasmCore, "call">;
  readonly bridge?: Omit<BridgeServiceOptions, "ctx" | "engines">;
  readonly portability?: Omit<PortabilityServiceOptions, "ctx" | "engine">;
  readonly nearby?: Omit<NearbyServiceOptions, "ctx" | "engines">;
  readonly connectedServices?: Omit<ConnectedServicesOptions, "ctx" | "engines">;
  readonly communities?: Omit<CommunitiesServiceOptions, "ctx" | "engines">;
}

/** The engineering entry supplies bindings only after loading the scoped module. */
export function createScopeServices({ ctx: inputCtx, enabled, loaded }: {
  readonly ctx: ProfileExecutionContext;
  readonly enabled: boolean;
  readonly loaded?: LoadedScopeServices;
}): ScopeServices {
  if (!enabled || loaded === undefined) return {};
  const ctx = captureProfileContext(inputCtx.profileId, inputCtx.generation);
  const engines = createScopeEngines(loaded.core);
  return {
    ...(loaded.bridge === undefined ? {} : { bridge: createBridgeService({ ...loaded.bridge, ctx, engines }) }),
    ...(loaded.portability === undefined ? {} : {
      portability: createPortabilityService({ ...loaded.portability, ctx, engine: createPortabilityEngine(loaded.core) }),
    }),
    ...(loaded.nearby === undefined ? {} : { nearby: createNearbyService({ ...loaded.nearby, ctx, engines }) }),
    ...(loaded.connectedServices === undefined ? {} : {
      connectedServices: createConnectedServicesService({ ...loaded.connectedServices, ctx, engines }),
    }),
    ...(loaded.communities === undefined ? {} : {
      communities: createCommunitiesService({ ...loaded.communities, ctx, engines }),
    }),
  };
}
