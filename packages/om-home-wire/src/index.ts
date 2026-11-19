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
export * from "./usage";
export * from "./recovery";
export * from "./push";
export * from "./media";
import { USAGE_FEATURE } from "./usage";
import { RECOVERY_FEATURE } from "./recovery";
import { PUSH_FEATURE } from "./push";
import { MEDIA_FEATURE } from "./media";
export { Obj } from "./fields";

/** Optional-feature ids a Home advertises in ProviderCapabilities.features (home-api.md section 1). */
export const FEATURES = {
  relayQueues: "om.relay.queues.v1",
  relayWake: "om.relay.wake.v1",
  /** A dedicated queue can be retired by its admin capability and is released after a tombstone horizon (ADR-0229 section 3). */
  relayQueueRetire: "om.relay.queue-retire.v1",
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
  /** The usage read of the personal limits profile (admin capability; see usage.ts). */
  relayUsage: USAGE_FEATURE,
  /** RecoveryRecord store, recovery inbox and veto board (ADR-0201); offered only where the Home has owner admission. */
  recoveryStore: RECOVERY_FEATURE,
  /** Optional push wake: an empty, signed push to a browser endpoint the owner registered, only when nobody is listening (ADR-0210). Offered only where the Home has owner admission. */
  relayPush: PUSH_FEATURE,
  mediaTurn: MEDIA_FEATURE,
} as const;
