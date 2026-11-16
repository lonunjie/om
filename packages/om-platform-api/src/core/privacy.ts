import type { ProfileExecutionContext } from "./ids";

/** The three levels a person can choose for themselves. "Custom" is shown by the client when fields differ; it is never chosen here (SECURITY_PRIVACY_PROFILES section 2). */
export type ChosenPrivacyLevel = "recommended" | "hardened" | "maximum";

/** One typed step: set the level. `confirmed` is true only after the screen named what loosening changes. */
export interface PrivacyIntent {
  readonly kind: "set_level";
  readonly level: ChosenPrivacyLevel;
  readonly confirmed?: true;
}

export type PrivacyRefusal = "loosening_needs_confirmation" | "unavailable";
export type PrivacyStep = { readonly ok: true } | { readonly ok: false; readonly refused: PrivacyRefusal };

/** Local and durable when `set` resolves. Core refuses an unconfirmed loosening whatever the screen thinks. */
export interface PrivacyApi {
  set(ctx: ProfileExecutionContext, intent: PrivacyIntent): Promise<PrivacyStep>;
}
