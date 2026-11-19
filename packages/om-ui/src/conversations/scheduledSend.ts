// SPDX-License-Identifier: MPL-2.0
/**
 * Scheduled send, device-local (ADR-0224; baseline "Scheduled send", ADR-0156/0157). The queue holds compose intent only: the words and the
 * time. Nothing is encrypted, signed or addressed until the moment of sending, when the item becomes an ordinary send (its time, epoch and
 * recipients are those of that moment, never the moment of scheduling). A device that was not running at the time sends it at the next
 * opportunity and tells its owner it went out late; an item that can no longer be sent is dropped with a notice and never moves to another device.
 */
import { useEffect, useRef, useState } from "react";
import { asOperationId, type ConversationId, type OperationId, type OutcomeClass } from "@om/platform-api";
import { resolveIntent, settledOutcome } from "./services";
import type { ConversationServices } from "./services";

export interface ScheduledItem {
  readonly id: string;
  readonly conversationId: ConversationId;
  readonly text: string;
  /** When to send (ms since the epoch, this device's clock). */
  readonly dueAt: number;
  readonly createdAt: number;
  /**
   * Which send attempt the item is on (absent = 0). It is the durable journal of the release: the attempt's Core operation id is derived
   * from it (`scheduledOperationId`), so a restart after the send was handed to Core but before the item left the queue repeats the exact
   * same operation, which Core answers from its record instead of sending again. The attempt advances (and is saved) only when Core
   * answered that the send needs the person, which makes the next try a new operation.
   */
  readonly attempt?: number;
}

/** The longest queued-item id the operation id below can carry (Core bounds an operation id at 128 bytes). */
export const MAX_SCHEDULED_ID_CHARS = 64;

/** The stable Core operation id for one send attempt of a queued item: the same item and attempt always give the same id. */
export const scheduledOperationId = (item: Pick<ScheduledItem, "id" | "attempt">): OperationId => asOperationId(`sched:${item.id}:${String(item.attempt ?? 0)}`);

export type ScheduleNoticeKind = "late" | "dropped";
export interface ScheduleNotice { readonly id: string; readonly kind: ScheduleNoticeKind; readonly conversationId: ConversationId; readonly text: string }

/** An item that goes out this much after its time is "sent late". */
export const LATE_AFTER_MS = 120_000;
export const MAX_SCHEDULED = 20;
/** Furthest ahead a message can be scheduled. */
export const MAX_AHEAD_MS = 366 * 24 * 3_600_000;
export const MIN_AHEAD_MS = 30_000;
export const MAX_SCHEDULED_CHARS = 8000;

export const isDue = (item: ScheduledItem, now: number): boolean => item.dueAt <= now;

/** Whether `text` may be scheduled for `dueAt` now. */
export function canSchedule(text: string, dueAt: number, now: number, held: number): boolean {
  return text.trim() !== "" && text.length <= MAX_SCHEDULED_CHARS && Number.isFinite(dueAt) && dueAt >= now + MIN_AHEAD_MS && dueAt <= now + MAX_AHEAD_MS && held < MAX_SCHEDULED;
}

const isGone = (o: OutcomeClass): boolean => o.kind === "Rejected";
const isSent = (o: OutcomeClass): boolean => o.kind === "Committed" || o.kind === "PendingAuthorization" || o.kind === "PendingNetwork" || o.kind === "PendingReconciliation";

export interface ScheduledSender {
  readonly notices: readonly ScheduleNotice[];
  readonly dismiss: (id: string) => void;
}

/**
 * Releases due items into the ordinary send path, at the next opportunity: on a timer, when the page is shown again and when the network returns.
 * An item stays queued while the send needs the person (`NeedsUserAction`) and is dropped, with a notice, when the send is refused for good.
 */
export function useScheduledSender(o: { readonly services: ConversationServices; readonly items: readonly ScheduledItem[]; readonly onItems: (next: readonly ScheduledItem[]) => void; readonly now?: () => number; readonly intervalMs?: number }): ScheduledSender {
  const { services, items, onItems } = o;
  const now = o.now ?? Date.now;
  const [notices, setNotices] = useState<readonly ScheduleNotice[]>([]);
  const itemsRef = useRef(items);
  itemsRef.current = items;
  const running = useRef(false);
  const live = useRef(true);
  const isLive = (): boolean => live.current;
  useEffect(() => { live.current = true; return () => { live.current = false; }; }, []);
  useEffect(() => {
    const release = async (): Promise<void> => {
      if (running.current) return;
      running.current = true;
      try {
        for (const item of [...itemsRef.current].sort((a, b) => a.dueAt - b.dueAt)) {
          if (!isLive() || !isDue(item, now())) continue;
          const late = now() - item.dueAt > LATE_AFTER_MS;
          const r = resolveIntent(services, "send-message", { conversationId: item.conversationId, text: item.text });
          let outcome: OutcomeClass = { kind: "Rejected", reason: "invalid" };
          if (r.status === "intent") {
            // The exact retry of a send already handed to Core (a restart before the item left the queue) lands on Core's record of it: one message.
            try { const handle = await services.client.dispatch({ ...r.intent, operationId: scheduledOperationId(item) }); outcome = await settledOutcome(services, handle.operationId); } catch { continue; }
          }
          if (!isLive()) return;
          if (!isSent(outcome) && !isGone(outcome)) {
            // Needs the person: keep it and try again at the next opportunity, as a new operation (Core still holds the answer to this one).
            itemsRef.current = itemsRef.current.map((x) => (x.id === item.id ? { ...x, attempt: (x.attempt ?? 0) + 1 } : x));
            onItems(itemsRef.current);
            continue;
          }
          itemsRef.current = itemsRef.current.filter((x) => x.id !== item.id);
          onItems(itemsRef.current);
          if (isGone(outcome)) setNotices((n) => [...n, { id: `n-${item.id}`, kind: "dropped", conversationId: item.conversationId, text: item.text }]);
          else if (late) setNotices((n) => [...n, { id: `n-${item.id}`, kind: "late", conversationId: item.conversationId, text: item.text }]);
        }
      } finally { running.current = false; }
    };
    void release();
    const timer = setInterval(() => { void release(); }, o.intervalMs ?? 15_000);
    const wake = (): void => { if (typeof document === "undefined" || document.visibilityState !== "hidden") void release(); };
    document.addEventListener("visibilitychange", wake);
    window.addEventListener("online", wake);
    return () => { clearInterval(timer); document.removeEventListener("visibilitychange", wake); window.removeEventListener("online", wake); };
  }, [services, o.intervalMs]);
  return { notices, dismiss: (id) => { setNotices((n) => n.filter((x) => x.id !== id)); } };
}
