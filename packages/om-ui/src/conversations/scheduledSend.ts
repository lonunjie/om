// SPDX-License-Identifier: MPL-2.0
/**
 * Scheduled send, device-local (ADR-0224; baseline "Scheduled send", ADR-0156/0157). The queue holds compose intent only: the words and the
 * time. Nothing is encrypted, signed or addressed until the moment of sending, when the item becomes an ordinary send (its time, epoch and
 * recipients are those of that moment, never the moment of scheduling). A device that was not running at the time sends it at the next
 * opportunity and tells its owner it went out late; an item that can no longer be sent is dropped with a notice and never moves to another device.
 */
import { useEffect, useRef, useState } from "react";
import type { ConversationId, OutcomeClass } from "@om/platform-api";
import { resolveIntent, settledOutcome } from "./services";
import type { ConversationServices } from "./services";

export interface ScheduledItem {
  readonly id: string;
  readonly conversationId: ConversationId;
  readonly text: string;
  /** When to send (ms since the epoch, this device's clock). */
  readonly dueAt: number;
  readonly createdAt: number;
}

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
            try { const handle = await services.client.dispatch(r.intent); outcome = await settledOutcome(services, handle.operationId); } catch { continue; }
          }
          if (!isLive()) return;
          if (!isSent(outcome) && !isGone(outcome)) continue; // needs the person: keep it, try again at the next opportunity
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
