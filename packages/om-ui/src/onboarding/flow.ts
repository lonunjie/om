export type OnboardingStep = "welcome" | "protect_device" | "invite_first";
export interface FlowState { readonly step: OnboardingStep }
export type FlowEvent =
  | { readonly type: "start" }
  | { readonly type: "device_protected" };

export const initialFlow: FlowState = { step: "welcome" };

// ADR-0111: recovery is on by default and needs no setup step; the safety code is offered later, never required here.
// "invite_first" is the ready state (SCREEN_MAP "zero-contact first run"): accepting or skipping an invite ends the flow.
const NEXT: Readonly<Record<OnboardingStep, Readonly<Partial<Record<FlowEvent["type"], OnboardingStep>>>>> = {
  welcome: { start: "protect_device" },
  protect_device: { device_protected: "invite_first" },
  invite_first: {},
};

/** Pure and total: an event that is not legal in the current step leaves the state unchanged (no step can be skipped). */
export function flowReducer(state: FlowState, event: FlowEvent): FlowState {
  const next = NEXT[state.step][event.type];
  return next === undefined ? state : { step: next };
}
