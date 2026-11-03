// How often an open OM asks the delivery service for new messages. Every ask is a request the service has to answer (and, on a
// free plan, count), so OM asks quickly while someone is using it and slowly while the screen is hidden or untouched. A message
// that lands during a slow stretch shows up the moment the person returns (RealApp polls again on focus), never later.
export const POLL_ACTIVE_MS = 3000;
export const POLL_IDLE_MS = 15_000;
export const POLL_HIDDEN_MS = 60_000;
/** How long without a touch, key press or tap before OM counts the person as away from the screen. */
export const AWAY_AFTER_MS = 60_000;

export interface PollPaceEnv {
  readonly now: () => number;
  readonly hidden: () => boolean;
}

export interface PollPace {
  /** Milliseconds until the next ask. */
  readonly next: () => number;
  /** Records that the person just used the app. */
  readonly touch: () => void;
}

export function createPollPace(env: PollPaceEnv): PollPace {
  let last = env.now();
  return {
    touch: () => { last = env.now(); },
    next: () => (env.hidden() ? POLL_HIDDEN_MS : env.now() - last > AWAY_AFTER_MS ? POLL_IDLE_MS : POLL_ACTIVE_MS),
  };
}

/** The browser's pace: watches the page for use and visibility. Returns the function the Core runtime asks before each poll. */
export function browserPollPace(): () => number {
  const pace = createPollPace({ now: () => Date.now(), hidden: () => document.visibilityState === "hidden" });
  for (const type of ["pointerdown", "keydown", "touchstart", "focus"] as const) globalThis.addEventListener(type, pace.touch, { passive: true });
  return pace.next;
}
