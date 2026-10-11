import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent, SyntheticEvent } from "react";
import type { ConversationId, CoreIntent, OperationHandle, OutcomeClass } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import type { ConversationCopy } from "./copy";
import { firstOutcome, resolveIntent, type ConversationServices } from "./services";
import "./conversations.css";

export interface InputPrivacy { readonly spellcheck: boolean; readonly autocomplete: "on" | "off" }
const PRIVATE_DEFAULT: InputPrivacy = { spellcheck: false, autocomplete: "off" };

export interface ComposerProps {
  readonly services: ConversationServices;
  readonly conversationId: ConversationId;
  readonly copy: ConversationCopy;
  /** Controlled draft: the parent owns it, so it survives mode transitions and remounts. */
  readonly draft: string;
  readonly onDraftChange: (next: string) => void;
  readonly inputPrivacy?: InputPrivacy;
  readonly onSent?: (handle: OperationHandle) => void;
}

/** Clear the draft only for these; Rejected and NeedsUserAction leave it intact. */
const clearsDraft = (o: OutcomeClass): boolean => o.kind === "Committed" || o.kind === "PendingAuthorization" || o.kind === "PendingNetwork" || o.kind === "PendingReconciliation";

export function Composer({ services, conversationId, copy, draft, onDraftChange, inputPrivacy = PRIVATE_DEFAULT, onSent }: ComposerProps) {
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  // After a dispatch error or NeedsUserAction, resubmitting the same text re-dispatches the same intent (same OperationId).
  const inflight = useRef<{ readonly text: string; readonly intent: CoreIntent } | null>(null);

  const submit = async (): Promise<void> => {
    const text = draftRef.current;
    if (busy || text.trim() === "") return;
    setBusy(true);
    setNotice(null);
    try {
      let intent = inflight.current?.text === text ? inflight.current.intent : null;
      if (intent === null) {
        const r = resolveIntent(services, "send-message", { conversationId, text });
        if (r.status !== "intent") { setNotice(copy.composer.sendFailed); return; }
        intent = r.intent;
        inflight.current = { text, intent };
      }
      const handle = await services.client.dispatch(intent);
      const outcome = await firstOutcome(services, handle.operationId);
      if (!mounted.current) { if (clearsDraft(outcome) && draftRef.current === text) onDraftChange(""); return; }
      if (clearsDraft(outcome)) {
        inflight.current = null;
        if (draftRef.current === text) onDraftChange("");
        onSent?.(handle);
      } else if (outcome.kind === "Rejected") {
        inflight.current = null;
        setNotice(copy.composer.rejected[outcome.reason]);
      } else if (outcome.kind === "NeedsUserAction") {
        setNotice(copy.composer.needsAction[outcome.action]);
      }
    } catch {
      if (mounted.current) setNotice(copy.composer.sendFailed);
    } finally {
      if (mounted.current) setBusy(false);
    }
  };

  const onSubmit = (e: SyntheticEvent): void => { e.preventDefault(); void submit(); };
  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>): void => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); void submit(); }
  };

  return (
    <form className="om-composer" onSubmit={onSubmit}>
      <textarea
        className="om-composer__input" aria-label={copy.composer.label} placeholder={copy.composer.placeholder} rows={2}
        value={draft} spellCheck={inputPrivacy.spellcheck} autoComplete={inputPrivacy.autocomplete} autoCorrect={inputPrivacy.autocomplete}
        onChange={(e) => { onDraftChange(e.target.value); }} onKeyDown={onKeyDown}
      />
      <Button type="submit" variant="primary" disabled={busy || draft.trim() === ""}>{copy.composer.send}</Button>
      {notice === null ? null : <p className="om-composer__notice" role="alert">{notice}</p>}
    </form>
  );
}
