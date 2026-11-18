import type { AlertsCopy } from "../copy";

export const testAlertsCopy: AlertsCopy = {
  heading: "Alerts",
  intro: "Get a notice on this device when a message arrives while OM is closed.",
  privacy: "Your browser's push service learns that this device was woken, and when. It never learns who wrote or what was said. The notice only says New message.",
  ios: "On iPhone and iPad this works only after you add OM to your Home Screen.",
  limit: "Alerts can arrive late or not at all. Opening OM always gets everything.",
  loading: "Loading",
  loadFailed: "Could not load this page.",
  unavailable: { no_browser: "This browser cannot show alerts.", needs_install: "Add OM to your Home Screen first, then open it from there.", no_home: "Your delivery service does not offer alerts.", many_profiles: "Alerts are offered only when this device holds one identity." },
  failure: { denied: "Notifications are blocked for OM in this browser.", refused: "Your delivery service refused this device.", full: "Your delivery service has no room for another device.", unreachable: "Could not reach your delivery service.", failed: "Could not turn on alerts.", unavailable: "Alerts are not available here." },
  off: "Alerts are off.",
  on: "Alerts are on.",
  turnOn: "Turn on alerts",
  turnOff: "Turn off alerts",
  maximum: { title: "Turn on alerts at Maximum Privacy?", body: "Your browser's push service will see the time of every wake. Maximum Privacy keeps this off by default.", confirm: "Turn on anyway", keepOff: "Keep off" },
  paused: { body: "Alerts are paused because protection is Maximum Privacy.", keep: "Keep alerts on" },
  turnedOn: "Alerts are on for this device.",
  turnedOff: "Alerts are off for this device.",
};
