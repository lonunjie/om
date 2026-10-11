// Omnibar adaptivity contract (UI-10/UI-11): a ranked suggestion is bound to the profile that produced it.
// It resolves through the Action Registry as origin "suggestion", so it can never carry or obtain a confirmation.
import type { ProfileExecutionContext } from "@om/platform-api";
import type { ActionRegistry, ActionResult } from "../actions/registry";

export interface Suggestion { readonly actionId: string; readonly params: unknown }
export type SuggestionResult = ActionResult | { readonly status: "cancelled"; readonly reason: "profile_changed" };

export interface BoundSuggestion {
  readonly ctx: ProfileExecutionContext;
  /** Re-checks the live profile at execution time; cancels instead of landing in another profile. Takes no confirmation. */
  execute(): SuggestionResult;
}

export interface SuggestionDeps {
  readonly registry: ActionRegistry;
  /** Reads the live context at execution time (not captured). Throwing or no active profile cancels. */
  readonly currentContext: () => ProfileExecutionContext;
}

export function bindSuggestionToProfile(suggestion: Suggestion, ctx: ProfileExecutionContext, deps: SuggestionDeps): BoundSuggestion {
  const bound = Object.freeze({ actionId: suggestion.actionId, params: suggestion.params });
  return {
    ctx,
    execute() {
      let now: ProfileExecutionContext;
      try { now = deps.currentContext(); } catch { return { status: "cancelled", reason: "profile_changed" }; }
      if (now.profileId !== ctx.profileId || now.generation !== ctx.generation) return { status: "cancelled", reason: "profile_changed" };
      return deps.registry.resolve({ actionId: bound.actionId, params: bound.params, ctx, origin: "suggestion" });
    },
  };
}
