// Shell-agnostic capability model (UI_CORE_PLATFORM_CONTRACT "PlatformAdapter"; ADR-0178).
export type CapabilityState = "available" | "partial" | "unavailable";

/** Machine reason code, never English copy (e.g. "sensitive_unsupported"). */
export type CapabilityDetail = string;

export type Capability<T> =
  | { readonly state: "unavailable"; readonly detail?: CapabilityDetail }
  | { readonly state: "available" | "partial"; readonly detail?: CapabilityDetail; readonly api: T };

export interface CapabilityEntry {
  readonly state: CapabilityState;
  readonly detail?: CapabilityDetail;
}

export const available = <T>(api: T, detail?: CapabilityDetail): Capability<T> =>
  detail === undefined ? { state: "available", api } : { state: "available", api, detail };
export const partial = <T>(api: T, detail?: CapabilityDetail): Capability<T> =>
  detail === undefined ? { state: "partial", api } : { state: "partial", api, detail };
export const unavailable = <T>(detail?: CapabilityDetail): Capability<T> =>
  detail === undefined ? { state: "unavailable" } : { state: "unavailable", detail };

export type BackgroundPush = "webpush" | "apns" | "fcm" | "unifiedpush" | "persistent" | "none";
export type LocalDiscoveryMode = "automatic" | "manual_known_endpoint" | "unavailable";
export type LocalContactsMode = "read_optional" | "unavailable";
