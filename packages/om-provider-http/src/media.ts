// SPDX-License-Identifier: MPL-2.0
import { MEDIA_FEATURE, MEDIA_TURN_PATH, MEDIA_MAX_REPLY_BYTES, parseMediaCapabilities, decodeTurnLease, encodeTurnRequest } from "@om/home-wire";
import type { CallMediaProvider } from "./provider";
import { exchange, type HttpOptions } from "./http";

export function createMediaClient(base: string, http: HttpOptions): CallMediaProvider {
  let advertised = false;
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
      if (!advertised) return undefined;
      const body = encodeTurnRequest(crypto.getRandomValues(new Uint8Array(16)));
      // An unknown result does not trigger a second reservation. The backend limits even an unused reservation.
      const r = await exchange({ ...http, timeoutMs: Math.min(http.timeoutMs, 5000) }, `${base}${MEDIA_TURN_PATH}`, {
        method: "POST", body, maxReplyBytes: MEDIA_MAX_REPLY_BYTES,
      });
      if (r.kind !== "ok") return undefined;
      try { return decodeTurnLease(r.body, Math.floor(Date.now() / 1000)); }
      catch { return undefined; }
    },
  };
}
