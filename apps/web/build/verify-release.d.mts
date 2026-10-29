export type RootKeyDescriptor = string | { pub: string; frame: string };
export interface PqRootPin { keyId: string; alg: string; pub: string; frame: string }
export interface RootPin { keys: Record<string, RootKeyDescriptor>; threshold: number; pq?: PqRootPin | null; pqRequired?: boolean }
export interface DelegationKey { keyId: string; alg: string; pub: string; notBefore: string; notAfter: string }
export interface Delegation { format: string; version: number; expires: string; keys: DelegationKey[]; revoked: string[]; pqRequired?: true; signatures: { keyId: string; alg: string; sig: string; frame?: string }[] }
export type Verdict<T = object> = ({ ok: true } & T) | { ok: false; reason: string };
export declare function verifyRelease(
  dir: string,
  opts?: { requireSigned?: boolean; roots?: Record<string, RootKeyDescriptor>; threshold?: number; now?: number; delegationFloor?: number | null; pq?: PqRootPin | null; pqRequired?: boolean; pqFloor?: boolean; requireProvenance?: boolean },
): { ok: boolean; errors: string[]; info: { provenance?: { ok: boolean; commit?: string; builder?: string }; version?: string; dev?: boolean; signed?: boolean; keyId?: string; signatureValid?: boolean; delegationVersion?: number } };
export declare const PINNED_ROOTS: Record<string, RootKeyDescriptor>;
export declare const FRAME_RAW: string;
export declare const FRAME_SOLANA_OFFCHAIN_V0: string;
export declare const KNOWN_FRAMES: string[];
export declare function ledgerText(d: Delegation, payloadSha256Hex: string): string;
export declare function frameDelegationPayload(frame: string, d: Delegation, signerPubHex: string): Buffer | null;
export declare function describeRoot(v: unknown): { pub: string; frame: string } | null;
export declare const PINNED_ROOT_THRESHOLD: number;
export declare const DELEGATION_FORMAT: string;
export declare function canonicalJson(v: unknown): string;
export declare function delegationPayload(d: unknown): Buffer;
export declare function parseDelegation(v: unknown): Delegation | null;
export declare function verifyDelegation(raw: unknown, pin: RootPin, ctx?: { now?: number; floor?: number | null; pqFloor?: boolean }): Verdict<{ delegation: Delegation; pqRequired?: true }>;
export declare function resolveReleaseKey(d: Delegation, keyId: string, alg: string, now?: number): Verdict<{ pub: string }>;
export declare function verifyManifestSignature(manifest: unknown, pin: RootPin, ctx?: { now?: number; floor?: number | null; pqFloor?: boolean }): Verdict<{ delegationVersion: number; pqRequired?: true }>;
export declare const PQ_ALG: string;
export declare const PQ_FRAME: string;
export declare const PQ_DOMAIN: string;
export declare const PQ_PUBLIC_KEY_BYTES: number;
export declare const PQ_SIGNATURE_BYTES: number;
export declare const PINNED_PQ_ROOT: PqRootPin | null;
export declare const PINNED_PQ_REQUIRED: boolean;
export declare function describePq(v: unknown): PqRootPin | null;
export declare function pqSigningBytes(pin: PqRootPin, payload: Uint8Array): Buffer;
export declare function pqVerify(pin: PqRootPin, payload: Uint8Array, sigBase64: string): boolean;
export declare const PROVENANCE_FILE: string;
export declare const DSSE_PAYLOAD_TYPE: string;
export declare const STATEMENT_TYPE: string;
export declare const SLSA_PREDICATE_TYPE: string;
export declare function dssePae(type: string, body: Uint8Array): Buffer;
export declare function verifyProvenance(
  dir: string,
  opts?: { roots?: Record<string, RootKeyDescriptor>; threshold?: number; now?: number; delegationFloor?: number | null; pq?: PqRootPin | null; pqRequired?: boolean; pqFloor?: boolean },
): { ok: boolean; errors: string[]; info: { commit?: string; keyId?: string; builder?: string } };
