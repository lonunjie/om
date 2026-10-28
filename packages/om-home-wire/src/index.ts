// SPDX-License-Identifier: MPL-2.0
export * from "./cbor";
export * from "./bytes";
export * from "./version";
export * from "./errors";
export * from "./bodies";
export * from "./route";
export * from "./sealed";
export * from "./bundle";
export { Obj } from "./fields";

/** Optional-feature ids a Home advertises in ProviderCapabilities.features (home-api.md section 1). */
export const FEATURES = {
  relayQueues: "om.relay.queues.v1",
  keysPool: "om.keys.pool.v1",
  keysAnonymousFetch: "om.keys.fetch-throttled.v1",
  discoveryPut: "om.discovery.put.v1",
  discoveryDelete: "om.discovery.delete.v1",
  introGate: "om.intro.v1",
} as const;
