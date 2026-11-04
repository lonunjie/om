// SPDX-License-Identifier: MPL-2.0
export * from "./cbor";
export * from "./bytes";
export * from "./version";
export * from "./errors";
export * from "./bodies";
export * from "./route";
export * from "./sealed";
export * from "./bundle";
export * from "./blob";
export { Obj } from "./fields";

/** Optional-feature ids a Home advertises in ProviderCapabilities.features (home-api.md section 1). */
export const FEATURES = {
  relayQueues: "om.relay.queues.v1",
  relayWake: "om.relay.wake.v1",
  /** The Home lets only its owner create resources (queues, descriptors, key pools, intro services); see `admissionHash`. */
  admission: "om.admission.v1",
  keysPool: "om.keys.pool.v1",
  keysAnonymousFetch: "om.keys.fetch-throttled.v1",
  /** Fetching a device's bundle needs a capability its owner granted for one relationship (D4). */
  keysRelationshipCaps: "om.keys.relationship-caps.v1",
  discoveryPut: "om.discovery.put.v1",
  discoveryDelete: "om.discovery.delete.v1",
  introGate: "om.intro.v1",
  /** A bounded encrypted blob spool (put, get, delete by capability); offered only where the Home has owner admission (docs/reports/2026-11-04-attachments-home-status.md). */
  blobSpool: "om.blob.spool.v1",
} as const;
