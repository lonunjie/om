import { PollCard, PollForm } from "./Polls";
import { RichText } from "../conversations/richtext";
import { textWithMentions } from "../conversations/mentions";
import { useMentions } from "../conversations/MentionPicker";
import type { MentionCandidate } from "../conversations/mentions";
import { useMemo } from "react";
import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import { asMessageId } from "@om/platform-api";
import type { ConversationSummary, GroupId, GroupMessageView, GroupNoticeView, GroupRejoinReviewView, MessageId, ReplyView } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Icon } from "../shell/primitives/icons";
import { IconButton } from "../shell/primitives/IconButton";
import { Text } from "../shell/primitives/Text";
import { VisuallyHidden } from "../shell/primitives/VisuallyHidden";
import { Sheet } from "../shell/layout/Sheet";
import { Avatar } from "../conversations/Avatar";
import type { ConversationServices } from "../conversations/services";
import type { ReplyTarget } from "../conversations/ConversationView";
import { DeleteConfirm, EditForm, Quote, ReactionChips, ReactionControl, ReplyChip, gistOf } from "../conversations/chatContext";
import type { GroupsCopy } from "./copy";
import { GroupMembers } from "./GroupMembers";
import { GroupRejoinReview } from "./GroupRejoin";
import { GroupStatus } from "./GroupStatus";
import { RoomSettings } from "./RoomSettings";
import { useGroupChat, usePeopleNames, type GroupEventsState } from "./hooks";
import { groupApiOf, runGroupAction } from "./services";
import "../conversations/conversations.css";
import "./groups.css";

const RUN_GAP_MS = 5 * 60 * 1000;

export interface GroupChatProps {
  readonly services: ConversationServices;
  readonly copy: GroupsCopy;
  readonly groupId: GroupId;
  readonly chats: readonly ConversationSummary[];
  /** Slot below the messages, normally the GroupComposer. */
  readonly footer?: ReactNode;
  readonly onBack?: () => void;
  /** A search hit: the message to bring into view and mark for a moment, once (it opens where the person found it, as a chat does). */
  readonly focusMessageId?: string | undefined;
  /** The person left (or is no longer in) the group. */
  readonly onLeft: () => void;
  /** Bump to re-read after a step made elsewhere (the composer). */
  readonly refreshKey?: number;
  readonly onChanged?: () => void;
  /** The background states of the group carriage (from `useGroupEvents`); without it no state line is shown. */
  readonly events?: GroupEventsState;
  readonly rejoinReviews?: readonly GroupRejoinReviewView[];
  /** Starts a reply to a message (the composer owns the quote chip). Without it no Reply control is offered. */
  readonly onReply?: (target: ReplyTarget) => void;
  /** The message a reply is being written to, marked in the timeline. */
  readonly replyingTo?: MessageId | null;
}

type Entry = { readonly kind: "message"; readonly m: GroupMessageView } | { readonly kind: "notice"; readonly n: GroupNoticeView };

/** Messages and settings notices in time order; at the same time a message comes first. */
function timeline(messages: readonly GroupMessageView[], notices: readonly GroupNoticeView[]): readonly Entry[] {
  const all: Entry[] = [...messages.map((m): Entry => ({ kind: "message", m })), ...notices.map((n): Entry => ({ kind: "notice", n }))];
  const at = (e: Entry): number => (e.kind === "message" ? e.m.atMs : e.n.atMs);
  return all.map((e, i) => ({ e, i })).sort((a, b) => at(a.e) - at(b.e) || a.i - b.i).map((x) => x.e);
}

