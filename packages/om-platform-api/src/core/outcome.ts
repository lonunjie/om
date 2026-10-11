// Stable outcome classes (UI_CORE_PLATFORM_CONTRACT.md). Provider-neutral; never free provider text.
export type RejectReason =
  | "invalid"
  | "unsupported"
  | "unauthorized"
  | "revoked"
  | "expired"
  | "stale_state"
  | "conflict"
  | "policy_denied"
  | "quota"
  | "rate_limited"
  | "integrity_failure"
  | "unavailable"
  | "not_found";

export type UserActionCode = "authorize_with_device" | "confirm_security_change" | "reauthenticate" | "update_client";

export type OutcomeClass =
  | { readonly kind: "Committed" }
  | { readonly kind: "PendingAuthorization" }
  | { readonly kind: "PendingNetwork" }
  | { readonly kind: "PendingReconciliation" }
  | { readonly kind: "NeedsUserAction"; readonly action: UserActionCode }
  | { readonly kind: "Rejected"; readonly reason: RejectReason };

export type TerminalOutcome = Extract<OutcomeClass, { kind: "Committed" | "Rejected" }>;

export const isTerminalOutcome = (o: OutcomeClass): o is TerminalOutcome => o.kind === "Committed" || o.kind === "Rejected";
