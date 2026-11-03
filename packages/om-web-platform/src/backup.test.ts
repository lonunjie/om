import { describe, expect, it } from "vitest";
import { deriveBackupKey } from "./backup";

const toHex = (b: Uint8Array): string => Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
const SALT = Uint8Array.from({ length: 16 }, (_, i) => i);
const COST = { mKib: 64, t: 1, p: 1 }; // token cost, tests only: a real backup uses what Core's plan says (64 MiB, 3 passes)

describe("backup key from a password (Argon2id)", () => {
  it("matches the pinned vector", async () => {
    // Argon2id v1.3, password "correct horse battery staple", salt 00..0f, m=64 KiB, t=1, p=1, 32 bytes. Pinned from @noble/hashes 2.4.0;
    // the same value is the `master` an OMB-1 file writer would hand Core with these settings (verification/vectors/om-backup-file-vectors.json
    // pins the Core side with a constant master, so the two halves compose without sharing a cost setting).
    const key = await deriveBackupKey("correct horse battery staple", COST, SALT);
    expect(toHex(key ?? new Uint8Array())).toBe("92dc5d67019623868bde079275e522f4b7e8213d3414ed85cbc2ac8a41117288");
  });

  it("is deterministic, depends on the salt and the settings, and is 32 bytes", async () => {
    const a = await deriveBackupKey("a password", COST, SALT);
    const b = await deriveBackupKey("a password", COST, SALT);
    const otherSalt = await deriveBackupKey("a password", COST, new Uint8Array(16));
    const otherCost = await deriveBackupKey("a password", { ...COST, t: 2 }, SALT);
    expect(a?.length).toBe(32);
    expect(toHex(a ?? new Uint8Array())).toBe(toHex(b ?? new Uint8Array()));
    expect(toHex(a ?? new Uint8Array())).not.toBe(toHex(otherSalt ?? new Uint8Array()));
    expect(toHex(a ?? new Uint8Array())).not.toBe(toHex(otherCost ?? new Uint8Array()));
  });

  it("types the same password the same way on any keyboard (NFKC), and refuses an empty one", async () => {
    const composed = await deriveBackupKey("café au lait 123", COST, SALT);
    const decomposed = await deriveBackupKey("café au lait 123", COST, SALT);
    const fullWidth = await deriveBackupKey("ｃafé au lait 123", COST, SALT);
    expect(toHex(composed ?? new Uint8Array())).toBe(toHex(decomposed ?? new Uint8Array()));
    expect(toHex(composed ?? new Uint8Array())).toBe(toHex(fullWidth ?? new Uint8Array()));
    expect(await deriveBackupKey("", COST, SALT)).toBeUndefined();
  });
});
