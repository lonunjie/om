export declare const RELEASE_KEY_ID: string;
export declare const VAULT_NOTE: string;
export declare function canonicalJson(v: unknown): string;
export declare function signingPayload(m: unknown): Buffer;
export declare function extractPem(text: string): string | null;
export declare function pemFromVault(): string;
export declare function signManifest(manifest: unknown, pem: string, keyId?: string): { alg: string; keyId: string; sig: string };
