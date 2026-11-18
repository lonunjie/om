/** What the Alerts screen needs from its host (structural: the web host implements it in `core/pushAlerts.ts`). */
export type AlertsAvailability = "ready" | "no_browser" | "needs_install" | "no_home" | "many_profiles";
export type EnableResult = "enabled" | "denied" | "refused" | "full" | "unreachable" | "failed" | "unavailable";
export interface AlertsStatus { readonly on: boolean; readonly permission: "granted" | "denied" | "default"; readonly maxOk: boolean }

export interface AlertsServices {
  availability(): Promise<AlertsAvailability>;
  status(): Promise<AlertsStatus>;
  enable(o: { readonly confirmedMaximum: boolean }): Promise<EnableResult>;
  disable(): Promise<void>;
  /** Renews the Home's registration at start (when on); not at Maximum unless the person confirmed. */
  refresh(o: { readonly maximum: boolean }): Promise<void>;
}
