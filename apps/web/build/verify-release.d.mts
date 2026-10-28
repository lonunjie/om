export declare function verifyRelease(
  dir: string,
  opts?: { requireSigned?: boolean; trustedKeys?: Record<string, string> },
): { ok: boolean; errors: string[]; info: { version?: string; dev?: boolean; signed?: boolean; keyId?: string; signatureValid?: boolean } };
export declare const PINNED_KEYS: Record<string, string>;
