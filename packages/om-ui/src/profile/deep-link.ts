// om: deep links with several IdentityProfiles (UI-07): never choose a profile by a hidden heuristic.
import { asProfileId } from "@om/platform-api";
import type { ProfileId, ProfileSummary } from "@om/platform-api";

export type DeepLinkResolution =
  | { readonly kind: "open"; readonly profileId: ProfileId; readonly target: string }
  | { readonly kind: "choose_profile"; readonly target: string; readonly options: readonly ProfileSummary[] }
  | { readonly kind: "rejected"; readonly reason: "malformed" | "no_profiles" | "unknown_profile" };

const MAX_LINK = 2048;
const TARGET = /^[A-Za-z0-9_.:/-]{1,256}$/;
const ID = /^[A-Za-z0-9_.:-]{1,128}$/;
const rejected = (reason: "malformed" | "no_profiles" | "unknown_profile"): DeepLinkResolution => ({ kind: "rejected", reason });

export function resolveDeepLink(link: string, profiles: readonly ProfileSummary[]): DeepLinkResolution {
  if (typeof link !== "string" || link.length > MAX_LINK || !link.startsWith("om:") || link.includes("#")) return rejected("malformed");
  const rest = link.slice(3);
  const q = rest.indexOf("?");
  const target = q === -1 ? rest : rest.slice(0, q);
  if (!TARGET.test(target)) return rejected("malformed");
  const explicit: string[] = [];
  if (q !== -1) {
    for (const pair of rest.slice(q + 1).split("&")) {
      const [name, value, ...extra] = pair.split("=");
      if (extra.length > 0 || name === undefined || name === "") return rejected("malformed");
      if (name === "profile") explicit.push(value ?? "");
    }
  }
  if (explicit.length > 1) return rejected("malformed");
  if (profiles.length === 0) return rejected("no_profiles");
  const [only, ...others] = profiles;
  if (explicit.length === 1) {
    const e = explicit[0] ?? "";
    if (!ID.test(e)) return rejected("malformed");
    return profiles.some((p) => p.id === e) ? { kind: "open", profileId: asProfileId(e), target } : rejected("unknown_profile");
  }
  if (only !== undefined && others.length === 0) return { kind: "open", profileId: only.id, target };
  return { kind: "choose_profile", target, options: profiles };
}