/** A group is an ordinary conversation with more people: the same header, bubbles and rhythm as a chat, plus who said it and a people sheet. */
export function GroupChat({ services, copy, groupId, chats, footer, onBack, onLeft, refreshKey = 0, onChanged, events, rejoinReviews = [], onReply, replyingTo, focusMessageId }: GroupChatProps) {
  const c = copy.chat;
  const api = groupApiOf(services);
  const [bump, setBump] = useState(0);
  const { detail, messages, notices } = useGroupChat(services, groupId, refreshKey + bump + (events?.version ?? 0));
  const invites = api === null ? [] : safeInvites(services);
  const names = usePeopleNames(detail);
  const [sheet, setSheet] = useState<"people" | "settings" | null>(null);
  const [said, setSaid] = useState("");
  const [picking, setPicking] = useState<string | null>(null);
  const [editing, setEditing] = useState<{ readonly id: string; readonly text: string } | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [marked, setMarked] = useState<string | null>(null);
  const end = useRef<HTMLDivElement>(null);
  const count = messages.length + notices.length;
  useEffect(() => {
    const el = end.current;
    if (el === null || typeof el.scrollIntoView !== "function") return;
    const calm = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
    el.scrollIntoView({ block: "end", behavior: calm ? "auto" : "smooth" });
  }, [count]);
  const shownFocus = useRef<string | null>(null);
  const hasFocus = focusMessageId !== undefined && messages.some((m) => m.id === focusMessageId);
  useEffect(() => {
    if (focusMessageId === undefined) { shownFocus.current = null; return; }
    if (!hasFocus || shownFocus.current === focusMessageId) return;
    shownFocus.current = focusMessageId;
    const el = document.getElementById(`om-gmsg-${focusMessageId}`);
    if (el === null) return;
    const calm = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (typeof el.scrollIntoView === "function") el.scrollIntoView({ block: "center", behavior: calm ? "auto" : "smooth" });
    setMarked(focusMessageId);
    const t = setTimeout(() => { setMarked((m) => (m === focusMessageId ? null : m)); }, 2400);
    return () => { clearTimeout(t); };
  }, [focusMessageId, hasFocus]);
  // A group this person is no longer in (left, or removed) has no screen: back to the list.
  useEffect(() => { if (api !== null && detail === null) onLeft(); }, [api, detail, onLeft]);
  if (detail === null) return <div className="om-conv-view" aria-busy="true" />;
  const group = detail.summary.name;
  const shown = (a: string, name: string): string => { const s = names.sameName(a); return s === null ? name : copy.sameName.of(name, s.index, s.of); };
  const senderLabel = (a: string): string => { const n = names.nameOf(a); return n === null ? copy.members.someone(names.unnamedNumber(a)) : shown(a, n); };
  const entries = timeline(messages, notices);
  const whoLabel = (a: string): string => (names.isYou(a) ? copy.notices.youChanged : shown(a, names.nameOf(a) ?? copy.notices.someone));
  /** One typed step on a message (react, edit, delete). A refusal is said once, plainly; the message stays as it was. */
  const act = (actionId: string, params: Readonly<Record<string, unknown>>, fail: string, done?: () => void): void => {
    void runGroupAction(services, actionId, { conversationId: groupId, ...params }).then((r) => {
      if (r.ok) { setFailed(null); done?.(); setBump((n) => n + 1); onChanged?.(); } else setFailed(fail);
    });
  };
  const jumpTo = (target: string): void => {
    const el = document.getElementById(`om-gmsg-${target}`);
    if (el === null) return;
    const calm = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (typeof el.scrollIntoView === "function") el.scrollIntoView({ block: "center", behavior: calm ? "auto" : "smooth" });
    setMarked(target);
    setTimeout(() => { setMarked((m) => (m === target ? null : m)); }, 2400);
  };
  const changed = (what: "invited" | "removed"): void => { setBump((n) => n + 1); setSaid(what === "invited" ? copy.done.invited : copy.done.removed); onChanged?.(); };
  return (
    <section className="om-conv-view" aria-label={group}>
      <header className="om-conv-view__header">
        {onBack ? <IconButton className="om-conv-view__back" label={c.back} icon={<Icon name="back" />} onClick={onBack} /> : null}
        <Avatar name={group} />
        <div className="om-conv-view__who">
          <Text as="h2" size="lg" weight="bold">{group}</Text>
        </div>
        <Button variant="quiet" className="om-group-people" aria-label={c.openPeople(group)} onClick={() => { setSheet("people"); }}>
          <Icon name="users" size={20} />{c.people(detail.summary.members)}
        </Button>
      </header>
      {rejoinReviews.map((review) => (
        <GroupRejoinReview key={review.id} services={services} copy={copy} chats={chats} review={review} onChanged={() => { onChanged?.(); }} />
      ))}
      {names.sharedNames.map((g) => <Text key={g.name} as="p" size="sm" tone="secondary" className="om-group-samename"><bdi>{(g.kind === "alike" ? copy.sameName.alike : copy.sameName.here)(g.name, g.count)}</bdi></Text>)}
      {failed === null ? null : <p className="om-conv-view__relationship" role="status">{failed}</p>}
      <div role="log" aria-label={c.label} className="om-conv-view__log">
        {entries.length === 0 ? <p className="om-conv-view__empty">{c.empty}</p> : (
          <ol className="om-conv-view__timeline">
            {entries.map((e, i) => {
              if (e.kind === "notice") return <NoticeLine key={`n-${String(e.n.atMs)}-${String(i)}`} copy={copy} notice={e.n} who={whoLabel(e.n.byAccount)} />;
              const m = e.m;
              const prev = entries[i - 1];
              const next = entries[i + 1];
              const sameAsPrev = prev?.kind === "message" && prev.m.senderAccount === m.senderAccount && m.atMs - prev.m.atMs < RUN_GAP_MS;
              const lastOfRun = next?.kind !== "message" || next.m.senderAccount !== m.senderAccount || next.m.atMs - m.atMs >= RUN_GAP_MS;
              const direction = m.outgoing ? "outgoing" : "incoming";
              const mine = names.isYou(m.senderAccount);
              const gist = gistOf(m.text, c.reply.attachment);
              const original = m.replyTo === undefined ? undefined : messages.find((x) => x.id === m.replyTo);
              const reply: ReplyView | undefined = m.replyTo === undefined ? undefined
                : original === undefined ? { messageId: asMessageId(m.replyTo), available: false }
                : { messageId: asMessageId(original.id), available: true, outgoing: names.isYou(original.senderAccount), preview: Array.from(original.text).slice(0, 140).join("") };
              const react = (key: string, present: boolean): void => { setPicking(null); act("group-react-message", { messageId: m.id, reaction: key, present }, c.react.failed); };
              return (
                <li key={m.id} id={`om-gmsg-${m.id}`} className="om-conv-view__item" data-found={marked === m.id ? "true" : undefined} data-direction={direction} data-joined={sameAsPrev ? "true" : undefined} data-last={lastOfRun ? "true" : undefined}>
                  {m.outgoing || sameAsPrev ? <VisuallyHidden>{m.outgoing ? c.outgoing : c.incoming}</VisuallyHidden> : <span className="om-group-sender"><bdi>{senderLabel(m.senderAccount)}</bdi></span>}
                  <div className="om-conv-view__row">
                    <div className="om-conv-view__bubble">
                      {reply === undefined ? null : <Quote reply={reply} name={original === undefined ? "" : senderLabel(original.senderAccount)} copy={c.reply} onJump={(id) => { jumpTo(id); }} />}
                      {editing?.id === m.id
                        ? <EditForm copy={c.change} text={editing.text} onChange={(next) => { setEditing({ id: m.id, text: next }); }} onSave={() => { act("group-edit-message", { messageId: m.id, text: editing.text }, c.change.editFailed, () => { setEditing(null); }); }} onCancel={() => { setEditing(null); }} />
                        : m.poll !== undefined ? <PollCard question={m.text} poll={m.poll} copy={copy.poll} busy={false} onVote={(optionIds) => { act("group-vote-poll", { pollId: m.id, optionIds }, copy.poll.failed); }} onClose={() => { act("group-close-poll", { pollId: m.id }, copy.poll.failed); }} />
                        : <div className="om-group-text"><RichText doc={textWithMentions(m.text, (m.mentions ?? []).map((x) => ({ ...x, me: names.isYou(x.account) })))} copy={copy.richText} /></div>}
                    </div>
                    {!detail.canSend ? null : (
                      <ReactionControl
                        copy={c.react} gist={gist} open={picking === m.id} onToggle={() => { setPicking((p) => (p === m.id ? null : m.id)); }} onClose={() => { setPicking(null); }}
                        onPick={react} isMine={(key) => m.reactions?.some((r) => r.key === key && r.mine) ?? false}
                      />
                    )}
                    {!detail.canSend || !mine ? null : (
                      <>
                        <IconButton className="om-conv-view__reply" label={`${c.change.edit}: ${gist}`} icon={<Icon name="edit" size={18} />} onClick={() => { setDeleting(null); setEditing({ id: m.id, text: m.text }); }} />
                        <IconButton className="om-conv-view__reply" label={`${c.change.delete}: ${gist}`} icon={<Icon name="trash" size={18} />} onClick={() => { setEditing(null); setDeleting(m.id); }} />
                      </>
                    )}
                    {onReply === undefined || !detail.canSend ? null : (
                      <IconButton
                        className="om-conv-view__reply" label={`${c.reply.action}: ${gist}`} icon={<Icon name="reply" size={18} />} aria-pressed={replyingTo === m.id}
                        onClick={() => { onReply({ messageId: asMessageId(m.id), preview: Array.from(m.text).slice(0, 140).join(""), hasAttachment: false, outgoing: mine, who: senderLabel(m.senderAccount) }); }}
                      />
                    )}
                  </div>
                  {deleting === m.id ? <DeleteConfirm copy={c.change} onConfirm={() => { act("group-delete-message", { messageId: m.id }, c.change.deleteFailed, () => { setDeleting(null); }); }} onCancel={() => { setDeleting(null); }} /> : null}
                  <ReactionChips copy={c.react} reactions={m.reactions ?? []} onPick={react} />
                  {lastOfRun ? (
                    <span className="om-conv-view__foot">
                      <time dateTime={new Date(m.atMs).toISOString()}>{c.formatTime(m.atMs)}</time>
                      {m.edited === true ? <span className="om-conv-view__edited">{c.change.edited}</span> : null}
                    </span>
                  ) : null}
                </li>
              );
            })}
          </ol>
        )}
        <div ref={end} className="om-conv-view__end" aria-hidden="true" />
      </div>
      {events === undefined ? null : (
        <GroupStatus copy={copy} states={events.states} scope={{ groupId, groupName: group }} onDismissUndeliverable={events.dismissUndeliverable} onDismissSettingBlocked={events.dismissSettingBlocked} onDismissAdmitBlocked={events.dismissAdmitBlocked} />
      )}
      {detail.canSend ? null : <p className="om-group-paused" role="status">{c.paused}</p>}
      {footer}
      <p className="om-visually-hidden" role="status">{said}</p>
      {sheet === "people" ? (
        <Sheet title={copy.members.title(group)} closeLabel={copy.members.close} onClose={() => { setSheet(null); }}>
          <GroupMembers services={services} copy={copy} detail={detail} names={names} invites={invites} chats={chats} onChanged={changed} onLeft={() => { setSheet(null); setSaid(copy.done.left); onLeft(); }} onOpenSettings={() => { setSheet("settings"); }} />
        </Sheet>
      ) : null}
      {sheet === "settings" ? (
        <Sheet title={copy.settings.title(group)} closeLabel={copy.settings.close} onClose={() => { setSheet(null); }}>
          <Button variant="quiet" onClick={() => { setSheet("people"); }}>{copy.settings.back}</Button>
          <RoomSettings services={services} copy={copy} detail={detail} onChanged={() => { setBump((n) => n + 1); onChanged?.(); }} />
        </Sheet>
      ) : null}
    </section>
  );
}

