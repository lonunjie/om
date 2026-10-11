// Passphrase input rules shared by setup and unlock. The vault derives the key (Argon2id); the UI only checks shape.
/** Counted in Unicode code points, not UTF-16 units. A longer minimum than a login password: the vault is attackable offline. */
export const PASSPHRASE_MIN_CODE_POINTS = 12;
export const PASSPHRASE_MAX_CODE_POINTS = 256;

export type PassphraseProblem = "too_short" | "too_long" | "mismatch";

const codePoints = (s: string): number => Array.from(s).length; // code points, not UTF-16 units

export function checkNewPassphrase(value: string, confirm: string): PassphraseProblem | null {
  const n = codePoints(value);
  if (n < PASSPHRASE_MIN_CODE_POINTS) return "too_short";
  if (n > PASSPHRASE_MAX_CODE_POINTS) return "too_long";
  return value === confirm ? null : "mismatch";
}

const encoder = new TextEncoder();
/** A fresh buffer each time: the vault zeroes what it is given. */
export const passphraseBytes = (value: string): Uint8Array => encoder.encode(value);
