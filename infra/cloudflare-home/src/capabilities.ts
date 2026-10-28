// SPDX-License-Identifier: MPL-2.0
// GET /.well-known/om-provider-capabilities (spec/protocol/provider-capabilities.logical.schema.json). Public, no secrets,
// no per-user data: the same document for every caller.
import { FEATURES } from "@om/home-wire";
import type { Env } from "./env";
import { LIMITS } from "./limits";

export function capabilitiesDocument(env: Env): Record<string, unknown> {
  return {
    schema_version: 1,
    provider_id: env.OM_PROVIDER_ID ?? "om-reference-home",
    provider_kinds: ["relay", "key_material", "discovery_relay", "intro_gate"],
    durability_class: "durable",
    features: Object.values(FEATURES).sort(),
    api_versions: ["1"],
    health: "healthy",
    limits: {
      max_deposit_bytes: LIMITS.maxDepositBytes,
      max_queue_items: LIMITS.maxQueueItems,
      max_queue_bytes: LIMITS.maxQueueBytes,
      max_retention_seconds: LIMITS.maxRetentionSeconds,
      drain_page_items: LIMITS.drainPageItems,
      idempotency_horizon_seconds: LIMITS.idempotencySeconds,
      rotate_overlap_seconds: LIMITS.rotateOverlapSeconds,
      max_deposit_capabilities: LIMITS.maxDepositCapabilities,
      max_descriptor_bytes: LIMITS.maxDescriptorBytes,
      max_ticket_uses: LIMITS.maxTicketUses,
    },
    abuse_controls: {
      per_capability_deposit_rate: { burst: LIMITS.depositBurst, per_second: LIMITS.depositRefillPerSecond },
      one_time_prekey_fetch_throttle: { burst: LIMITS.fetchBurst, refill_seconds: LIMITS.fetchRefillSeconds },
      introgate_submit_rate: { burst: LIMITS.submitBurst, per_second: LIMITS.submitRefillPerSecond },
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
    cost_state: { hard_free_tier_limits: env.OM_HARD_FREE_TIER === "1" },
  };
}
