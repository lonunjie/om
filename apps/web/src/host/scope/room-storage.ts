// SPDX-License-Identifier: MPL-2.0
import type { ConversationId, EncryptedKv, ProfileExecutionContext, ProfileId } from "@om/platform-api";
import type { RoomSnapshotKind, ScopeEngines } from "../../../../../packages/om-core-wasm/src/scope-engines";

type PendingRecord = { readonly id: string; readonly conversationId: ConversationId };
type Purpose = "pending" | "snapshot";
const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });
const pendingLimit = 4096;
const snapshotLimit = (512 * 1024 + 32) * 2;

function key(profile: ProfileId, room: ConversationId, kind: RoomSnapshotKind, purpose: Purpose): string {
  return `scope/v1/${encodeURIComponent(profile)}/${kind}/${purpose}/${encodeURIComponent(room)}`;
}

const queues = new WeakMap<EncryptedKv, Map<string, Promise<unknown>>>();

/** The engineering host owns one writer for each profile and uses the same vault binding for its stores. */
export class DurablePendingStore<T extends PendingRecord> {
  constructor(private readonly kv: EncryptedKv, private readonly kind: RoomSnapshotKind) {}

  async load(profile: ProfileId, room: ConversationId): Promise<T | null> {
    const result = await this.kv.get(key(profile, room, this.kind, "pending"));
    if (!result.ok) throw new Error("Pending Room record unavailable");
    if (result.value === undefined) return null;
    if (result.value.length > pendingLimit) throw new Error("Invalid pending Room record");
    const record = JSON.parse(decoder.decode(result.value)) as T | null;
    if (record === null || record.conversationId !== room || typeof record.id !== "string") {
      throw new Error("Invalid pending Room record");
    }
    return record;
  }

  reserve(profile: ProfileId, operation: T): Promise<boolean> {
    const id = key(profile, operation.conversationId, this.kind, "pending");
    return this.exclusive(id, async () => {
      if (await this.load(profile, operation.conversationId) !== null) return false;
      const value = encoder.encode(JSON.stringify(operation));
      if (value.length > pendingLimit || !(await this.kv.put(id, value)).ok) {
        throw new Error("Could not reserve pending Room record");
      }
      return true;
    });
  }

  clear(profile: ProfileId, room: ConversationId, operationId: string): Promise<void> {
    const id = key(profile, room, this.kind, "pending");
    return this.exclusive(id, async () => {
      if ((await this.load(profile, room))?.id === operationId && !(await this.kv.delete(id)).ok) {
        throw new Error("Could not clear pending Room record");
      }
    });
  }

  private async exclusive<R>(id: string, work: () => Promise<R>): Promise<R> {
    let queue = queues.get(this.kv);
    if (queue === undefined) {
      queue = new Map();
      queues.set(this.kv, queue);
    }
    const own = (queue.get(id) ?? Promise.resolve()).catch(() => undefined).then(work);
    queue.set(id, own);
    try {
      return await own;
    } finally {
      if (queue.get(id) === own) queue.delete(id);
    }
  }
}

export class RoomSnapshotStore {
  constructor(private readonly kv: EncryptedKv) {}

  async load(profile: ProfileId, room: ConversationId, kind: RoomSnapshotKind): Promise<string | null> {
    const result = await this.kv.get(key(profile, room, kind, "snapshot"));
    if (!result.ok) throw new Error("Room snapshot unavailable");
    if (result.value === undefined) return null;
    if (result.value.length > snapshotLimit) throw new Error("Room snapshot exceeds limit");
    return decoder.decode(result.value);
  }

  async save(profile: ProfileId, room: ConversationId, kind: RoomSnapshotKind, snapshot: string): Promise<void> {
    const value = encoder.encode(snapshot);
    if (value.length > snapshotLimit || !(await this.kv.put(key(profile, room, kind, "snapshot"), value)).ok) {
      throw new Error("Could not persist Room snapshot");
    }
  }
}

export interface RoomPersistence {
  restore(room: ConversationId, hasPending: boolean): Promise<void>;
  save(room: ConversationId): Promise<void>;
}

export interface RoomStorageOptions {
  readonly kv: EncryptedKv;
  /** Re-observe current signed heads through Core before importing cached Room entries. */
  readonly observeCurrentHeads: (ctx: ProfileExecutionContext, room: ConversationId, control: ScopeEngines["roomControl"]) => Promise<void>;
}

export function roomPersistence(
  ctx: ProfileExecutionContext,
  engines: ScopeEngines,
  store: RoomSnapshotStore,
  kind: RoomSnapshotKind,
  observeCurrentHeads: RoomStorageOptions["observeCurrentHeads"],
  open: (handle: string) => ReturnType<ScopeEngines["bridgeRoom"]["open"]>,
): RoomPersistence {
  const ready = new Set<ConversationId>();
  const handle = (room: ConversationId) => JSON.stringify([ctx.profileId, room]);
  return {
    async restore(room, hasPending) {
      if (ready.has(room)) return;
      const snapshot = await store.load(ctx.profileId, room, kind);
      if (snapshot === null && hasPending) throw new Error("Pending Room snapshot unavailable");
      await observeCurrentHeads(ctx, room, engines.roomControl);
      if (snapshot === null) {
        const current = engines.roomSnapshot.export(handle(room), kind, ctx);
        if ("refused" in current && (current.refused !== "unknown_handle" || "refused" in open(handle(room)))) {
          throw new Error("Could not open Room");
        }
      } else if ("refused" in engines.roomSnapshot.import(handle(room), kind, snapshot, ctx)) {
        throw new Error("Could not restore Room");
      }
      ready.add(room);
    },
    async save(room) {
      const result = engines.roomSnapshot.export(handle(room), kind, ctx);
      if ("refused" in result) throw new Error("Could not export Room snapshot");
      await store.save(ctx.profileId, room, kind, result.snapshot);
    },
  };
}
