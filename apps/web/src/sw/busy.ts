// What the shell tells the update path about the session (CLIENTHOST_UPDATE_LIFECYCLE section 5, staged activation): a staged worker takes over
// and the page reloads, which ends a call, drops live media and loses work not yet saved. Each part of the shell that owns such state
// registers a probe that reads its live value; the update path asks all of them at each tick and waits while any says "busy". Probes read
// refs or plain values and never throw out of here: one that fails counts as busy (waiting is the safe answer).
export type BusyProbe = () => boolean;

const probes = new Set<BusyProbe>();

/** Registers a probe for the session state this caller owns; returns the function that removes it. */
export const addBusyProbe = (probe: BusyProbe): (() => void) => {
  probes.add(probe);
  return () => { probes.delete(probe); };
};

/** True while a call or live media session, an active ceremony or unsaved work means a reload would cost the person something. */
export const sessionBusy = (): boolean => {
  for (const probe of probes) {
    try { if (probe()) return true; } catch { return true; }
  }
  return false;
};
