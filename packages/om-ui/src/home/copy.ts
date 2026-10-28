import type { HomeUrlProblem, HomeProbeResult } from "./services";

/** Wording for Settings > Your Home. "Home" is the product word; no infrastructure vocabulary reaches the user. */
export interface HomeCopy {
  readonly heading: string;
  readonly currentHeading: string;
  /** No Home at all: nothing leaves this device. */
  readonly none: string;
  readonly address: (url: string) => string;
  readonly source: { readonly chosen: string; readonly default: string };
  readonly checkingCurrent: string;
  /** The Home's own words about itself; the app cannot vouch for them. */
  readonly selfReported: string;
  readonly changeHeading: string;
  readonly changeBody: string;
  readonly urlLabel: string;
  readonly urlHint: string;
  readonly check: string;
  readonly checking: string;
  /** Shown above the confirm button when a Home is already in use. */
  readonly changeWarning: string;
  readonly useThis: string;
  readonly useAnyway: string;
  readonly alreadyUsing: string;
  readonly saveFailed: string;
  readonly saved: string;
  readonly reconnect: string;
  readonly useDefault: string;
  readonly disconnect: string;
  readonly cleared: string;
  readonly invalid: Readonly<Record<HomeUrlProblem, string>>;
  readonly result: {
    readonly ok: (providerId: string) => string;
    readonly health: Readonly<Record<Extract<HomeProbeResult, { kind: "ok" }>["health"], string>>;
    readonly unreachable: string;
    readonly failing: (status: number) => string;
    readonly notAHome: string;
    /** `offered` is the versions the Home lists, comma-separated; empty when it lists none. */
    readonly wrongVersion: (offered: string) => string;
    readonly noMailbox: string;
  };
  readonly lost: {
    readonly heading: string;
    readonly body: (count: number) => string;
    readonly item: (reference: string) => string;
  };
  readonly back: string;
}
