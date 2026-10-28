export declare function verifyRelease(
  dir: string,
  opts?: { requireSigned?: boolean },
): { ok: boolean; errors: string[]; info: { version?: string; dev?: boolean; signed?: boolean; keyId?: string } };
