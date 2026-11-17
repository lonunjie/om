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
import { ContactDetail } from "./ContactDetail";
import { shownName } from "./shownName";
import { PatternAvatar } from "../groups/PatternAvatar";
import type { SameName } from "../groups/sameName";
import { contactsLine, type SameNameCopy } from "./nameCues";
import { MessageAttachments } from "./Attachments";
import { DeliveryIndicator, RETRYABLE_REASONS, deliveryOf } from "./delivery";
import { useConversationView } from "./hooks";
import { DeleteConfirm, EditForm, Quote, ReactionChips, ReactionControl, gistOf } from "./chatContext";
import { PeerRecovery } from "./PeerRecovery";
import { SecurityNotices } from "./notices";
import type { RecoveryContactCopy } from "../recovery/copy";
import { RichText, plainTextToRichText } from "./richtext";
import { firstOutcome, resolveIntent, type ConversationServices } from "./services";
import "./conversations.css";

/** What the composer needs to quote a message: its id and the bounded start of its text (read from the timeline, never from the wire). */
export interface ReplyTarget {
  readonly messageId: MessageId;
  readonly preview: string;
  readonly hasAttachment: boolean;
  /** The original is our own message. */
  readonly outgoing: boolean;
  /** Who wrote the original, already as the screen shows them (a group names the person; a chat knows it already). */
  readonly who?: string;
}

