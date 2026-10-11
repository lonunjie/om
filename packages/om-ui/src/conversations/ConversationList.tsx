import { useId, useState } from "react";
import type { KeyboardEvent } from "react";
import type { ConversationId, ConversationSummary } from "@om/platform-api";
import { VisuallyHidden } from "../shell/primitives/VisuallyHidden";
import type { ConversationCopy } from "./copy";
import { DeliveryIndicator, type DeliveryDisplay } from "./delivery";
import "./conversations.css";

export interface ConversationListProps {
  readonly conversations: readonly ConversationSummary[];
  readonly selectedId: ConversationId | null;
  readonly onSelect: (id: ConversationId) => void;
  readonly copy: ConversationCopy;
  readonly unread?: ReadonlyMap<ConversationId, number>;
  readonly lastDelivery?: ReadonlyMap<ConversationId, DeliveryDisplay>;
}

/** Single-select listbox; focus stays on the listbox and `aria-activedescendant` tracks the active row. */
export function ConversationList({ conversations, selectedId, onSelect, copy, unread, lastDelivery }: ConversationListProps) {
  const base = useId();
  const [activeIndex, setActive] = useState(() => Math.max(0, conversations.findIndex((c) => c.id === selectedId)));
  if (conversations.length === 0) return <p className="om-conv-list__empty">{copy.list.empty}</p>;
  const active = Math.min(activeIndex, conversations.length - 1);
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
    <div role="listbox" tabIndex={0} aria-label={copy.list.label} aria-activedescendant={optionId(active)} className="om-conv-list" onKeyDown={onKeyDown}>
      {conversations.map((c, i) => {
        const count = unread?.get(c.id) ?? 0;
        const delivery = lastDelivery?.get(c.id);
        return (
          <div
            key={c.id} id={optionId(i)} role="option" aria-selected={c.id === selectedId} data-active={i === active ? "true" : undefined}
            data-unread={count > 0 ? "true" : undefined} className="om-conv-list__row"
            onClick={() => { setActive(i); onSelect(c.id); }}
          >
            <span className="om-conv-list__name">{c.displayName}</span>
            <span className="om-conv-list__meta">
              {c.notices.length > 0 ? <span className="om-conv-list__notice" data-security-notice="true"><span aria-hidden="true">!</span><VisuallyHidden>{copy.list.securityNotice}</VisuallyHidden></span> : null}
              {count > 0 ? <span className="om-conv-list__unread">{copy.list.unread(count)}</span> : null}
              <span className="om-conv-list__time">{c.lastMessageAt === null ? copy.list.noActivity : copy.view.formatTime(c.lastMessageAt)}</span>
              {delivery ? <DeliveryIndicator delivery={delivery} copy={copy.delivery} /> : null}
            </span>
          </div>
        );
      })}
    </div>
  );
}
