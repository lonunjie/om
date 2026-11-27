// SPDX-License-Identifier: MPL-2.0
import type { WasmCore } from "@om/core-wasm";
import { createScopeEngines, type ScopeEngines } from "../../../../../packages/om-core-wasm/src/scope-engines";
import { captureProfileContext, type CoreClient, type ProfileExecutionContext } from "@om/platform-api";
import { createPortabilityEngine } from "../../../../../packages/om-core-wasm/src/portability";
import type { BridgeService } from "../../../../../packages/om-ui/src/bridges/bridges";
import type { CommunitiesService } from "../../../../../packages/om-ui/src/communities/communities";
import type { NearbyService } from "../../../../../packages/om-ui/src/conversations/nearby";
import type { PortabilityService } from "../../../../../packages/om-ui/src/portability/portability";
import type { ConnectedServicesService } from "../../../../../packages/om-ui/src/services/services";
import { createConnectedServicesService, type ConnectedServicesOptions } from "./services";
import { createBridgeService, denyAll, type BridgeServiceOptions } from "./bridge";
import { createCommunitiesService, type CommunitiesServiceOptions } from "./communities";
import { createNearbyService, type NearbyServiceOptions } from "./nearby";
import { createCoreHistorySource, createPortabilityService, emptyHistory, type PortabilityServiceOptions } from "./portability";
import { DurablePendingStore, RoomSnapshotStore, roomPersistence, type RoomStorageOptions } from "./room-storage";

export { DurablePendingStore, RoomSnapshotStore, type RoomStorageOptions } from "./room-storage";

export { bridgeRoomHandle, denyAll, type PendingBridgeOperation, type PendingStore, type RoomAuthority } from "./bridge";
export {
  communityChildLinkHandle, communitiesRoomLinkHandle, createCommunitiesService, type CommunitiesRoomDetails, type CommunitiesRoomViews,
  type CommunitiesServiceOptions, type FollowOperation, type FollowRecord, type FollowStore,
} from "./communities";
export { createCoreHistorySource, type FileHost, type HistorySource } from "./portability";
export {
  createConnectedServicesService, serviceRoomHandle, type ConnectedServicesOptions,
  type InvocationSelection, type PendingServiceOperation, type PendingServiceStore, type SelectionSource,
  type ServiceInvocationOptions,
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
  readonly roomStorage?: RoomStorageOptions;
  readonly bridge?: {
    readonly pending?: BridgeServiceOptions["pending"];
    readonly policy?: Parameters<ScopeEngines["bridgeRoom"]["open"]>[1];
  };
  readonly portability?: Omit<PortabilityServiceOptions, "ctx" | "engine">;
  readonly nearby?: Omit<NearbyServiceOptions, "ctx" | "engines">;
  readonly connectedServices?: {
    readonly pending?: ConnectedServicesOptions["pending"];
    readonly policy?: Parameters<ScopeEngines["roomServices"]["open"]>[1];
    readonly selectionSource?: ConnectedServicesOptions["selectionSource"];
  };
  readonly communities?: Omit<CommunitiesServiceOptions, "ctx" | "engines">;
}

/** The engineering entry supplies bindings only after loading the scoped module. */
export function createScopeServices({ ctx: inputCtx, enabled, loaded, client }: {
  readonly ctx: ProfileExecutionContext;
  readonly enabled: boolean;
  readonly loaded?: LoadedScopeServices;
  readonly client?: Pick<CoreClient, "query">;
}): ScopeServices {
  if (!enabled || loaded === undefined) return {};
  const ctx = captureProfileContext(inputCtx.profileId, inputCtx.generation);
  const engines = createScopeEngines(loaded.core, ctx);
  const { roomStorage } = loaded;
  const snapshot = roomStorage === undefined ? undefined : new RoomSnapshotStore(roomStorage.kv);
  const openBridge = loaded.bridge?.policy;
  const openService = loaded.connectedServices?.policy;
  const bridgePending: BridgeServiceOptions["pending"] | undefined = roomStorage === undefined
    ? loaded.bridge?.pending : new DurablePendingStore(roomStorage.kv, "bridge");
  const servicePending: ConnectedServicesOptions["pending"] | undefined = roomStorage === undefined
    ? loaded.connectedServices?.pending : new DurablePendingStore(roomStorage.kv, "service");
  const bridgePersistence = snapshot === undefined || roomStorage === undefined ? {} : {
    persistence: roomPersistence(ctx, engines, snapshot, "bridge", roomStorage.observeCurrentHeads, (handle) =>
      openBridge === undefined ? { refused: "unavailable" } : engines.bridgeRoom.open(handle, openBridge)),
  };
  const servicePersistence = snapshot === undefined || roomStorage === undefined ? {} : {
    persistence: roomPersistence(ctx, engines, snapshot, "service", roomStorage.observeCurrentHeads, (handle) =>
      openService === undefined ? { refused: "unavailable" } : engines.roomServices.open(handle, openService)),
  };
  return {
    ...(loaded.bridge === undefined || bridgePending === undefined ? {} : {
      bridge: createBridgeService({ ctx, engines, authority: denyAll, pending: bridgePending, ...bridgePersistence }),
    }),
    ...(loaded.portability === undefined ? {} : {
      portability: createPortabilityService({
        ...loaded.portability, ctx, engine: createPortabilityEngine(loaded.core),
        history: loaded.portability.history ?? (client === undefined ? emptyHistory : createCoreHistorySource({ ctx, client })),
      }),
    }),
    ...(loaded.nearby === undefined ? {} : { nearby: createNearbyService({ ...loaded.nearby, ctx, engines }) }),
    ...(loaded.connectedServices === undefined || servicePending === undefined ? {} : {
      connectedServices: createConnectedServicesService({
        ...(loaded.connectedServices.selectionSource === undefined ? {} : { selectionSource: loaded.connectedServices.selectionSource }),
        ctx, engines, authority: denyAll, invocation: { core: loaded.core },
        pending: servicePending, ...servicePersistence,
      }),
    }),
    ...(loaded.communities === undefined ? {} : {
      communities: createCommunitiesService({ ...loaded.communities, ctx, engines }),
    }),
  };
}
