import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import type { ConversationSummary, GroupId } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Icon } from "../shell/primitives/icons";
import { IconButton } from "../shell/primitives/IconButton";
import { Text } from "../shell/primitives/Text";
import { VisuallyHidden } from "../shell/primitives/VisuallyHidden";
import { Sheet } from "../shell/layout/Sheet";
import { Avatar } from "../conversations/Avatar";
import type { ConversationServices } from "../conversations/services";
import type { GroupsCopy } from "./copy";
import { GroupMembers } from "./GroupMembers";
import { useGroupChat, usePeopleNames } from "./hooks";
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
  /** The person left (or is no longer in) the group. */
  readonly onLeft: () => void;
  /** Bump to re-read after a step made elsewhere (the composer). */
  readonly refreshKey?: number;
  readonly onChanged?: () => void;
}

/** A group is an ordinary conversation with more people: the same header, bubbles and rhythm as a chat, plus who said it and a people sheet. */
export function GroupChat({ services, copy, groupId, chats, footer, onBack, onLeft, refreshKey = 0, onChanged }: GroupChatProps) {
  const c = copy.chat;
  const api = groupApiOf(services);
  const [bump, setBump] = useState(0);
  const { detail, messages } = useGroupChat(services, groupId, refreshKey + bump);
  const invites = api === null ? [] : safeInvites(services);
  const names = usePeopleNames(invites, chats, messages);
  const [people, setPeople] = useState(false);
  const [said, setSaid] = useState("");
  const end = useRef<HTMLDivElement>(null);
  const count = messages.length;
  useEffect(() => {
    const el = end.current;
    if (el === null || typeof el.scrollIntoView !== "function") return;
    const calm = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
    el.scrollIntoView({ block: "end", behavior: calm ? "auto" : "smooth" });
  }, [count]);
  // A group this person is no longer in (left, or removed) has no screen: back to the list.
  useEffect(() => { if (api !== null && detail === null) onLeft(); }, [api, detail, onLeft]);
  if (detail === null) return <div className="om-conv-view" aria-busy="true" />;
  const group = detail.summary.name;
  let unnamed = 0;
  const senderLabel = (a: string): string => names.nameOf(a) ?? copy.members.someone(++unnamed);
  const changed = (what: "invited" | "removed"): void => { setBump((n) => n + 1); setSaid(what === "invited" ? copy.done.invited : copy.done.removed); onChanged?.(); };
  return (
    <section className="om-conv-view" aria-label={group}>
      <header className="om-conv-view__header">
        {onBack ? <IconButton className="om-conv-view__back" label={c.back} icon={<Icon name="back" />} onClick={onBack} /> : null}
        <Avatar name={group} />
        <div className="om-conv-view__who">
          <Text as="h2" size="lg" weight="bold">{group}</Text>
        </div>
        <Button variant="quiet" className="om-group-people" aria-label={c.openPeople(group)} onClick={() => { setPeople(true); }}>
          <Icon name="users" size={20} />{c.people(detail.summary.members)}
        </Button>
      </header>
      <div role="log" aria-label={c.label} className="om-conv-view__log">
        {messages.length === 0 ? <p className="om-conv-view__empty">{c.empty}</p> : (
          <ol className="om-conv-view__timeline">
            {messages.map((m, i) => {
              const prev = messages[i - 1];
              const next = messages[i + 1];
              const sameAsPrev = prev !== undefined && prev.senderAccount === m.senderAccount && m.atMs - prev.atMs < RUN_GAP_MS;
              const lastOfRun = next === undefined || next.senderAccount !== m.senderAccount || next.atMs - m.atMs >= RUN_GAP_MS;
              const direction = m.outgoing ? "outgoing" : "incoming";
              return (
                <li key={m.id} className="om-conv-view__item" data-direction={direction} data-joined={sameAsPrev ? "true" : undefined} data-last={lastOfRun ? "true" : undefined}>
                  {m.outgoing || sameAsPrev ? <VisuallyHidden>{m.outgoing ? c.outgoing : c.incoming}</VisuallyHidden> : <span className="om-group-sender"><bdi>{senderLabel(m.senderAccount)}</bdi></span>}
                  <div className="om-conv-view__bubble"><p className="om-group-text">{m.text}</p></div>
                  {lastOfRun ? <span className="om-conv-view__foot"><time dateTime={new Date(m.atMs).toISOString()}>{c.formatTime(m.atMs)}</time></span> : null}
                </li>
              );
            })}
          </ol>
        )}
        <div ref={end} className="om-conv-view__end" aria-hidden="true" />
      </div>
      {detail.canSend ? null : <p className="om-group-paused" role="status">{c.paused}</p>}
      {footer}
      <p className="om-visually-hidden" role="status">{said}</p>
      {people ? (
        <Sheet title={copy.members.title(group)} closeLabel={copy.members.close} onClose={() => { setPeople(false); }}>
          <GroupMembers services={services} copy={copy} detail={detail} names={names} invites={invites} chats={chats} onChanged={changed} onLeft={() => { setPeople(false); setSaid(copy.done.left); onLeft(); }} />
        </Sheet>
      ) : null}
    </section>
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
}

const enterSends = (): boolean => typeof matchMedia === "function" && matchMedia("(pointer: fine)").matches;

/** Text only: the same pill as a chat's. The draft stays until the step went through; a refusal is said and the words stay. */
export function GroupComposer({ services, copy, groupId, draft, onDraftChange, trailing, onSent }: GroupComposerProps) {
  const c = copy.chat;
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const submit = async (): Promise<void> => {
    const text = draftRef.current;
    if (busy || text.trim() === "") return;
    setBusy(true);
    setNotice(null);
    const r = await runGroupAction(services, "group-send-text", { conversationId: groupId, text });
    setBusy(false);
    if (r.ok) { if (draftRef.current === text) onDraftChange(""); onSent?.(); } else setNotice(r.refused === "invalid" ? c.sendFailed : copy.refused(r.refused));
  };
  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>): void => {
    if (e.key !== "Enter" || e.nativeEvent.isComposing) return;
    if (e.ctrlKey || e.metaKey || (!e.shiftKey && enterSends())) { e.preventDefault(); void submit(); }
  };
  return (
    <form className="om-composer" onSubmit={(e) => { e.preventDefault(); void submit(); }}>
      <textarea
        className="om-composer__input" aria-label={c.composerLabel} placeholder={c.placeholder} rows={1} value={draft}
        spellCheck={false} autoComplete="off" autoCorrect="off" onChange={(e) => { onDraftChange(e.target.value); }} onKeyDown={onKeyDown}
      />
      {trailing}
      <IconButton type="submit" className="om-composer__send" data-variant="primary" label={c.send} icon={<Icon name="send" />} disabled={busy || draft.trim() === ""} />
      {notice === null ? null : <p className="om-composer__notice" role="alert">{notice}</p>}
    </form>
  );
}
