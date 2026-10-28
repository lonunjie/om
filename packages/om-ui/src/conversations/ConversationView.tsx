import { useRef, useState } from "react";
import type { ReactNode } from "react";
import type { ConversationId, ConversationTimelineItem, MessageId } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import { VisuallyHidden } from "../shell/primitives/VisuallyHidden";
import type { ConversationCopy } from "./copy";
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
}

export function ConversationView({ services, conversationId, copy, richTextFor, footer }: ConversationViewProps) {
  const state = useConversationView(services, conversationId);
  const retrying = useRef(new Set<MessageId>());
  const [, rerender] = useState(0);

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
        <Text as="h2" size="lg" weight="bold">{view.summary.displayName}</Text>
        <SecurityNotices notices={view.summary.notices} copy={copy.notices} />
        {view.summary.relationship === "active" ? null : <p className="om-conv-view__relationship" role="status" data-relationship={view.summary.relationship}>{copy.relationship[view.summary.relationship]}</p>}
      </header>
      <div role="log" aria-label={copy.view.label} className="om-conv-view__log">
        {view.timeline.length === 0 ? <p className="om-conv-view__empty">{copy.view.empty}</p> : (
          <ol className="om-conv-view__timeline">
            {view.timeline.map((item) => {
              const outgoing = item.direction === "outgoing";
              const delivery = item.direction === "outgoing" ? deliveryOf(item.delivery, ops.get(item.operationId)) : null;
              return (
                <li key={item.messageId} className="om-conv-view__item" data-direction={item.direction}>
                  <VisuallyHidden>{outgoing ? copy.view.outgoing : copy.view.incoming}</VisuallyHidden>
                  <RichText doc={(richTextFor ?? ((i) => plainTextToRichText(i.text)))(item)} copy={copy.richText} />
                  <span className="om-conv-view__foot">
                    <time dateTime={new Date(item.sentAt).toISOString()}>{copy.view.formatTime(item.sentAt)}</time>
                    {delivery ? <DeliveryIndicator delivery={delivery} copy={copy.delivery} /> : null}
                    {delivery?.kind === "rejected" && RETRYABLE_REASONS.has(delivery.reason)
                      ? <Button variant="quiet" disabled={retrying.current.has(item.messageId)} onClick={() => { retry(item.messageId); }}>{copy.view.retry}</Button>
                      : null}
                  </span>
                </li>
              );
            })}
          </ol>
        )}
      </div>
      {footer}
    </section>
  );
}
