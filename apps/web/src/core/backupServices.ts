// What Settings > Backup file can do, over the open identity's Core. Kept apart from the screen so each step can be tested without a DOM.
// The password never reaches Core: this derives a key from it (Argon2id, the same cost the app's own password lock uses), hands Core
// the key, and wipes it. Core writes and reads the file; it refuses a wrong password, a damaged or altered file, or a newer format by name.
import type { ProfileExecutionContext } from "@om/platform-api";
import { backupRefusal, fromHex, type BackupRefusal, type ImportSummary, type WasmCoreClient } from "@om/core-wasm";
import { deriveBackupKey } from "@om/web-platform";

/** Why a step did not happen: Core's own reason, a password nothing could be made from, or anything unexpected. */
export type BackupFailure = BackupRefusal | "empty_password" | "failed";

export type SaveResult = { readonly ok: true; readonly bytes: Uint8Array; readonly name: string } | { readonly ok: false; readonly failure: BackupFailure | "no_chats" };
export type RestoreResult = { readonly ok: true; readonly summary: ImportSummary } | { readonly ok: false; readonly failure: BackupFailure };

export interface BackupServices {
  /** Whether there is anything to back up yet. */
  readonly hasChats: () => Promise<boolean>;
  readonly save: (password: string) => Promise<SaveResult>;
  /** Looks at the file's start only (no password): says at once that it is not a backup, or from a newer OM. Null: worth asking for the password. */
  readonly check: (file: Uint8Array) => BackupFailure | null;
  readonly restore: (file: Uint8Array, password: string) => Promise<RestoreResult>;
}

/** The largest file the app reads: Core's limit (6 MiB) with a little room, checked before the file is read into memory. */
export const MAX_BACKUP_BYTES = 6 * 1024 * 1024 + 1024;

const pad = (n: number): string => String(n).padStart(2, "0");
/** `om-backup-2026-11-03.ombackup`: the date is local, so the name matches the person's calendar. */
export const backupFileName = (now: Date = new Date()): string => `om-backup-${String(now.getFullYear())}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}.ombackup`;

export function createBackupServices(init: { readonly rt: WasmCoreClient; readonly ctx: ProfileExecutionContext; readonly now?: () => Date }): BackupServices {
  const { rt, ctx } = init;
  const failure = (e: unknown): BackupFailure => backupRefusal(e) ?? "failed";
  return {
    hasChats: async () => (await rt.query({ kind: "GetConversationList", ctx })).length > 0,
    save: async (password) => {
      let key: Uint8Array | undefined;
      try {
        if ((await rt.query({ kind: "GetConversationList", ctx })).length === 0) return { ok: false, failure: "no_chats" };
        const plan = rt.backupPlan();
        key = await deriveBackupKey(password, plan.kdf, fromHex(plan.salt));
        if (key === undefined) return { ok: false, failure: "empty_password" };
        return { ok: true, bytes: rt.exportBackup(ctx, plan, key), name: backupFileName(init.now?.()) };
      } catch (e) {
        return { ok: false, failure: failure(e) };
      } finally {
        key?.fill(0);
      }
    },
    check: (file) => {
      try { rt.inspectBackup(file); return null; } catch (e) { return failure(e); }
    },
    restore: async (file, password) => {
      let key: Uint8Array | undefined;
      try {
        const head = rt.inspectBackup(file);
        key = await deriveBackupKey(password, head.kdf, fromHex(head.salt));
        if (key === undefined) return { ok: false, failure: "empty_password" };
        return { ok: true, summary: await rt.importBackup(ctx, file, key) };
      } catch (e) {
        return { ok: false, failure: failure(e) };
      } finally {
        key?.fill(0);
      }
    },
  };
}

/** Hands the browser a file to save. The object URL is released straight after the click. */
export function downloadBytes(bytes: Uint8Array, name: string): void {
  const url = URL.createObjectURL(new Blob([bytes as Uint8Array<ArrayBuffer>], { type: "application/octet-stream" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.rel = "noopener";
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => { URL.revokeObjectURL(url); }, 10_000);
}
