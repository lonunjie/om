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
  readonly ownAccount?: string | undefined;
  readonly copy: ConversationCopy["contact"];
  readonly onClose: () => void;
}

const ACCOUNT_ID = /^[0-9a-f]{64}$/u;

export function ContactDetail({ services, summary, ownAccount, copy, onClose }: ContactDetailProps) {
  const op = useOperation(services);
  const [text, setText] = useState(summary.nickname ?? "");
  const [said, setSaid] = useState<"saved" | "cleared" | null>(null);
  const inputId = useId();
  const hintId = useId();
  const name = shownName(summary);
  const dirty = text.trim() !== (summary.nickname ?? "");
  const peer = summary.peerAccount;
  const canVerify = summary.relationship === "active" && ownAccount !== undefined && peer !== undefined && ACCOUNT_ID.test(ownAccount) && ACCOUNT_ID.test(peer);

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
        {canVerify ? <AccountCheck key={`${ownAccount}:${peer}`} own={ownAccount} peer={peer} copy={copy.verification} /> : null}
      </div>
    </Sheet>
  );
}

function AccountCheck({ own, peer, copy }: { readonly own: string; readonly peer: string; readonly copy: ConversationCopy["contact"]["verification"] }) {
  const [open, setOpen] = useState(false);
  const panelId = useId();
  return (
    <div className="om-contact__verification">
      <Button variant="quiet" aria-expanded={open} aria-controls={panelId} onClick={() => { setOpen(!open); }}>{copy.open}</Button>
      <div id={panelId} hidden={!open}>
        {open ? <AccountComparison own={own} peer={peer} copy={copy} /> : null}
      </div>
    </div>
  );
}

function AccountComparison({ own, peer, copy }: { readonly own: string; readonly peer: string; readonly copy: ConversationCopy["contact"]["verification"] }) {
  const [input, setInput] = useState("");
  const [result, setResult] = useState<"invalid" | "match" | "mismatch" | null>(null);
  const inputId = useId();
  const hintId = useId();
  return (
    <form className="om-contact__form" noValidate onSubmit={(e) => {
      e.preventDefault();
      const entered = input.trim();
      if (!/^[0-9a-fA-F]{64}$/u.test(entered)) { setResult("invalid"); return; }
      setResult(entered.toLowerCase() === peer ? "match" : "mismatch");
    }}>
      <Text as="p" size="sm" id={hintId}>{copy.hint}</Text>
      <dl>
        <dt>{copy.theirAccount}</dt><dd><code dir="ltr">{peer}</code></dd>
        <dt>{copy.ownAccount}</dt><dd><code dir="ltr">{own}</code></dd>
      </dl>
      <label htmlFor={inputId}>{copy.input}</label>
      <input id={inputId} dir="ltr" type="text" autoComplete="off" autoCapitalize="off" spellCheck={false} aria-describedby={hintId} aria-invalid={result === "invalid"} value={input}
        onChange={(e) => { setInput(e.target.value); setResult(null); }} />
      <div className="om-contact__actions"><Button type="submit">{copy.check}</Button></div>
      <p role="status">{result === null ? "" : copy[result]}</p>
    </form>
  );
}
