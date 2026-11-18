import { VisuallyHidden } from "../shell/primitives/VisuallyHidden";
import type { MessageId, ReactionView, ReplyView } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Icon } from "../shell/primitives/icons";
import { IconButton } from "../shell/primitives/IconButton";
import type { ConversationCopy } from "./copy";
import { DEFAULT_REACTIONS } from "./reactions";

/**
 * The everyday chat gestures (reply, react, edit, delete) as parts a chat and a group share, so both draw and speak them the same
 * way: same controls, same accessible names, same keys. They hold no state of their own and send nothing; the caller owns both.
 */

/** A short spoken name for a message, so "Reply" says which one: the start of its text, or that it is an attachment. */
export const gistOf = (text: string, attachment: string): string => {
  const t = Array.from(text.trim()).slice(0, 40).join("");
  return t === "" ? attachment : t;
};

/** The quoted block: who wrote the original and its first words, from the local copy. A tap moves to the original; a missing one is only said so. */
export function Quote({ reply, name, copy, onJump }: { readonly reply: ReplyView; readonly name: string; readonly copy: ConversationCopy["reply"]; readonly onJump: (id: MessageId) => void }) {
  if (!reply.available) return <p className="om-quote" data-available="false">{copy.unavailable}</p>;
  const who = reply.outgoing === true ? copy.inReplyToSelf : copy.inReplyTo(name);
  const text = reply.preview === undefined || reply.preview === "" ? copy.attachment : reply.preview;
  return (
    <button type="button" className="om-quote" data-available="true" aria-label={`${who}: ${text}. ${copy.jump}`} onClick={() => { onJump(reply.messageId); }}>
      <span className="om-quote__who" aria-hidden="true"><bdi>{who}</bdi></span>
      <span className="om-quote__text" aria-hidden="true"><bdi>{text}</bdi></span>
    </button>
  );
}

/** The chip above the composer while a reply is being written: who it answers and the start of the original. */
export function ReplyChip({ copy, who, outgoing, preview, busy, onCancel }: { readonly copy: ConversationCopy["reply"]; readonly who: string; readonly outgoing: boolean; readonly preview: string; readonly busy: boolean; readonly onCancel: () => void }) {
  const label = outgoing ? copy.replyingToSelf : copy.replyingTo(who);
  return (
    <div className="om-composer__reply" role="group" aria-label={label}>
      <span className="om-composer__reply-text">
        <span className="om-composer__reply-who"><bdi>{label}</bdi></span>
        <span className="om-composer__reply-preview"><bdi>{preview === "" ? copy.attachment : preview}</bdi></span>
      </span>
      <IconButton label={copy.cancel} icon={<Icon name="close" size={18} />} disabled={busy} onClick={onCancel} />
    </div>
  );
}

/** The React control of one message and, while open, the short set to choose from. Escape closes the picker and nothing else. */
export function ReactionControl({ copy, gist, open, onToggle, onClose, onPick, isMine }: {
  readonly copy: ConversationCopy["react"]; readonly gist: string; readonly open: boolean;
  readonly onToggle: () => void; readonly onClose: () => void; readonly onPick: (key: string, present: boolean) => void; readonly isMine: (key: string) => boolean;
}) {
  return (
    <span className="om-react" onKeyDown={(e) => { if (e.key === "Escape" && open) { e.stopPropagation(); onClose(); } }}>
      <IconButton className="om-conv-view__reply" label={`${copy.action}: ${gist}`} icon={<Icon name="smile" size={18} />} aria-expanded={open} onClick={onToggle} />
      {open ? (
        <div role="group" aria-label={copy.picker} className="om-react__picker">
          {DEFAULT_REACTIONS.map((key) => (
            <button key={key} type="button" className="om-react__choice" aria-label={copy.set(key)} onClick={() => { onPick(key, !isMine(key)); }}>
              <span aria-hidden="true">{key}</span>
            </button>
          ))}
        </div>
      ) : null}
    </span>
  );
}

/** The reactions under a message: a chip per key; a tap adds your reaction, or removes it when it is yours. */
export function ReactionChips({ copy, reactions, onPick }: { readonly copy: ConversationCopy["react"]; readonly reactions: readonly ReactionView[]; readonly onPick?: (key: string, present: boolean) => void }) {
  if (reactions.length === 0) return null;
  return (
    <ul className="om-react__chips" aria-label={copy.list}>
      {reactions.map((r) => (
        <li key={r.key}>
          {onPick === undefined ? (
            <span className="om-react__chip">
              <VisuallyHidden>{copy.chip(r.key, r.count, r.mine)}</VisuallyHidden>
              <span aria-hidden="true">{r.key}{r.count > 1 ? ` ${String(r.count)}` : ""}</span>
            </span>
          ) : (
            <button type="button" className="om-react__chip" data-mine={r.mine ? "true" : undefined} aria-pressed={r.mine} aria-label={copy.chip(r.key, r.count, r.mine)} onClick={() => { onPick(r.key, !r.mine); }}>
              <span aria-hidden="true">{r.key}{r.count > 1 ? ` ${String(r.count)}` : ""}</span>
            </button>
          )}
        </li>
      ))}
    </ul>
  );
}

/** Inline editing of one of your own messages: Save, Cancel or Escape. */
export function EditForm({ copy, text, onChange, onSave, onCancel }: { readonly copy: ConversationCopy["change"]; readonly text: string; readonly onChange: (next: string) => void; readonly onSave: () => void; readonly onCancel: () => void }) {
  return (
    <form className="om-edit" onSubmit={(e) => { e.preventDefault(); if (text.trim() !== "") onSave(); }}>
      <textarea
        className="om-composer__input" aria-label={copy.editLabel} rows={2} value={text} autoFocus
        onChange={(e) => { onChange(e.target.value); }}
        onKeyDown={(e) => { if (e.key === "Escape") { e.preventDefault(); onCancel(); } }}
      />
      <span className="om-edit__actions">
        <Button variant="primary" type="submit" disabled={text.trim() === ""}>{copy.save}</Button>
        <Button variant="quiet" onClick={onCancel}>{copy.cancel}</Button>
      </span>
    </form>
  );
}

/** The question before a delete for everyone: it says who loses the message and that a copy already read cannot be called back. */
export function DeleteConfirm({ copy, onConfirm, onCancel }: { readonly copy: ConversationCopy["change"]; readonly onConfirm: () => void; readonly onCancel: () => void }) {
  return (
    <div className="om-conv-view__confirm" role="group" aria-label={copy.deleteTitle}>
      <p>{copy.deleteBody}</p>
      <span className="om-edit__actions">
        <Button variant="danger" onClick={onConfirm}>{copy.deleteConfirm}</Button>
        <Button variant="quiet" onClick={onCancel}>{copy.cancel}</Button>
      </span>
    </div>
  );
}
