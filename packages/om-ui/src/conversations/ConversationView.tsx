import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import type { ConversationId, ConversationTimelineItem, MessageId } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Icon } from "../shell/primitives/icons";
import { IconButton } from "../shell/primitives/IconButton";
import { Text } from "../shell/primitives/Text";
import { VisuallyHidden } from "../shell/primitives/VisuallyHidden";
import type { ConversationCopy } from "./copy";
import { Avatar } from "./Avatar";
import { DeliveryIndicator, RETRYABLE_REASONS, deliveryOf } from "./delivery";
import { useConversationView } from "./hooks";
import { SecurityNotices } from "./notices";
import { RichText, plainTextToRichText } from "./richtext";
import { resolveIntent, type ConversationServices } from "./services";
import "./conversations.css";

export interface ConversationViewProps {
  readonly services: ConversationServices;
  readonly conversationId: ConversationId;
  readonly copy: ConversationCopy;
  /** Supplies the RichTextV1 document (untrusted) for an item; defaults to its plain text as paragraphs. */
  readonly richTextFor?: (item: ConversationTimelineItem) => unknown;
  /** Slot below the timeline, normally the Composer. */
  readonly footer?: ReactNode;
  /** With it the header shows a Back control (compact screens, where the conversation replaces the list). */
  readonly onBack?: () => void;
}

/** Messages from one person within this gap read as one run: the time (and the tail of the bubble) is shown once. */
const RUN_GAP_MS = 5 * 60 * 1000;
const NEAR_BOTTOM_PX = 160;

/** The nearest ancestor that scrolls, which is the pane around the conversation, not the conversation itself. */
const scrollerOf = (el: HTMLElement): HTMLElement | null => {
  for (let p = el.parentElement; p !== null; p = p.parentElement) {
    const oy = getComputedStyle(p).overflowY;
    if (oy === "auto" || oy === "scroll") return p;
  }
  return null;
};

export function ConversationView({ services, conversationId, copy, richTextFor, footer, onBack }: ConversationViewProps) {
  const state = useConversationView(services, conversationId);
  const retrying = useRef(new Set<MessageId>());
  const [, rerender] = useState(0);
  const end = useRef<HTMLDivElement>(null);
  const seen = useRef<{ readonly count: number } | null>(null);
  const timeline = state.status === "loading" || state.status === "failed" ? null : state.value.view.timeline;
  const count = timeline?.length ?? -1;
  const newest = timeline?.[timeline.length - 1];
  const newestOutgoing = newest?.direction === "outgoing";
  // Opening a conversation shows its newest message; a new one follows if it is yours, or if you were already reading the end.
  useEffect(() => {
    const el = end.current;
    if (el === null || count < 0) return;
    const first = seen.current === null;
    const grew = seen.current !== null && count > seen.current.count;
    seen.current = { count };
    if (!first && !grew) return;
    const scroller = scrollerOf(el);
    const nearEnd = scroller === null || scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < NEAR_BOTTOM_PX + el.offsetHeight;
    if (!first && !newestOutgoing && !nearEnd) return;
    if (typeof el.scrollIntoView !== "function") return;
    const calm = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
    el.scrollIntoView({ block: "end", behavior: first || calm ? "auto" : "smooth" });
  }, [count, newestOutgoing]);

  if (state.status === "loading") return <div className="om-conv-view" aria-busy="true" />;
  if (state.status === "failed") return <div className="om-conv-view"><p role="alert">{copy.view.loadFailed}</p></div>;
  const { view, ops } = state.value;

  const retry = (messageId: MessageId): void => {
    if (retrying.current.has(messageId)) return;
    const r = resolveIntent(services, "retry-message", { messageId });
    if (r.status !== "intent") return;
    retrying.current.add(messageId);
    rerender((n) => n + 1);
    // Same logical message only: the intent carries the MessageId, never new text.
    const done = (): void => { retrying.current.delete(messageId); rerender((n) => n + 1); };
    services.client.dispatch(r.intent).then(done, done);
  };

  return (
    <section className="om-conv-view" aria-label={view.summary.displayName}>
      <header className="om-conv-view__header">
        {onBack ? <IconButton className="om-conv-view__back" label={copy.view.back} icon={<Icon name="back" />} onClick={onBack} /> : null}
        <Avatar name={view.summary.displayName} />
        <div className="om-conv-view__who">
          <Text as="h2" size="lg" weight="bold">{view.summary.displayName}</Text>
        </div>
      </header>
      {view.summary.relationship === "active" && view.summary.notices.length === 0 ? null : (
        <div className="om-conv-view__notes">
          {view.summary.relationship === "active" ? null : <p className="om-conv-view__relationship" role="status" data-relationship={view.summary.relationship}>{copy.relationship[view.summary.relationship]}</p>}
          <SecurityNotices notices={view.summary.notices} copy={copy.notices} />
        </div>
      )}
      <div role="log" aria-label={copy.view.label} className="om-conv-view__log">
        {view.timeline.length === 0 ? <p className="om-conv-view__empty">{copy.view.empty}</p> : (
          <ol className="om-conv-view__timeline">
            {view.timeline.map((item, i) => {
              const outgoing = item.direction === "outgoing";
              const delivery = item.direction === "outgoing" ? deliveryOf(item.delivery, ops.get(item.operationId)) : null;
              const prev = view.timeline[i - 1];
              const next = view.timeline[i + 1];
              const joinsPrev = prev !== undefined && prev.direction === item.direction && item.sentAt - prev.sentAt < RUN_GAP_MS;
              const lastOfRun = next === undefined || next.direction !== item.direction || next.sentAt - item.sentAt >= RUN_GAP_MS;
              // A message that is not settled (sending, waiting, refused) always says so, even in the middle of a run.
              const unsettled = delivery !== null && delivery.kind !== "sent" && delivery.kind !== "delivered";
              return (
                <li key={item.messageId} className="om-conv-view__item" data-direction={item.direction} data-joined={joinsPrev ? "true" : undefined} data-last={lastOfRun ? "true" : undefined}>
                  <VisuallyHidden>{outgoing ? copy.view.outgoing : copy.view.incoming}</VisuallyHidden>
                  <div className="om-conv-view__bubble" data-state={delivery?.kind}>
                    <RichText doc={(richTextFor ?? ((i) => plainTextToRichText(i.text)))(item)} copy={copy.richText} />
                  </div>
                  {lastOfRun || unsettled ? (
                    <span className="om-conv-view__foot">
                      <time dateTime={new Date(item.sentAt).toISOString()}>{copy.view.formatTime(item.sentAt)}</time>
                      {delivery ? <DeliveryIndicator delivery={delivery} copy={copy.delivery} /> : null}
                      {delivery?.kind === "rejected" && view.summary.relationship !== "restored" && RETRYABLE_REASONS.has(delivery.reason)
                        ? <Button variant="quiet" disabled={retrying.current.has(item.messageId)} onClick={() => { retry(item.messageId); }}>{copy.view.retry}</Button>
                        : null}
                    </span>
                  ) : null}
                </li>
              );
            })}
          </ol>
        )}
        <div ref={end} className="om-conv-view__end" aria-hidden="true" />
      </div>
      {footer}
    </section>
  );
}
