import { describe, expect, it } from "vitest";
import { flowReducer, initialFlow } from "./flow";
import type { FlowEvent, FlowState } from "./flow";
import { groupMatches, pickVerifyIndices } from "./recovery";

const run = (events: readonly FlowEvent["type"][], from: FlowState = initialFlow): FlowState =>
  events.reduce((s, type) => flowReducer(s, { type }), from);

describe("onboarding flow reducer", () => {
  it("walks welcome -> protect_device -> recovery_key -> invite_first -> ready", () => {
    expect(initialFlow.step).toBe("welcome");
    expect(run(["start"]).step).toBe("protect_device");
    expect(run(["start", "device_protected"]).step).toBe("recovery_key");
    expect(run(["start", "device_protected", "key_confirmed"]).step).toBe("invite_first");
    expect(run(["start", "device_protected", "key_confirmed", "invite_accepted"]).step).toBe("ready");
    expect(run(["start", "device_protected", "key_confirmed", "invites_skipped"]).step).toBe("ready");
  });

  it("ignores events that are illegal in the current step, so no step can be skipped", () => {
    expect(run(["key_confirmed", "invite_accepted", "device_protected"]).step).toBe("welcome");
    expect(run(["start", "key_confirmed"]).step).toBe("protect_device");
    expect(run(["start", "device_protected", "invites_skipped"]).step).toBe("recovery_key");
    expect(run(["start", "device_protected", "key_confirmed", "start"]).step).toBe("invite_first");
    const ready = run(["start", "device_protected", "key_confirmed", "invites_skipped"]);
    expect(run(["start", "device_protected", "key_confirmed", "invite_accepted"], ready)).toBe(ready);
  });

  it("is pure: the input state is unchanged", () => {
    const s: FlowState = Object.freeze({ step: "welcome" });
    flowReducer(s, { type: "start" });
    expect(s.step).toBe("welcome");
  });
});

describe("recovery verification helpers", () => {
  it("picks 3 distinct ascending indices in range", () => {
    for (let i = 0; i < 50; i++) {
      const idx = pickVerifyIndices(9);
      expect(idx).toHaveLength(3);
      expect(new Set(idx).size).toBe(3);
      expect([...idx].sort((a, b) => a - b)).toEqual(idx);
      for (const v of idx) {
        expect(v).toBeGreaterThanOrEqual(0);
        expect(v).toBeLessThan(9);
      }
    }
  });
  it("never asks for more groups than exist", () => {
    expect(pickVerifyIndices(2)).toHaveLength(2);
    expect(pickVerifyIndices(0)).toEqual([]);
  });
  it("matches ignoring case and whitespace, never an empty entry", () => {
    expect(groupMatches(" ab c1 ", "ABC1")).toBe(true);
    expect(groupMatches("", "")).toBe(false);
    expect(groupMatches("ABC2", "ABC1")).toBe(false);
  });
});
