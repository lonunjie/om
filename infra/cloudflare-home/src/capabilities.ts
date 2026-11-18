// SPDX-License-Identifier: AGPL-3.0-or-later
// GET /.well-known/om-provider-capabilities (spec/protocol/provider-capabilities.logical.schema.json). Public, no secrets,
// no per-user data: the same document for every caller.
import { FEATURES, PUSH_LIMITS, WAKE_AUTH_SECONDS } from "@om/home-wire";
import type { Env } from "./env";
import { LIMITS, limitsOf, profileOf } from "./limits";
import { pushPacing } from "./push";
import { usageOn } from "./usage";

/** The optional wake channel is on unless the deployment sets OM_WAKE=0. */
export const wakeOn = (env: Env): boolean => env.OM_WAKE !== "0";

/**
 * The blob spool is offered only where all three hold: the deployment bound it, the owner did not switch it off, and the Home has
 * owner admission (creating spool space is an owner act, like creating a queue). An open Home therefore never becomes free storage.
 */
export const blobsOn = (env: Env): boolean => env.BLOBS !== undefined && env.OM_BLOB_SPOOL !== "0" && admissionHashes(env).length > 0;

/**
 * The RecoveryStore (om.recovery.store.v1, ADR-0201) is offered only where all three hold: the deployment bound it, the owner did not
 * switch it off, and the Home has owner admission (creating a record or an inbox is an owner act), so an open Home never becomes free
 * storage and a stranger cannot squat a locator on a personal Home.
 */
export const recoveryOn = (env: Env): boolean => env.RECOVERY !== undefined && env.OM_RECOVERY_STORE !== "0" && admissionHashes(env).length > 0;

/**
 * The push wake (om.relay.push.v1, ADR-0210) is offered only where all three hold: the deployment bound it, the owner did not switch it off,
 * and the Home has owner admission (registering an address this Home will call is an owner act), so an open Home never becomes a free push sender.
 */
export const pushOn = (env: Env): boolean => env.PUSH !== undefined && env.OM_PUSH !== "0" && admissionHashes(env).length > 0;

/** The hashes of the admission secrets this deployment accepts; empty and not `admissionConfigured` means resource creation is open (a shared or test Home). */
export const admissionHashes = (env: Env): readonly string[] =>
  (env.OM_ADMISSION ?? "").split(",").map((h) => h.trim().toLowerCase()).filter((h) => /^[0-9a-f]{64}$/.test(h));

/**
 * True when the deployment was given an admission value at all, or declares one required (`OM_ADMISSION_REQUIRED=1`, set by the per-user template so an empty prompt cannot open the relay). A value that yields no valid hash (a typo, a placeholder) must close creation, never open it:
 * "set but unusable" is a misconfiguration, not "no admission wanted" (per-user template, 2026-11-03).
 */
export const admissionConfigured = (env: Env): boolean => (env.OM_ADMISSION ?? "").trim() !== "" || env.OM_ADMISSION_REQUIRED === "1";

