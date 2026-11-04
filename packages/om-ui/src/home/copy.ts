import type { RelaySetupCopy } from "../onboarding/copy";
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
  /** Advanced: the person's delivery services, a backup, and moving to a different one. Plain words only ("delivery service"); the vendor is named only inside `add.relay.cloudflare`. */
  readonly advanced: {
    readonly toggle: string;
    readonly listHeading: string;
    readonly listIntro: string;
    readonly role: { readonly main: string; readonly backup: string; readonly leaving: string };
    readonly add: {
      readonly toggle: string;
      readonly intro: string;
      /** The same setup steps and fields as first run (`RelaySetupCopy`), so the one-tap setup is described once. */
      readonly relay: Pick<RelaySetupCopy, "cloudflare" | "setup">;
      readonly check: string;
      readonly cancel: string;
      readonly already: string;
      readonly full: string;
      readonly added: string;
    };
    readonly move: {
      readonly heading: string;
      readonly body: string;
      readonly needBackup: string;
      readonly action: string;
      readonly retry: string;
      readonly confirmHeading: string;
      readonly confirmBody: string;
      /** Only when leaving the main service: invitations made there and not yet accepted are collected at the new main service, which does not know them. */
      readonly confirmInvites: string;
      readonly yes: string;
      readonly no: string;
      readonly working: string;
      readonly done: string;
      readonly stuck: (chats: number) => string;
      readonly failed: string;
    };
  };
}
