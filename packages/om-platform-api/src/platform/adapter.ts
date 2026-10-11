import type { BackgroundPush, Capability, CapabilityEntry, LocalContactsMode, LocalDiscoveryMode } from "./capability";
import type { FamilyApis, FamilyName } from "./families";

export interface CapabilityReport {
  readonly families: Readonly<Record<FamilyName, CapabilityEntry>>;
  readonly backgroundPush: BackgroundPush;
  readonly localDiscovery: LocalDiscoveryMode;
  readonly localContacts: LocalContactsMode;
  readonly callSystemIntegration: boolean;
}

/** Capability-oriented, no business logic. Each accessor returns state plus the typed API when usable. */
export type PlatformAdapter = {
  capabilities(): CapabilityReport;
} & { readonly [K in FamilyName]: () => Capability<FamilyApis[K]> };
