import type { DelegationTrust } from "./sign.mjs";
export interface DigestEntry {
  version: string; commit: string; manifestSha256: string; sha256sumsSha256: string; time: string; prev: string;
  signature: { alg: "ed25519"; keyId: string; delegation: { version: number; sha256: string }; sig: string };
}
export declare const GENESIS: string;
export declare const LOG_DOMAIN: string;
export declare const DEFAULT_LOG: string;
export declare function appendEntry(logFile: string, e: { version: string; commit: string; manifestSha256: string; sha256sumsSha256: string; time?: string }, pem: string, keyId: string, delegation: unknown, trust?: DelegationTrust): DigestEntry;
export declare function verifyLog(logFile: string, opts?: { roots?: Record<string, string | { pub: string; frame: string }>; threshold?: number; now?: number; pq?: { keyId: string; alg: string; pub: string; frame: string } | null; pqRequired?: boolean; delegations?: unknown[] }): { ok: boolean; errors: string[]; entries: DigestEntry[] };
