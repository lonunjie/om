import type { RecoveringView } from "@om/platform-api";
import type { OutcomeCopy } from "../devices/copy";

/**
 * "I lost my devices" (ADR-0200, ADR-0202, ADR-0203). Plain words: no epochs, digests, pins, heads or "Home". The person has the
 * saved recovery key (the sheet or its QR) and maybe still one working device; the screens say what to do next and what is going on.
 */
export interface LostDevicesCopy {
  readonly heading: string;
  readonly intro: string;
  readonly keyLabel: string;
  readonly keyHint: string;
  readonly placeLabel: string;
  readonly placeHint: string;
  readonly scan: string;
  readonly scanNothing: string;
  readonly find: string;
  readonly cancel: string;
  readonly looking: string;
  /** Core's lookup of the saved account. The same words for a wrong key and an unknown account. */
  readonly stage: Readonly<Record<Exclude<RecoveringView["stage"], "checking" | "found">, string>>;
  readonly foundHeading: string;
  readonly foundBody: string;
  /** What this device can finish by itself: only with the sheet kept from one of the person's own devices. */
  readonly evidence: Readonly<Record<"pinned" | "other", string>>;
  readonly ask: string;
  readonly waitingHeading: string;
  readonly waitingBody: string;
  /** "about 2 days left" for Core's remaining milliseconds; never a precise or promised time. */
  readonly remaining: (ms: number) => string;
  readonly waitDone: string;
  readonly checkAgain: string;
  readonly sendAgain: string;
  readonly vetoCheckIncomplete: string;
  readonly stop: string;
  readonly approval: {
    readonly heading: string;
    readonly body: string;
    readonly linkLabel: string;
    readonly linkHint: string;
    readonly join: string;
    readonly state: Readonly<Record<"waiting_for_controller" | "receiving" | "done" | "failed" | "expired", string>>;
    readonly codeLabel: string;
    readonly codeHint: string;
    readonly parseError: string;
  };
  readonly ended: Readonly<Record<"vetoed" | "stale" | "cancelled", string>>;
  readonly forgetAttempt: string;
  readonly forgetEffect: string;
  readonly startOver: string;
  readonly outcome: OutcomeCopy;
}

/** On a device that still works: approve the other device's request (the exact request, after comparing the code). */
export interface ApproveRecoveryCopy {
  readonly action: string;
  readonly heading: string;
  readonly body: string;
  readonly show: string;
  readonly back: string;
  readonly linkHeading: string;
  readonly notInvitation: string;
  readonly waiting: string;
  readonly codeLabel: string;
  readonly codeHint: string;
  readonly confirm: string;
  readonly confirmNote: string;
  readonly decline: string;
  readonly state: Readonly<Record<"waiting_for_device" | "awaiting_confirmation" | "approved" | "declined" | "failed" | "expired", string>>;
  readonly outcome: OutcomeCopy;
}

export interface ReconnectCopy {
  readonly heading: string;
  readonly reconnect: (name: string) => string;
  readonly link: (name: string) => string;
  readonly share: string;
  readonly copyLink: string;
  readonly copied: string;
  readonly copyFailed: string;
  readonly waiting: string;
  readonly ready: string;
  readonly open: (name: string) => string;
  readonly outcome: OutcomeCopy;
}
