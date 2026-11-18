/**
 * Settings > Alerts: whether OM may wake this device when it is closed (ADR-0210). Plain words, no protocol terms; it says what a push service can
 * learn (that this device was woken, and when) and what it cannot (anything about the message), and the one condition on iPhone and iPad.
 */
export type AlertsUnavailable = "no_browser" | "needs_install" | "no_home" | "many_profiles";
export type AlertsFailure = "denied" | "refused" | "full" | "unreachable" | "failed" | "unavailable";

export interface AlertsCopy {
  readonly heading: string;
  readonly intro: string;
  /** What the browser's push service learns, and what it does not. */
  readonly privacy: string;
  /** The Add to Home Screen condition, shown whether or not it applies. */
  readonly ios: string;
  /** Honest limit: alerts can be late or missed. */
  readonly limit: string;
  readonly loading: string;
  readonly loadFailed: string;
  readonly unavailable: Readonly<Record<AlertsUnavailable, string>>;
  readonly failure: Readonly<Record<AlertsFailure, string>>;
  readonly off: string;
  readonly on: string;
  readonly turnOn: string;
  readonly turnOff: string;
  /** Shown at the Maximum protection level before turning on: the cost, then an explicit confirmation. */
  readonly maximum: { readonly title: string; readonly body: string; readonly confirm: string; readonly keepOff: string };
  /** Alerts are on but protection became Maximum: they are paused until the person confirms. */
  readonly paused: { readonly body: string; readonly keep: string };
  readonly turnedOn: string;
  readonly turnedOff: string;
}
