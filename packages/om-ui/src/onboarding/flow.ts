export type OnboardingStep = "welcome" | "protect_device" | "recovery_key" | "invite_first" | "ready";
export interface FlowState { readonly step: OnboardingStep }
export type FlowEvent =
  | { readonly type: "start" }
  | { readonly type: "device_protected" }
  | { readonly type: "key_confirmed" }
  | { readonly type: "invite_accepted" }
  | { readonly type: "invites_skipped" };

export const initialFlow: FlowState = { step: "welcome" };

const NEXT: Readonly<Record<OnboardingStep, Readonly<Partial<Record<FlowEvent["type"], OnboardingStep>>>>> = {
  welcome: { start: "protect_device" },
  protect_device: { device_protected: "recovery_key" },
  recovery_key: { key_confirmed: "invite_first" },
  invite_first: { invite_accepted: "ready", invites_skipped: "ready" },
  ready: {},
};

/** Pure and total: an event that is not legal in the current step leaves the state unchanged (no step can be skipped). */
export function flowReducer(state: FlowState, event: FlowEvent): FlowState {
  const next = NEXT[state.step][event.type];
  return next === undefined ? state : { step: next };
}
