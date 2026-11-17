import { useId, useState } from "react";
import type { ConversationSummary } from "@om/platform-api";
import { useOperation } from "../devices/operation";
import { PatternAvatar } from "../groups/PatternAvatar";
import { Sheet } from "../shell/layout/Sheet";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { ConversationCopy } from "./copy";
import type { ConversationServices } from "./services";
import { shownName } from "./shownName";

export interface ContactDetailProps {
  readonly services: ConversationServices;
  readonly summary: ConversationSummary;
  readonly copy: ConversationCopy["contact"];
  readonly onClose: () => void;
}

/**
 * A small sheet about one contact (ADR-0196): the name as shown, the name they chose for themselves, a pattern that is the same every
 * time for this person, and the person's own nickname for them (set or clear). The nickname is kept on this device only and is never
 * sent. There is no verification or AccountID row: the design defines no form for them yet (D129), so none is invented.
 */
export function ContactDetail({ services, summary, copy, onClose }: ContactDetailProps) {
  const op = useOperation(services);
  const [text, setText] = useState(summary.nickname ?? "");
  const [said, setSaid] = useState<"saved" | "cleared" | null>(null);
  const inputId = useId();
  const hintId = useId();
  const name = shownName(summary);
  const dirty = text.trim() !== (summary.nickname ?? "");

  const save = async (value: string): Promise<void> => {
    setSaid(null);
    const id = await op.run("set-contact-nickname", { conversationId: summary.id, nickname: value });
    if (id !== null) setSaid(value.trim() === "" ? "cleared" : "saved");
    if (value.trim() === "") setText("");
  };
  const outcome = op.state.phase === "observing" ? op.state.outcome : null;
  const status = op.state.phase === "failed" ? copy.failed : outcome?.kind === "Rejected" ? copy.invalid : outcome?.kind === "Committed" && said !== null ? copy[said] : "";

  return (
    <Sheet title={copy.title(name)} closeLabel={copy.close} onClose={onClose}>
      <div className="om-contact">
        <div className="om-contact__id">
          <PatternAvatar seed={summary.peerAccount ?? summary.id} size="lg" />
          <div>
            <Text as="p" size="lg" weight="bold"><bdi>{name}</bdi></Text>
            <Text as="p" size="sm" tone="secondary" data-original-name="true">{copy.theirName}: <bdi>{summary.displayName}</bdi></Text>
          </div>
        </div>
        <form className="om-contact__form" onSubmit={(e) => { e.preventDefault(); if (dirty && !op.locked) void save(text); }}>
          <label htmlFor={inputId}><Text as="span" weight="bold">{copy.nicknameLabel}</Text></label>
          <input id={inputId} data-sheet-initial="true" type="text" maxLength={64} autoComplete="off" spellCheck={false} value={text} aria-describedby={hintId} placeholder={copy.nicknamePlaceholder} onChange={(e) => { setText(e.target.value); setSaid(null); }} />
          <Text as="p" size="sm" tone="secondary" id={hintId}>{copy.nicknameHint}</Text>
          <div className="om-contact__actions">
            <Button type="submit" variant="primary" disabled={op.locked || !dirty}>{copy.save}</Button>
            {summary.nickname === undefined ? null : <Button variant="secondary" disabled={op.locked} onClick={() => { void save(""); }}>{copy.clear}</Button>}
          </div>
          <p className="om-settings__status" role={status === copy.invalid || status === copy.failed ? "alert" : "status"}>{status}</p>
        </form>
      </div>
    </Sheet>
  );
}
