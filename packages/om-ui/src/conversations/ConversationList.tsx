import { useId, useState } from "react";
import type { KeyboardEvent } from "react";
import type { ConversationId, ConversationSummary } from "@om/platform-api";
import { shownName } from "./shownName";
import { Avatar } from "./Avatar";
import { PatternAvatar } from "../groups/PatternAvatar";
import { contactCollisions, contactsLine, type SameNameCopy } from "./nameCues";
import { Icon } from "../shell/primitives/icons";
import { VisuallyHidden } from "../shell/primitives/VisuallyHidden";
import type { ConversationCopy } from "./copy";
import { DeliveryIndicator, type DeliveryDisplay } from "./delivery";
import type { ConversationDigest } from "./hooks";
import "./conversations.css";
import "../groups/groups.css";

export interface ConversationListProps {
  readonly conversations: readonly ConversationSummary[];
  readonly selectedId: ConversationId | null;
  readonly onSelect: (id: ConversationId) => void;
  readonly copy: ConversationCopy;
  readonly unread?: ReadonlyMap<ConversationId, number>;
  /** The state of the newest message you sent; only an unsettled one (or a refused one) is shown, so a row stays calm. */
  readonly lastDelivery?: ReadonlyMap<ConversationId, DeliveryDisplay>;
  /** The latest message of each chat (see `useConversationDigests`). */
  readonly digests?: ReadonlyMap<ConversationId, ConversationDigest>;
  /** Unsent text per chat: shown after "Draft:" in place of the latest message. */
  readonly drafts?: ReadonlyMap<ConversationId, string>;
  /** Rows that are groups, with their accessible name ("Group, 3 people"): they get a small people marker next to the name. */
  readonly marks?: ReadonlyMap<ConversationId, string>;
  /** With it, contacts who show the same name or names that look alike get a pattern, "1 of 2" and one calm line above the list (ADR-0197). Group rows are not compared. */
  readonly sameNameCopy?: SameNameCopy;
}

/** Single-select listbox; focus stays on the listbox and `aria-activedescendant` tracks the active row. */
export function ConversationList({ conversations, selectedId, onSelect, copy, unread, lastDelivery, digests, drafts, marks, sameNameCopy }: ConversationListProps) {
  const base = useId();
  const [activeIndex, setActive] = useState(() => Math.max(0, conversations.findIndex((c) => c.id === selectedId)));
  if (conversations.length === 0) return <p className="om-conv-list__empty">{copy.list.empty}</p>;
  const active = Math.min(activeIndex, conversations.length - 1);
  const names = sameNameCopy === undefined ? null : contactCollisions(conversations, (id) => marks?.has(id) === true);
  const nameOf = (c: ConversationSummary): string => { const n = shownName(c); const s = names?.of(c.id) ?? null; return s === null || sameNameCopy === undefined ? n : sameNameCopy.of(n, s.index, s.of); };
  const optionId = (i: number): string => `${base}-opt-${String(i)}`;

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>): void => {
    const last = conversations.length - 1;
    const move = (i: number): void => { e.preventDefault(); setActive(Math.max(0, Math.min(last, i))); };
    switch (e.key) {
      case "ArrowDown": move(active + 1); break;
      case "ArrowUp": move(active - 1); break;
      case "Home": move(0); break;
      case "End": move(last); break;
      case "Enter":
      case " ": {
        e.preventDefault();
        const c = conversations[active];
        if (c) onSelect(c.id);
        break;
      }
      default: break;
    }
  };

  return (
    <>
    {names === null || sameNameCopy === undefined || names.groups.length === 0 ? null : (
      <div className="om-conv-list__samename">{names.groups.map((g) => <p key={g.name + g.kind} className="om-group-samename" data-samename={g.kind}><bdi>{contactsLine(sameNameCopy, { ...g, index: 1, of: g.count })}</bdi></p>)}</div>
    )}
    <div role="listbox" tabIndex={0} aria-label={copy.list.label} aria-activedescendant={optionId(active)} className="om-conv-list" onKeyDown={onKeyDown}>
      {conversations.map((c, i) => {
        const count = unread?.get(c.id) ?? 0;
        const digest = digests?.get(c.id);
        const delivery = lastDelivery?.get(c.id) ?? digest?.lastDelivery ?? undefined;
        const unsettled = delivery !== undefined && delivery.kind !== "sent" && delivery.kind !== "delivered" ? delivery : undefined;
        const draft = c.id === selectedId ? "" : (drafts?.get(c.id)?.replace(/\s+/g, " ").trim() ?? "");
        const preview = draft !== "" ? { lead: copy.list.draft, text: draft } : digest?.lastText ? { lead: digest.lastFromMe ? copy.list.you : "", text: digest.lastText } : null;
        return (
          <div
            key={c.id} id={optionId(i)} role="option" aria-selected={c.id === selectedId} data-active={i === active ? "true" : undefined}
            data-unread={count > 0 ? "true" : undefined} data-draft={draft !== "" ? "true" : undefined} className="om-conv-list__row"
            onClick={() => { setActive(i); onSelect(c.id); }}
          >
            {names?.of(c.id) == null ? <Avatar name={shownName(c)} /> : <PatternAvatar seed={c.id} />}
            <span className="om-conv-list__main">
              <span className="om-conv-list__top">
                <span className="om-conv-list__name">{marks?.has(c.id) ? <span className="om-group-mark"><Icon name="users" size={16} /><VisuallyHidden>{marks.get(c.id)}</VisuallyHidden></span> : null}<bdi>{nameOf(c)}</bdi></span>
                <span className="om-conv-list__meta">
                  {c.notices.length > 0 ? <span className="om-conv-list__notice" data-security-notice="true"><span aria-hidden="true">!</span><VisuallyHidden>{copy.list.securityNotice}</VisuallyHidden></span> : null}
                  {c.lastMessageAt === null ? null : <span className="om-conv-list__time">{copy.view.formatTime(c.lastMessageAt)}</span>}
                </span>
              </span>
              <span className="om-conv-list__bottom">
                <span className="om-conv-list__preview" data-empty={preview === null ? "true" : undefined}>
                  {preview === null ? copy.list.noActivity : <>{preview.lead === "" ? null : <span className="om-conv-list__lead">{preview.lead} </span>}{preview.text}</>}
                </span>
                {unsettled ? <DeliveryIndicator delivery={unsettled} copy={copy.delivery} /> : null}
                {count > 0 ? <span className="om-conv-list__unread"><span aria-hidden="true">{count > 99 ? "99+" : count}</span><VisuallyHidden>{copy.list.unread(count)}</VisuallyHidden></span> : null}
              </span>
            </span>
          </div>
        );
      })}
    </div>
    </>
  );
}
