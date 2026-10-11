// Opaque identifiers and the immutable profile execution context (R-332). Brands are compile-time only.
export type Brand<T, B extends string> = T & { readonly __brand: B };

export type OperationId = Brand<string, "OperationId">;
export type ProfileId = Brand<string, "ProfileId">;
export type ProfileGeneration = Brand<string, "ProfileGeneration">;
export type ConversationId = Brand<string, "ConversationId">;
export type PersonId = Brand<string, "PersonId">;
export type MessageId = Brand<string, "MessageId">;

export const asOperationId = (v: string): OperationId => v as OperationId;
export const asProfileId = (v: string): ProfileId => v as ProfileId;
export const asProfileGeneration = (v: string): ProfileGeneration => v as ProfileGeneration;
export const asConversationId = (v: string): ConversationId => v as ConversationId;
export const asPersonId = (v: string): PersonId => v as PersonId;
export const asMessageId = (v: string): MessageId => v as MessageId;

/** Captured at dispatch; never re-derived from a process-global active profile. */
export interface ProfileExecutionContext {
  readonly profileId: ProfileId;
  readonly generation: ProfileGeneration;
}

export function captureProfileContext(profileId: ProfileId, generation: ProfileGeneration): ProfileExecutionContext {
  return Object.freeze({ profileId, generation });
}

export function isProfileExecutionContext(v: unknown): v is ProfileExecutionContext {
  if (typeof v !== "object" || v === null || !Object.isFrozen(v)) return false;
  const r = v as Record<string, unknown>;
  return typeof r["profileId"] === "string" && r["profileId"] !== "" && typeof r["generation"] === "string" && r["generation"] !== "";
}
