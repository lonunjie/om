/**
 * The three levels a person can choose for themselves. "Custom" is shown by the client when fields differ; it is never chosen here
 * (SECURITY_PRIVACY_PROFILES section 2). A change goes through the `ChangePrivacyPolicy` intent with a Core-issued nonce.
 */
export type ChosenPrivacyLevel = "recommended" | "hardened" | "maximum";
