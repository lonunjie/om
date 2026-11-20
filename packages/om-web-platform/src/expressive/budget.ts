import type { ValidatedExpressiveAsset } from "@om/platform-api";
import { budget as registryBudget, kind_resource_profile, media_profiles, resource_profiles } from "../../../../spec/registries/om-expressive-media.json";

export const DECODE_TIME_MS = registryBudget.decode_time_limit_ms;

/** Checks the existing Core walker's facts before loading bytes into a native decoder. */
export function checkBudget(asset: ValidatedExpressiveAsset): "unsupported_profile" | "invalid_budget" | undefined {
  if (!media_profiles.some((profile) => profile.id === asset.mediaProfileId && profile.media_type === "image/webp")) {
    return "unsupported_profile";
  }

  const id = kind_resource_profile[asset.kind] as keyof typeof registryBudget.profiles;
  const profile = resource_profiles.find((candidate) => candidate.id === id);
  if (profile === undefined) return "invalid_budget";
  const budget = registryBudget.profiles[id];
  const { width, height, frames, durationMs, encodedSize, totalPixels } = asset.facts;
  const pixels = width * height;
  const canvasTotal = pixels * frames;
  // Compositing canvas, previous frame, current frame and the RGBA readback.
  const residentBytes = pixels * 4 * 4;
  if (![width, height, frames, encodedSize, totalPixels].every((n) => Number.isSafeInteger(n) && n > 0)
    || encodedSize !== asset.bytes.byteLength || encodedSize > profile.max_encoded_bytes
    || width > profile.max_canvas || height > profile.max_canvas
    || frames > budget.max_frames || pixels > budget.max_pixels_per_frame
    || canvasTotal > registryBudget.multiplication_limit
    || totalPixels < frames || totalPixels > Math.min(canvasTotal, budget.max_total_pixels)
    || residentBytes > registryBudget.resident_decode_ceiling_bytes) {
    return "invalid_budget";
  }
  if (durationMs === null) {
    if (frames !== 1 || profile.animated === "required") return "invalid_budget";
  } else if (!Number.isSafeInteger(durationMs) || durationMs < 1 || durationMs > profile.max_duration_ms
    || (profile.animated === "required" && frames < 2)
    || (frames > 1 && frames * 1000 > durationMs * profile.max_fps)) {
    return "invalid_budget";
  }
  return undefined;
}
