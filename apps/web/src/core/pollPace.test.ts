import { describe, expect, it } from "vitest";
import { AWAY_AFTER_MS, POLL_ACTIVE_MS, POLL_HIDDEN_MS, POLL_IDLE_MS, createPollPace } from "./pollPace";

describe("poll pace", () => {
  const make = () => {
    const s = { t: 1_000_000, hidden: false };
    return { s, pace: createPollPace({ now: () => s.t, hidden: () => s.hidden }) };
  };
  it("asks quickly while the app is in use, slowly once it is untouched, slowest while hidden", () => {
    const { s, pace } = make();
    expect(pace.next()).toBe(POLL_ACTIVE_MS);
    s.t += AWAY_AFTER_MS + 1;
    expect(pace.next()).toBe(POLL_IDLE_MS);
    s.hidden = true;
    expect(pace.next()).toBe(POLL_HIDDEN_MS);
  });
  it("a touch brings the quick pace back", () => {
    const { s, pace } = make();
    s.t += AWAY_AFTER_MS * 5;
    expect(pace.next()).toBe(POLL_IDLE_MS);
    pace.touch();
    expect(pace.next()).toBe(POLL_ACTIVE_MS);
  });
  it("a day left open and untouched costs a fraction of the requests", () => {
    const perDay = (ms: number): number => Math.round(86_400_000 / ms);
    expect(perDay(POLL_ACTIVE_MS)).toBe(28_800);
    expect(perDay(POLL_HIDDEN_MS)).toBe(1_440);
  });
});
