// The key for a chat backup file: Argon2id over the person's password (NFKC-normalised exactly like the vault's passphrase
// layer, so the same typed password means the same key on every keyboard). Core takes the 32 bytes and does everything else.
import { argon2idRaw, normalizePassphrase } from "./vault/crypto";

export interface BackupCost { readonly mKib: number; readonly t: number; readonly p: number }

const encoder = new TextEncoder();

/** 32 bytes, or `undefined` when the password is empty. The caller zeroes the result when done. */
export async function deriveBackupKey(password: string, cost: BackupCost, salt: Uint8Array<ArrayBuffer>): Promise<Uint8Array | undefined> {
  const pw = normalizePassphrase(encoder.encode(password));
  if (pw === undefined) return undefined;
  try {
    return await argon2idRaw(pw, salt, { m: cost.mKib, t: cost.t, p: cost.p });
  } finally {
    pw.fill(0);
  }
}
