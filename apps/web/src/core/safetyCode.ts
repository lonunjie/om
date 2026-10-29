// Which profiles still have a display-once safety code waiting in Core. Setup does not ask for it (ADR-0111: recovery is on
// by default and never a gate); the shell offers it until it is saved. Only the operation id is kept here: it is not secret,
// and Core alone holds the code and hands it out through the same query the setup step used.
import { asOperationId, type OperationId, type ProfileId } from "@om/platform-api";

const keyFor = (id: ProfileId): string => `om.safety-code-pending.${id}`;

export function readPendingSafetyCode(id: ProfileId): OperationId | null {
  try {
    const v = globalThis.localStorage.getItem(keyFor(id));
    return v === null || v === "" ? null : asOperationId(v);
  } catch {
    return null;
  }
}

export function savePendingSafetyCode(id: ProfileId, op: OperationId): void {
  try { globalThis.localStorage.setItem(keyFor(id), op); } catch { /* storage blocked: the offer is simply not shown again */ }
}

export function clearPendingSafetyCode(id: ProfileId): void {
  try { globalThis.localStorage.removeItem(keyFor(id)); } catch { /* nothing to clear */ }
}
