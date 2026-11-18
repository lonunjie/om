export type OnboardingStep = "welcome" | "choose" | "protect_device" | "invite_first" | "join_protect" | "join_link" | "recover_protect" | "recover_flow";
export interface FlowState { readonly step: OnboardingStep }
export type FlowEvent =
  | { readonly type: "start" }
  | { readonly type: "device_protected" }
  | { readonly type: "choose_new" }
  | { readonly type: "choose_join" }
  | { readonly type: "choose_recover" }
  | { readonly type: "back_to_choice" }
  | { readonly type: "vault_ready" };

export const initialFlow: FlowState = { step: "welcome" };
/** First run on a host that can link devices (deviation D24): the two equal choices come first. */
export const choosingFlow: FlowState = { step: "choose" };

// ADR-0111: recovery is on by default and needs no setup step; the safety code is offered later, never required here.
// "invite_first" is the ready state (SCREEN_MAP "zero-contact first run"): accepting or skipping an invite ends the flow.
// "I already use OM" (UX brief section 5): protect this device WITHOUT creating an identity, then type or scan the link the other
// device shows; Core adopts the account only when the other device approves (`join_link`).
const NEXT: Readonly<Record<OnboardingStep, Readonly<Partial<Record<FlowEvent["type"], OnboardingStep>>>>> = {
  welcome: { start: "protect_device" },
  choose: { choose_new: "protect_device", choose_join: "join_protect", choose_recover: "recover_protect" },
  protect_device: { device_protected: "invite_first" },
  invite_first: {},
  join_protect: { vault_ready: "join_link", back_to_choice: "choose" },
  join_link: {},
  // "I lost my devices" (ADR-0202/0203): protect this device first (no name, no identity), then the restricted recovery screens.
  recover_protect: { vault_ready: "recover_flow", back_to_choice: "choose" },
  recover_flow: {},
};

/** Pure and total: an event that is not legal in the current step leaves the state unchanged (no step can be skipped). */
export function flowReducer(state: FlowState, event: FlowEvent): FlowState {
  const next = NEXT[state.step][event.type];
  return next === undefined ? state : { step: next };
}
