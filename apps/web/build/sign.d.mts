export declare const RELEASE_KEY_ID: string;
export declare const VAULT_NOTE: string;
export declare function canonicalJson(v: unknown): string;
export declare function signingPayload(m: unknown): Buffer;
export declare function pemFromSecretText(text: string): string;
export declare function pemFromVault(): string;
export interface DelegationTrust { roots?: Record<string, string | { pub: string; frame: string }>; threshold?: number; now?: number; pq?: { keyId: string; alg: string; pub: string; frame: string } | null }
export declare function requireDelegation(delegation: unknown, pem: string, keyId: string, trust?: DelegationTrust): unknown;
export declare function signManifest(manifest: { delegation?: unknown }, pem: string, keyId?: string, trust?: DelegationTrust): { alg: string; keyId: string; sig: string };
