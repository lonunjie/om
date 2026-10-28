import type { Delegation, PqRootPin } from "./verify-release.mjs";
export interface PqRoot { seed: Uint8Array; secretKey: Uint8Array; pub: string; keyId: string; alg: string; frame: string; fingerprint: string }
export declare function pqFingerprint(pubHex: string): string;
export declare function pqKeyId(pubHex: string): string;
export declare function pqDescriptor(pubHex: string): PqRootPin;
export declare function describePqKey(pubHex: string): PqRootPin & { fingerprint: string };
export declare function generatePqRoot(): PqRoot;
export declare function formatPqNote(r: { seed: Uint8Array; pub: string; keyId: string }): string;
export declare function loadPqRootNote(text: string): PqRoot;
export declare function checkDescriptor(v: unknown): PqRootPin;
export declare function pqPayload(delegation: unknown, pin: PqRootPin): Buffer;
export declare function addPqSignature(delegation: unknown, root: { secretKey: Uint8Array; pub: string }): Delegation;
export declare function externalPqAttach(delegation: unknown, pin: PqRootPin, sig: string): Delegation;
