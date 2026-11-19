// SPDX-License-Identifier: MPL-2.0
import { MEDIA_FEATURE, MEDIA_TURN_PATH, MEDIA_MAX_REPLY_BYTES, parseMediaCapabilities, decodeTurnLease, encodeTurnRequest } from "@om/home-wire";
import type { CallMediaProvider, TurnResult } from "./provider";
import { exchange, type HttpOptions } from "./http";

/** After an unknown result the same request id is retried at the same Home at 1, 2 and 4 seconds (ADR-0228 s4). */
export const TURN_RETRY_MS: readonly number[] = [1000, 2000, 4000];

const pause = (ms: number): Promise<void> => new Promise((resolve) => { setTimeout(resolve, ms); });

export function createMediaClient(base: string, http: HttpOptions, wait: (ms: number) => Promise<void> = pause): CallMediaProvider {
  let advertised = false;
  const once = async (body: Uint8Array): Promise<TurnResult> => {
    const r = await exchange({ ...http, timeoutMs: Math.min(http.timeoutMs, 5000) }, `${base}${MEDIA_TURN_PATH}`, {
      method: "POST", body, maxReplyBytes: MEDIA_MAX_REPLY_BYTES,
    });
    if (r.kind === "network" || r.kind === "gateway") return { kind: "unknown" };
    if (r.kind === "home") {
      // Only an explicit retryable answer is unknown; quota, disabled, unauthorized and the rest are definite refusals.
      return { kind: r.error.name === "expired" ? "expired" : r.error.name === "retryable" ? "unknown" : "refused" };
    }
    try {
      const lease = decodeTurnLease(r.body, Math.floor(Date.now() / 1000));
      return lease === undefined ? { kind: "expired" } : { kind: "issued", lease };
    } catch { return { kind: "unknown" }; }
  };
  return {
    async mediaCapabilities() {
      if (!http.admission) return undefined;
      const r = await exchange({ ...http, timeoutMs: Math.min(http.timeoutMs, 5000) }, `${base}/.well-known/om-provider-capabilities`, {
        method: "GET", maxReplyBytes: 32_768,
      });
      const media = (() => {
        if (r.kind !== "ok") return undefined;
        try {
          const doc = JSON.parse(new TextDecoder().decode(r.body)) as { features?: unknown; media?: unknown };
          return Array.isArray(doc.features) && doc.features.includes(MEDIA_FEATURE) ? parseMediaCapabilities(doc.media) : undefined;
        } catch { return undefined; }
      })();
      advertised = media !== undefined;
      return media;
    },

    async issueTurn() {
      if (!advertised) return { kind: "refused" };
      // One request id per attempt; every retry of this attempt at this Home reuses it, so the Home finds the same reservation.
      const body = encodeTurnRequest(crypto.getRandomValues(new Uint8Array(16)));
      let result = await once(body);
      for (const delay of TURN_RETRY_MS) {
        if (result.kind !== "unknown") break;
        await wait(delay);
        result = await once(body);
      }
      return result;
    },
  };
}
