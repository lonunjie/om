// When OM locks itself after the person has been away: a per-device preference, not a secret, so it lives in this browser's
// localStorage. Null means "only when OM is closed or reloaded". Anything else stored is ignored, never trusted.
export const IDLE_CHOICES = [1, 5, 15, 60] as const;
export type IdleMinutes = (typeof IDLE_CHOICES)[number] | null;

const KEY = "om.lock.idle-minutes";

const storage = (): Storage | undefined => { try { return globalThis.localStorage; } catch { return undefined; } };

export function readIdleMinutes(): IdleMinutes {
  try {
    const n = Number(storage()?.getItem(KEY));
    return (IDLE_CHOICES as readonly number[]).includes(n) ? (n as IdleMinutes) : null;
  } catch { return null; }
}

export function writeIdleMinutes(minutes: IdleMinutes): void {
  try {
    if (minutes === null) storage()?.removeItem(KEY); else storage()?.setItem(KEY, String(minutes));
  } catch { /* a browser that refuses storage keeps the choice for this visit only */ }
}
