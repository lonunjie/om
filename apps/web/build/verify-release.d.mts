export interface RootPin { keys: Record<string, string>; threshold: number }
export interface DelegationKey { keyId: string; alg: string; pub: string; notBefore: string; notAfter: string }
export interface Delegation { format: string; version: number; expires: string; keys: DelegationKey[]; revoked: string[]; signatures: { keyId: string; alg: string; sig: string }[] }
export type Verdict<T = object> = ({ ok: true } & T) | { ok: false; reason: string };
export declare function verifyRelease(
  dir: string,
  opts?: { requireSigned?: boolean; roots?: Record<string, string>; threshold?: number; now?: number; delegationFloor?: number | null },
): { ok: boolean; errors: string[]; info: { version?: string; dev?: boolean; signed?: boolean; keyId?: string; signatureValid?: boolean; delegationVersion?: number } };
export declare const PINNED_ROOTS: Record<string, string>;
export declare const PINNED_ROOT_THRESHOLD: number;
export declare const DELEGATION_FORMAT: string;
export declare function canonicalJson(v: unknown): string;
export declare function delegationPayload(d: unknown): Buffer;
export declare function parseDelegation(v: unknown): Delegation | null;
export declare function verifyDelegation(raw: unknown, pin: RootPin, ctx?: { now?: number; floor?: number | null }): Verdict<{ delegation: Delegation }>;
export declare function resolveReleaseKey(d: Delegation, keyId: string, alg: string, now?: number): Verdict<{ pub: string }>;
export declare function verifyManifestSignature(manifest: unknown, pin: RootPin, ctx?: { now?: number; floor?: number | null }): Verdict<{ delegationVersion: number }>;
