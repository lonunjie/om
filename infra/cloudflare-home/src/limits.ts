// SPDX-License-Identifier: MPL-2.0
// Operational limits of this Home. They are implementation parameters, never protocol constants (CLOUDFLARE_REFERENCE_DEPLOYMENT.md);
// they are advertised in ProviderCapabilities.limits and every one fails with a typed `quota`, `payload_too_large` or `rate_limited`.

export const DAY = 86_400;

export const LIMITS = {
  /** Largest SealedDeliveryEnvelope: 131088-byte ciphertext, 1120-byte HPKE enc and a few dozen bytes of framing. */
  maxDepositBytes: 133_000,
  maxQueueItems: 512,
  maxQueueBytes: 16 * 1024 * 1024,
  /** Retention bound; a sealed wrapper's own authenticated expiry may be shorter, never longer (SEALED_SENDER_DELIVERY_PROFILE section 7). */
  maxRetentionSeconds: 14 * DAY,
  defaultRetentionSeconds: 7 * DAY,
  /** spec/registries/om-time-fields.json default skew. */
  skewSeconds: 300,
  /** Per deposit capability token bucket. */
  depositBurst: 30,
  depositRefillPerSecond: 1,
  drainPageItems: 64,
  drainPageBytes: 1024 * 1024,
  maxDepositCapabilities: 64,
  /** request_id idempotency horizon across restarts (home-api.md section 5). */
  idempotencySeconds: 7 * DAY,
  rotateOverlapSeconds: DAY,
  rotateResultSeconds: 600,
  /** Key pool. */
  maxOneTimePerKind: 100,
  maxBundleBytes: 65_536,
  fetchBurst: 8,
  fetchRefillSeconds: 600,
  /** Descriptor store. */
  maxDescriptorBytes: 32_768,
  maxDescriptorSeconds: 30 * DAY,
  descriptorReadBurst: 20,
  descriptorReadRefillSeconds: 5,
  /** IntroGate. */
  maxTicketsPerService: 256,
  maxTicketUses: 16,
  maxTicketSeconds: 14 * DAY,
  maxTicketPayload: 65_536,
  maxPendingRequests: 256,
  submitBurst: 20,
  submitRefillPerSecond: 2,
} as const;