/** A change of the group's settings, said as a system line. A loosening is marked in words (never colour alone). */
function NoticeLine({ copy, notice, who }: { readonly copy: GroupsCopy; readonly notice: GroupNoticeView; readonly who: string }) {
  const n = copy.notices;
  return (
    <li className="om-group-notice" aria-label={n.label}>
      {notice.changes.map((ch) => (
        <p key={ch.field} className="om-group-notice__line" data-loosened={ch.loosened ? "true" : undefined}>
          {ch.loosened ? <strong className="om-group-notice__mark">{n.loosened}</strong> : null}
          {ch.loosened ? " " : null}<bdi>{n.line(who, n.field[ch.field] ?? n.field["_"] ?? "", n.value(ch.field, ch.to))}</bdi>
        </p>
      ))}
    </li>
  );
}

function safeInvites(s: ConversationServices) {
  try { return groupApiOf(s)?.invites(s.ctx) ?? []; } catch { return []; }
}

export interface GroupComposerProps {
  readonly services: ConversationServices;
  readonly copy: GroupsCopy;
  readonly groupId: GroupId;
  readonly draft: string;
  readonly onDraftChange: (next: string) => void;
  /** Controls placed before Send (the Omnibar trigger), as in a chat. */
  readonly trailing?: ReactNode;
  readonly onSent?: () => void;
  /** The message being answered: shown as a chip above the box and named in the send (`reply_to`). Controlled by the parent, like the draft. */
  readonly replyTo?: ReplyTarget | null;
  /** Ends the reply (the cancel control, Escape, or after a send). */
  readonly onCancelReply?: () => void;
}