export interface ConversationViewProps {
  readonly services: ConversationServices;
  readonly conversationId: ConversationId;
  readonly copy: ConversationCopy;
  /** Words for a contact's recovery of their account (ADR-0190). Without it nothing is said here, but Core still refuses to send to them. */
  readonly recoveryCopy?: RecoveryContactCopy;
  /** Supplies the RichTextV1 document (untrusted) for an item; defaults to its plain text as paragraphs. */
  readonly richTextFor?: (item: ConversationTimelineItem) => unknown;
  /** Slot below the timeline, normally the Composer. */
  readonly footer?: ReactNode;
  /** With it the header shows a Back control (compact screens, where the conversation replaces the list). */
  readonly onBack?: () => void;
  /** A message to bring into view and mark briefly (a search hit). Ignored if the message is not in this conversation. */
  readonly focusMessageId?: MessageId | undefined;
  /** This contact's place among contacts who show the same name or one that looks alike, and the words for it (ADR-0197). Null or missing: no cue. */
  readonly sameName?: SameName | null;
  readonly sameNameCopy?: SameNameCopy;
  /** Starts a reply to a message (the composer owns the quote chip). Without it no Reply control is offered. */
  readonly onReply?: (target: ReplyTarget) => void;
  /** The message a reply is being written to, marked in the timeline. */
  readonly replyingTo?: MessageId | null;
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

/** A short spoken name for a message, so "Reply" says which one. */
const messageGist = (item: ConversationTimelineItem, copy: ConversationCopy): string => gistOf(item.text, copy.reply.attachment);

export function ConversationView({ services, conversationId, copy, recoveryCopy, richTextFor, footer, onBack, focusMessageId, sameName, sameNameCopy, onReply, replyingTo }: ConversationViewProps) {
  const state = useConversationView(services, conversationId);
  const retrying = useRef(new Set<MessageId>());
  const [, rerender] = useState(0);
  const [contactOpen, setContactOpen] = useState(false);
  const [picking, setPicking] = useState<MessageId | null>(null);
  const [reactFailed, setReactFailed] = useState(false);
  const [editing, setEditing] = useState<{ readonly id: MessageId; readonly text: string } | null>(null);
  const [deleting, setDeleting] = useState<MessageId | null>(null);
  const [changeFailed, setChangeFailed] = useState<string | null>(null);
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

  // A search hit opens the chat at the exact message: scroll it to the middle, mark it for a moment (no motion if reduced), once per target.
  const [marked, setMarked] = useState<MessageId | null>(null);
  const shownFocus = useRef<MessageId | null>(null);
  const hasFocus = focusMessageId !== undefined && timeline !== null && timeline.some((i) => i.messageId === focusMessageId);
  useEffect(() => {
    if (focusMessageId === undefined) { shownFocus.current = null; return; }
    if (!hasFocus || shownFocus.current === focusMessageId) return;
    shownFocus.current = focusMessageId;
    const el = document.getElementById(`om-msg-${focusMessageId}`);
    if (el === null) return;
    const calm = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (typeof el.scrollIntoView === "function") el.scrollIntoView({ block: "center", behavior: calm ? "auto" : "smooth" });
    setMarked(focusMessageId);
    const t = setTimeout(() => { setMarked(null); }, 2400);
    return () => { clearTimeout(t); setMarked(null); };
  }, [focusMessageId, hasFocus]);

  /** Edit or delete one of our own messages. A refusal is said once; the message stays as it was. */
  const change = (actionId: "edit-message" | "delete-message", messageId: MessageId, text: string | null, failed: string): void => {
    const r = resolveIntent(services, actionId, { conversationId, messageId, ...(text === null ? {} : { text }) });
    if (r.status !== "intent") { setChangeFailed(failed); return; }
    setChangeFailed(null);
    const fail = (): void => { setChangeFailed(failed); };
    services.client.dispatch(r.intent).then((h) => firstOutcome(services, h.operationId)).then((o) => {
      if (o.kind === "Rejected") fail(); else { setEditing(null); setDeleting(null); }
    }, fail);
  };

  /** Set or remove our reaction on a message. A refusal is said once, plainly; the chip simply does not change. */
  const react = (messageId: MessageId, reaction: string, present: boolean): void => {
    const r = resolveIntent(services, "react-message", { conversationId, messageId, reaction, present });
    if (r.status !== "intent") return;
    setReactFailed(false);
    setPicking(null);
    const fail = (): void => { setReactFailed(true); };
    services.client.dispatch(r.intent).then((h) => firstOutcome(services, h.operationId)).then((o) => { if (o.kind === "Rejected") fail(); }, fail);
  };

  /** Tap the quoted block: scroll to the original and mark it for a moment (nothing moves if reduced motion is asked for). */
  const jumpTo = (target: MessageId): void => {
    const el = document.getElementById(`om-msg-${target}`);
    if (el === null) return;
    const calm = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (typeof el.scrollIntoView === "function") el.scrollIntoView({ block: "center", behavior: calm ? "auto" : "smooth" });
    setMarked(target);
    setTimeout(() => { setMarked((m) => (m === target ? null : m)); }, 2400);
  };

  if (state.status === "loading") return <div className="om-conv-view" aria-busy="true" />;
  if (state.status === "failed") return <div className="om-conv-view"><p role="alert">{copy.view.loadFailed}</p></div>;
  const { view, ops } = state.value;
  const cue = sameName ?? null;

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
    <section className="om-conv-view" aria-label={shownName(view.summary)}>
      <header className="om-conv-view__header">
        {onBack ? <IconButton className="om-conv-view__back" label={copy.view.back} icon={<Icon name="back" />} onClick={onBack} /> : null}
        {cue === null ? <Avatar name={shownName(view.summary)} /> : <PatternAvatar seed={conversationId} />}
        <div className="om-conv-view__who">
          <Text as="h2" size="lg" weight="bold">{cue === null || sameNameCopy === undefined ? shownName(view.summary) : <bdi>{sameNameCopy.of(shownName(view.summary), cue.index, cue.of)}</bdi>}</Text>
          {cue === null || sameNameCopy === undefined ? null : <Text as="p" size="sm" tone="secondary" className="om-conv-view__samename" data-samename={cue.kind}><bdi>{contactsLine(sameNameCopy, cue)}</bdi></Text>}
          {view.summary.nickname === undefined ? null : <Text as="p" size="sm" tone="secondary" className="om-conv-view__original"><bdi>{view.summary.displayName}</bdi></Text>}
        </div>
        <IconButton className="om-conv-view__contact" label={copy.contact.open(shownName(view.summary))} icon={<Icon name="user" />} onClick={() => { setContactOpen(true); }} />
      </header>
      {view.summary.relationship === "active" && view.summary.notices.length === 0 && (view.summary.peerRecovery === undefined || recoveryCopy === undefined) ? null : (
        <div className="om-conv-view__notes">
          {view.summary.peerRecovery !== undefined && recoveryCopy !== undefined
            ? <PeerRecovery services={services} conversationId={conversationId} name={shownName(view.summary)} recovery={view.summary.peerRecovery} copy={recoveryCopy} />
            : null}
          {view.summary.relationship === "active" ? null : <p className="om-conv-view__relationship" role="status" data-relationship={view.summary.relationship}>{copy.relationship[view.summary.relationship]}</p>}
          <SecurityNotices notices={view.summary.notices} copy={copy.notices} />
        </div>
      )}
      {changeFailed === null ? null : <p className="om-conv-view__relationship" role="status">{changeFailed}</p>}
      {reactFailed ? <p className="om-conv-view__relationship" role="status">{copy.react.failed}</p> : null}
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
                <li key={item.messageId} id={`om-msg-${item.messageId}`} className="om-conv-view__item" data-found={marked === item.messageId ? "true" : undefined} data-direction={item.direction} data-joined={joinsPrev ? "true" : undefined} data-last={lastOfRun ? "true" : undefined}>
                  <VisuallyHidden>{outgoing ? copy.view.outgoing : copy.view.incoming}</VisuallyHidden>
                  <div className="om-conv-view__row">
                  <div className="om-conv-view__bubble" data-state={delivery?.kind}>
                    {item.reply === undefined ? null : <Quote reply={item.reply} name={shownName(view.summary)} copy={copy.reply} onJump={jumpTo} />}
                    {editing?.id === item.messageId ? (
                      <EditForm copy={copy.change} text={editing.text} onChange={(next) => { setEditing({ id: item.messageId, text: next }); }} onSave={() => { change("edit-message", item.messageId, editing.text, copy.change.editFailed); }} onCancel={() => { setEditing(null); }} />
                    ) : <RichText doc={(richTextFor ?? ((i) => plainTextToRichText(i.text)))(item)} copy={copy.richText} />}
                    <MessageAttachments services={services} item={item} copy={copy.attachments} />
                  </div>
                  {view.summary.relationship !== "active" || (item.direction === "outgoing" && delivery !== null && delivery.kind !== "sent" && delivery.kind !== "delivered") ? null : (
                    <ReactionControl
                      copy={copy.react} gist={messageGist(item, copy)} open={picking === item.messageId}
                      onToggle={() => { setPicking((p) => (p === item.messageId ? null : item.messageId)); }} onClose={() => { setPicking(null); }}
                      onPick={(key, present) => { react(item.messageId, key, present); }} isMine={(key) => item.reactions?.some((r) => r.key === key && r.mine) ?? false}
                    />
                  )}
                  {outgoing && view.summary.relationship === "active" && (delivery?.kind === "sent" || delivery?.kind === "delivered") ? (
                    <>
                      {item.presentation?.locationCard === undefined ? <IconButton className="om-conv-view__reply" label={`${copy.change.edit}: ${messageGist(item, copy)}`} icon={<Icon name="edit" size={18} />} onClick={() => { setDeleting(null); setEditing({ id: item.messageId, text: item.text }); }} /> : null}
                      <IconButton className="om-conv-view__reply" label={`${copy.change.delete}: ${messageGist(item, copy)}`} icon={<Icon name="trash" size={18} />} onClick={() => { setEditing(null); setDeleting(item.messageId); }} />
                    </>
                  ) : null}
                  {onReply === undefined || view.summary.relationship !== "active" || (item.direction === "outgoing" && delivery !== null && delivery.kind !== "sent" && delivery.kind !== "delivered") ? null : (
                    <IconButton
                      className="om-conv-view__reply" label={`${copy.reply.action}: ${messageGist(item, copy)}`} icon={<Icon name="reply" size={18} />}
                      aria-pressed={replyingTo === item.messageId}
                      onClick={() => { onReply({ messageId: item.messageId, preview: Array.from(item.text).slice(0, 140).join(""), hasAttachment: (item.attachments?.length ?? 0) > 0, outgoing: outgoing }); }}
                    />
                  )}
                  </div>
                  {deleting === item.messageId ? <DeleteConfirm copy={copy.change} onConfirm={() => { change("delete-message", item.messageId, null, copy.change.deleteFailed); }} onCancel={() => { setDeleting(null); }} /> : null}
                  <ReactionChips copy={copy.react} reactions={item.reactions ?? []} onPick={(key, present) => { react(item.messageId, key, present); }} />
                  {lastOfRun || unsettled ? (
                    <span className="om-conv-view__foot">
                      <time dateTime={new Date(item.sentAt).toISOString()}>{copy.view.formatTime(item.sentAt)}</time>
                      {item.edited === true ? <span className="om-conv-view__edited">{copy.change.edited}</span> : null}
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
      {contactOpen ? <ContactDetail services={services} summary={view.summary} copy={copy.contact} onClose={() => { setContactOpen(false); }} /> : null}
    </section>
  );
}