export function capabilitiesDocument(env: Env, recoveryScopeId?: string): Record<string, unknown> {
  const L = limitsOf(env);
  return {
    schema_version: 1,
    provider_id: env.OM_PROVIDER_ID ?? "om-reference-home",
    provider_kinds: ["relay", "key_material", "discovery_relay", "intro_gate", ...(blobsOn(env) ? ["blob"] : []), ...(recoveryOn(env) ? ["recovery"] : [])],
    durability_class: "durable",
    features: Object.values(FEATURES).filter((f) => (f !== FEATURES.relayWake || wakeOn(env)) && (f !== FEATURES.admission || admissionHashes(env).length > 0) && (f !== FEATURES.blobSpool || blobsOn(env)) && (f !== FEATURES.relayPush || pushOn(env)) && (f !== FEATURES.relayUsage || usageOn(env)) && (f !== FEATURES.recoveryStore || recoveryOn(env))).sort(),
    api_versions: ["1"],
    health: "healthy",
    ...(recoveryOn(env) && recoveryScopeId !== undefined ? { recovery_scope_id: recoveryScopeId } : {}),
    limits: {
      max_deposit_bytes: LIMITS.maxDepositBytes,
      max_queue_items: L.maxQueueItems,
      max_queue_bytes: L.maxQueueBytes,
      max_retention_seconds: LIMITS.maxRetentionSeconds,
      drain_page_items: LIMITS.drainPageItems,
      idempotency_horizon_seconds: LIMITS.idempotencySeconds,
      rotate_overlap_seconds: LIMITS.rotateOverlapSeconds,
      max_deposit_capabilities: L.maxDepositCapabilities,
      max_descriptor_bytes: LIMITS.maxDescriptorBytes,
      max_ticket_uses: LIMITS.maxTicketUses,
      ...(blobsOn(env) ? { max_blob_spool_bytes: L.maxBlobSpoolBytes, max_blob_bytes: LIMITS.maxBlobBytes, max_blobs: L.maxBlobs, blob_default_retention_seconds: LIMITS.defaultRetentionSeconds } : {}),
      ...(recoveryOn(env) ? {
        max_recovery_record_bytes: L.maxRecoveryRecordBytes, max_recovery_records: L.maxRecoveryRecords, max_recovery_inboxes: L.maxRecoveryInboxes,
        recovery_put_rate: { burst: L.recoveryPutBurst, refill_seconds: L.recoveryPutRefillSeconds }, recovery_get_rate: { burst: L.recoveryGetBurst, refill_seconds: L.recoveryGetRefillSeconds },
        recovery_failed_get_rate: { burst: L.recoveryFailedGetBurst, refill_seconds: L.recoveryFailedGetRefillSeconds },
        recovery_requests_per_day: L.recoveryRequestsPerDay, recovery_pending_requests: L.recoveryPendingRequests,
        recovery_pending_vetoes: L.recoveryPendingVetoes, recovery_pending_vetoes_per_request: L.recoveryPendingVetoesPerDigest, recovery_vetoes_per_hour: L.recoveryVetoesPerHour,
        recovery_post_rate: { burst: L.recoveryPostBurst, refill_seconds: L.recoveryPostRefillSeconds },
        recovery_drain_rate: { burst: L.recoveryDrainBurst, refill_seconds: L.recoveryDrainRefillSeconds },
        recovery_veto_read_rate: { burst: L.recoveryVetoReadBurst, refill_seconds: L.recoveryVetoReadRefillSeconds },
        max_recovery_object_bytes: L.maxRecoveryObjectBytes, recovery_item_retention_seconds: L.maxRetentionSeconds,
        recovery_record_retention_seconds: L.recoveryRecordRetentionSeconds, recovery_inbox_retention_seconds: L.recoveryInboxRetentionSeconds,
      } : {}),
      ...(pushOn(env) ? { push_subscriptions_per_queue: PUSH_LIMITS.perQueue, push_subscriptions_per_home: PUSH_LIMITS.perHome, push_lifetime_seconds: PUSH_LIMITS.lifetimeSeconds, push_min_interval_seconds: pushPacing(env).minInterval, push_per_day: pushPacing(env).perDay } : {}),
      ...(wakeOn(env) ? { wake_sockets_per_queue: LIMITS.maxWakeSockets, wake_auth_seconds: WAKE_AUTH_SECONDS } : {}),
    },
    abuse_controls: {
      per_capability_deposit_rate: { burst: LIMITS.depositBurst, per_second: LIMITS.depositRefillPerSecond },
      one_time_prekey_fetch_throttle: { burst: LIMITS.fetchBurst, refill_seconds: LIMITS.fetchRefillSeconds },
      introgate_submit_rate: { burst: LIMITS.submitBurst, per_second: LIMITS.submitRefillPerSecond },
      ...(blobsOn(env) ? { blob_put: "needs owner admission; the ciphertext digest is verified; bytes and count are bounded; the read capability is minted by the Home" } : {}),
      object_shape_check: "deposits must be a SealedDeliveryEnvelope or a MessageRequest",
      replay: "exact-retry idempotent; a different object under the same (delivery_key_id, delivery_id) is rejected",
    },
    privacy_properties: {
      logs: "none: the Home keeps no request log and reads no network address",
      stores: "capability hashes, sealed ciphertext, signed public key material, opaque descriptors",
      never_stores: ["plaintext", "sender identity", "network address", "AccountID"],
    },
    security_properties: {
      authority: "none: a stored object is never account, device or relationship authority",
      bearer_capabilities: "carried in the request body or Authorization header only, never in a URL, cookie or log",
    },
    cost_state: { hard_free_tier_limits: env.OM_HARD_FREE_TIER === "1", ...(profileOf(env) === "personal" ? { limits_profile: "personal" } : {}) },
  };
}
