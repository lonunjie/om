import { useState } from "react";
import type { ReactNode } from "react";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import { MAX_AHEAD_MS, MIN_AHEAD_MS, canSchedule } from "./scheduledSend";
import type { ScheduleNotice, ScheduledItem } from "./scheduledSend";

export interface ScheduleCopy {
  readonly action: string;
  readonly title: string;
  readonly when: string;
  readonly confirm: string;
  readonly cancel: string;
  /** What the device must be doing for the message to go out: truthful, no exact-time promise. */
  readonly deviceNote: string;
  readonly pastTime: string;
  readonly tooMany: string;
  readonly listLabel: string;
  readonly item: (when: string) => string;
  readonly cancelItem: (excerpt: string) => string;
  readonly editItem: (excerpt: string) => string;
  readonly late: (excerpt: string) => string;
  readonly dropped: (excerpt: string) => string;
  readonly dismiss: string;
}

export interface ScheduleControl {
  readonly copy: ScheduleCopy;
  readonly items: readonly ScheduledItem[];
  readonly notices: readonly ScheduleNotice[];
  readonly now: () => number;
  /** Holds `text` for the chat until `dueAt`. */
  readonly add: (conversationId: string, text: string, dueAt: number) => void;
  readonly cancel: (id: string) => void;
  readonly dismiss: (id: string) => void;
}

const excerpt = (t: string): string => { const a = Array.from(t.replace(/\s+/g, " ").trim()); return a.length > 40 ? `${a.slice(0, 40).join("")}…` : a.join(""); };
const pad = (n: number): string => String(n).padStart(2, "0");
/** `datetime-local` value (this device's time) for a timestamp. */
export const toLocalInput = (ms: number): string => { const d = new Date(ms); return `${String(d.getFullYear())}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`; };
const when = (ms: number): string => new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(new Date(ms));

/** Scheduled messages and notices of one chat, shown above the composer. No permanent tab: it appears only when there is something to say. */
export function ScheduledList({ control, conversationId, onEdit }: { readonly control: ScheduleControl; readonly conversationId: string; readonly onEdit: (item: ScheduledItem) => void }): ReactNode {
  const c = control.copy;
  const mine = control.items.filter((i) => i.conversationId === conversationId);
  const notes = control.notices.filter((n) => n.conversationId === conversationId);
  if (mine.length === 0 && notes.length === 0) return null;
  return (
    <section className="om-scheduled" aria-label={c.listLabel}>
      {notes.map((n) => (
        <p key={n.id} className="om-scheduled__notice" role="status">
          {n.kind === "late" ? c.late(excerpt(n.text)) : c.dropped(excerpt(n.text))}{" "}
          <Button variant="quiet" onClick={() => { control.dismiss(n.id); }}>{c.dismiss}</Button>
        </p>
      ))}
      {mine.length === 0 ? null : (
        <ul className="om-scheduled__list">
          {mine.map((i) => (
            <li key={i.id} className="om-scheduled__item">
              <Text as="span" size="sm"><bdi>{excerpt(i.text)}</bdi> {"·"} {c.item(when(i.dueAt))}</Text>
              <Button variant="quiet" onClick={() => { onEdit(i); }}>{c.editItem(excerpt(i.text))}</Button>
              <Button variant="quiet" onClick={() => { control.cancel(i.id); }}>{c.cancelItem(excerpt(i.text))}</Button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

/** The small form that asks when. It states plainly what has to be true on this device for the message to go out. */
export function ScheduleForm({ control, conversationId, text, held, onDone, onCancel }: { readonly control: ScheduleControl; readonly conversationId: string; readonly text: string; readonly held: number; readonly onDone: () => void; readonly onCancel: () => void }): ReactNode {
  const c = control.copy;
  const start = control.now();
  const [value, setValue] = useState(() => toLocalInput(start + 3_600_000));
  const dueAt = new Date(value).getTime();
  const ok = canSchedule(text, dueAt, control.now(), held);
  return (
    <form className="om-schedule-form" aria-label={c.title} onSubmit={(e) => { e.preventDefault(); if (ok) { control.add(conversationId, text, dueAt); onDone(); } }}>
      <label className="om-schedule-form__field">{c.when}
        <input data-sheet-initial type="datetime-local" value={value} min={toLocalInput(start + MIN_AHEAD_MS)} max={toLocalInput(start + MAX_AHEAD_MS)} onChange={(e) => { setValue(e.target.value); }} />
      </label>
      <p className="om-schedule-form__note">{c.deviceNote}</p>
      {held >= 20 ? <p role="alert">{c.tooMany}</p> : !Number.isFinite(dueAt) || dueAt < start + MIN_AHEAD_MS ? <p role="status">{c.pastTime}</p> : null}
      <div className="om-schedule-form__actions">
        <Button variant="primary" type="submit" disabled={!ok}>{c.confirm}</Button>
        <Button variant="quiet" onClick={onCancel}>{c.cancel}</Button>
      </div>
    </form>
  );
}