const enterSends = (): boolean => typeof matchMedia === "function" && matchMedia("(pointer: fine)").matches;

/** Text only: the same pill as a chat's. The draft stays until the step went through; a refusal is said and the words stay. */
export function GroupComposer({ services, copy, groupId, draft, onDraftChange, trailing, onSent, replyTo, onCancelReply }: GroupComposerProps) {
  const c = copy.chat;
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const input = useRef<HTMLTextAreaElement>(null);
  // Who can be mentioned: the people in the Room now, other than the person using this device, by the names the people list shows (ADR-0222).
  const { detail } = useGroupChat(services, groupId, 0);
  const names = usePeopleNames(detail);
  const candidates = useMemo<readonly MentionCandidate[]>(
    () => (detail?.people ?? []).filter((p) => p.status === "member" && !p.you).map((p) => ({ id: p.account, name: names.nameOf(p.account) ?? copy.members.someone(names.unnamedNumber(p.account)) })),
    [detail, names, copy],
  );
  const mentions = useMentions({ candidates, copy: copy.mentions, draft, onDraftChange, input });
  const [pollOpen, setPollOpen] = useState(false);
  const submit = async (): Promise<void> => {
    const text = draftRef.current;
    if (busy || text.trim() === "") return;
    setBusy(true);
    setNotice(null);
    const ranges = mentions.ranges(text);
    const r = ranges.length > 0
      ? await runGroupAction(services, "group-send-mentions", { conversationId: groupId, text, mentions: ranges, ...(replyTo ? { replyTo: replyTo.messageId } : {}) })
      : replyTo
        ? await runGroupAction(services, "group-send-reply", { conversationId: groupId, text, replyTo: replyTo.messageId })
        : await runGroupAction(services, "group-send-text", { conversationId: groupId, text });
    setBusy(false);
    if (r.ok) { if (draftRef.current === text) { onDraftChange(""); mentions.reset(); } onCancelReply?.(); onSent?.(); } else setNotice(r.refused === "invalid" ? c.sendFailed : copy.refused(r.refused));
  };
  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>): void => {
    if (mentions.onKeyDown(e)) return;
    if (e.key === "Escape" && replyTo && onCancelReply) { e.preventDefault(); onCancelReply(); return; }
    if (e.key !== "Enter" || e.nativeEvent.isComposing) return;
    if (e.ctrlKey || e.metaKey || (!e.shiftKey && enterSends())) { e.preventDefault(); void submit(); }
  };
  return (
    <form className="om-composer" onSubmit={(e) => { e.preventDefault(); void submit(); }}>
      {replyTo ? <ReplyChip copy={c.reply} who={replyTo.who ?? ""} outgoing={replyTo.outgoing} preview={replyTo.preview} busy={busy} onCancel={() => { onCancelReply?.(); }} /> : null}
      <IconButton label={copy.poll.create} icon={<Icon name="check" />} disabled={busy} onClick={() => { setPollOpen(true); }} />
      {pollOpen ? (
        <Sheet title={copy.poll.title} closeLabel={copy.poll.cancel} onClose={() => { setPollOpen(false); }}>
          <PollForm copy={copy.poll} busy={busy} onCancel={() => { setPollOpen(false); }} onCreate={(poll) => {
            setBusy(true);
            void runGroupAction(services, "group-create-poll", { conversationId: groupId, ...poll }).then((r) => {
              setBusy(false);
              if (r.ok) { setPollOpen(false); onSent?.(); } else setNotice(copy.poll.failed);
            });
          }} />
        </Sheet>
      ) : null}
      {mentions.picker}
      <textarea
        ref={input} className="om-composer__input" aria-label={c.composerLabel} placeholder={c.placeholder} rows={1} value={draft}
        spellCheck={false} autoComplete="off" autoCorrect="off" onChange={mentions.onChange} onSelect={mentions.onSelect} onKeyDown={onKeyDown} {...mentions.inputProps}
      />
      {trailing}
      <IconButton type="submit" className="om-composer__send" data-variant="primary" label={c.send} icon={<Icon name="send" />} disabled={busy || draft.trim() === ""} />
      {notice === null ? null : <p className="om-composer__notice" role="alert">{notice}</p>}
    </form>
  );
}
