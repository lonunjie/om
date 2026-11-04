import type { HomeUrlProblem, HomeProbeResult } from "./services";

/** Wording for Settings > Message delivery. The user-facing word is "delivery service" (the code and docs keep "Home"); no infrastructure vocabulary reaches the user. */
export interface HomeCopy {
  readonly heading: string;
  /** What the delivery service is, in two plain sentences, shown first. */
  readonly intro: string;
  readonly currentHeading: string;
  /** No Home at all: nothing leaves this device. */
  readonly none: string;
  readonly address: (url: string) => string;
  /** The raw address is infrastructure detail: it stays behind this disclosure. */
  readonly showAddress: string;
  readonly hideAddress: string;
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
