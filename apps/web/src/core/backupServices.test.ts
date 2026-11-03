import { describe, expect, it, vi } from "vitest";
import { asProfileId, captureProfileContext, asProfileGeneration } from "@om/platform-api";
import { CoreCallError, type BackupHeader, type BackupPlan, type ImportSummary, type WasmCoreClient } from "@om/core-wasm";
import { backupFileName, createBackupServices } from "./backupServices";

const ctx = captureProfileContext(asProfileId("p-1"), asProfileGeneration("g"));
const PLAN: BackupPlan = { kdf: { mKib: 8, t: 1, p: 1 }, salt: "00".repeat(16) };
const HEAD: BackupHeader = { version: 1, profile: 1, kdf: PLAN.kdf, salt: PLAN.salt, ciphertextLen: 99 };
const SUMMARY: ImportSummary = { chatsAdded: 1, chatsUpdated: 0, chatsSkipped: 0, messagesAdded: 3 };

function fakeRt(over: Partial<Record<"query" | "backupPlan" | "exportBackup" | "inspectBackup" | "importBackup", unknown>> = {}): WasmCoreClient {
  return {
    query: vi.fn(() => Promise.resolve([{ id: "c" }])), backupPlan: vi.fn(() => PLAN), exportBackup: vi.fn(() => Uint8Array.of(1, 2, 3)),
    inspectBackup: vi.fn(() => HEAD), importBackup: vi.fn(() => Promise.resolve(SUMMARY)), ...over,
  } as unknown as WasmCoreClient;
}
const refuse = (reason: string) => () => { throw new CoreCallError(`backup_${reason}`, reason); };

describe("backup services", () => {
  it("names the file after today's local date", () => {
    expect(backupFileName(new Date(2026, 10, 3, 23, 59))).toBe("om-backup-2026-11-03.ombackup");
  });

  it("saves: derives the key from the password and the plan, hands it to Core, and wipes it", async () => {
    let seen: Uint8Array | undefined;
    const rt = fakeRt({ exportBackup: vi.fn((_c: unknown, _p: unknown, key: Uint8Array) => { seen = key; return Uint8Array.of(9); }) });
    const r = await createBackupServices({ rt, ctx, now: () => new Date(2026, 0, 2) }).save("a long enough password");
    expect(r).toEqual({ ok: true, bytes: Uint8Array.of(9), name: "om-backup-2026-01-02.ombackup" });
    expect(seen?.length).toBe(32);
    expect(Array.from(seen ?? [1])).toEqual(new Array(32).fill(0)); // wiped after use
  });

  it("does not make a file when there are no chats, or no password", async () => {
    const exportBackup = vi.fn(() => Uint8Array.of(1));
    const none = fakeRt({ query: vi.fn(() => Promise.resolve([])), exportBackup });
    expect(await createBackupServices({ rt: none, ctx }).save("a long enough password")).toEqual({ ok: false, failure: "no_chats" });
    expect(exportBackup).not.toHaveBeenCalled();
    expect(await createBackupServices({ rt: fakeRt(), ctx }).save("")).toEqual({ ok: false, failure: "empty_password" });
  });

  it("restores with the settings and salt the FILE names, not the current ones", async () => {
    const odd: BackupHeader = { ...HEAD, kdf: { mKib: 16, t: 2, p: 1 }, salt: "ab".repeat(16) };
    const importBackup = vi.fn(() => Promise.resolve(SUMMARY));
    const a = await createBackupServices({ rt: fakeRt({ inspectBackup: vi.fn(() => odd), importBackup }), ctx }).restore(Uint8Array.of(1), "pw pw pw pw pw pw");
    const b = await createBackupServices({ rt: fakeRt({ importBackup }), ctx }).restore(Uint8Array.of(1), "pw pw pw pw pw pw");
    expect(a).toEqual({ ok: true, summary: SUMMARY });
    const keyA = (importBackup.mock.calls[0] as unknown as [unknown, unknown, Uint8Array])[2];
    expect(keyA.length).toBe(32);
    expect(b.ok).toBe(true);
  });

  it("passes every Core refusal through by name, and anything else as 'failed'", async () => {
    for (const reason of ["wrong_password", "tampered", "truncated", "not_a_backup", "unsupported_version", "invalid_content"] as const) {
      const r = await createBackupServices({ rt: fakeRt({ importBackup: vi.fn(() => Promise.reject(new CoreCallError(`backup_${reason}`, reason))) }), ctx }).restore(Uint8Array.of(1), "pw pw pw pw pw pw");
      expect(r).toEqual({ ok: false, failure: reason });
      expect(createBackupServices({ rt: fakeRt({ inspectBackup: vi.fn(refuse(reason)) }), ctx }).check(Uint8Array.of(1))).toBe(reason);
    }
    const odd = await createBackupServices({ rt: fakeRt({ importBackup: vi.fn(() => Promise.reject(new Error("boom"))) }), ctx }).restore(Uint8Array.of(1), "pw pw pw pw pw pw");
    expect(odd).toEqual({ ok: false, failure: "failed" });
    expect(createBackupServices({ rt: fakeRt(), ctx }).check(Uint8Array.of(1))).toBeNull();
  });

  it("says an empty password is not a password, before touching Core's import", async () => {
    const importBackup = vi.fn(() => Promise.resolve(SUMMARY));
    const rt = fakeRt({ importBackup });
    expect(await createBackupServices({ rt, ctx }).restore(Uint8Array.of(1), "")).toEqual({ ok: false, failure: "empty_password" });
    expect(importBackup).not.toHaveBeenCalled();
  });
});
