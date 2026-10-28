import type { Delegation } from "./verify-release.mjs";
export interface RootKey { key: unknown; id: string; pub: string; fingerprint: string }
export declare function parseTime(t: string, now?: number): string;
export declare function fingerprint(pubHex: string): string;
export declare function rootKeyId(pubHex: string): string;
export declare function describeKey(pubHex: string): { id: string; pub: string; fingerprint: string };
export declare function generateRoot(): { pem: string; id: string; pub: string; fingerprint: string };
export declare function loadRoot(file: string): RootKey;
export declare function addRootSignature(delegation: unknown, root: RootKey): Delegation;
export declare function buildDelegation(o: {
  prev?: unknown; version?: number | undefined; expires: string; addKey?: { keyId: string; pub: string; notBefore?: string; notAfter?: string }; revokeId?: string | undefined; now?: number; allowLong?: boolean;
}): Delegation;
export declare function summary(d: Delegation): string;
