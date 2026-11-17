import { useEffect, useRef, useState } from "react";
import type { IntroductionRequest, OutcomeClass } from "@om/platform-api";
import { confirmWithCore } from "../actions/confirm";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import { PatternAvatar } from "../groups/PatternAvatar";
import type { Named } from "../groups/sameName";
import { requestCollision, requestLine, type SameNameCopy } from "./nameCues";
import type { ConversationCopy } from "./copy";
import { firstOutcome, resolveIntent, type ConversationServices } from "./services";
import "./conversations.css";
import "../groups/groups.css";

export interface MessageRequestProps {
  readonly services: ConversationServices;
  /** Only the view-model fields are shown; nothing else from the unknown sender is rendered. */
  readonly request: IntroductionRequest;
  readonly copy: ConversationCopy;
  /** Ignore is local presentation (no Core intent). */
  readonly onIgnore: (requestId: string) => void;
  /** The shown names of your contacts (nickname else their own name). With these and `sameNameCopy`, a request whose name equals or looks like one of them says so, calmly, before you accept (ADR-0197). */
  readonly contacts?: readonly Named[];
  readonly sameNameCopy?: SameNameCopy;
  readonly onResolved?: (action: "accept" | "block", outcome: OutcomeClass) => void;
}

type Phase = "idle" | "confirm_block" | "busy" | "done";

export function MessageRequest({ services, request, copy, contacts, sameNameCopy, onIgnore, onResolved }: MessageRequestProps) {
  const [phase, setPhase] = useState<Phase>("idle");
  const [status, setStatus] = useState<string | null>(null);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const c = copy.request;
  const same = contacts === undefined || sameNameCopy === undefined ? null : requestCollision(request.id, request.displayName, contacts);

  const run = async (action: "accept" | "block"): Promise<void> => {
    setPhase("busy");
    setStatus(null);
    try {
      let r;
      if (action === "accept") {
        r = resolveIntent(services, "accept-introduction", { requestId: request.id });
      } else {
        const params = { personId: request.from, scope: "local" } as const;
        const issued = await confirmWithCore(services, "block-person", params);
        if (issued.status !== "issued") { setPhase("idle"); setStatus(c.failed); return; }
        r = resolveIntent(services, "block-person", params, issued.token);
      }
      if (r.status !== "intent") { setPhase("idle"); setStatus(c.failed); return; }
      const handle = await services.client.dispatch(r.intent);
      const outcome = await firstOutcome(services, handle.operationId);
      if (!mounted.current) return;
      onResolved?.(action, outcome);
      switch (outcome.kind) {
        case "Committed": setPhase("done"); setStatus(c.done); break;
        case "Rejected": setPhase("idle"); setStatus(c.rejected[outcome.reason]); break;
        case "NeedsUserAction": setPhase("idle"); setStatus(c.failed); break;
        default: setPhase("done"); setStatus(c.pending); break;
      }
    } catch {
      if (mounted.current) { setPhase("idle"); setStatus(c.failed); }
    }
  };

  const idle = phase === "idle";
  return (
    <section className="om-request" aria-label={c.heading}>
      <Text as="h3" size="md" weight="bold">{c.heading}</Text>
      {same === null ? null : <PatternAvatar seed={request.id} />}
      <Text as="p" className="om-request__sender"><bdi>{same === null || sameNameCopy === undefined ? request.displayName : sameNameCopy.of(request.displayName, same.index, same.of)}</bdi> <span className="om-request__unknown">{c.unknownSender}</span>{request.verification === "unverified" ? <> <span className="om-request__unverified" data-verification="unverified">{c.unverified}</span></> : null}</Text>
      {same === null || sameNameCopy === undefined ? null : <Text as="p" size="sm" tone="secondary" className="om-group-samename" data-samename={same.kind}><bdi>{requestLine(sameNameCopy, same)}</bdi></Text>}
      <Text as="p" size="sm" tone="secondary">{c.hint}</Text>
      {phase === "confirm_block" ? (
        <div role="group" aria-label={c.blockPrompt} className="om-request__confirm">
          <Text as="p">{c.blockPrompt}</Text>
          <Button variant="primary" onClick={() => { setPhase("idle"); }}>{c.blockCancel}</Button>
          <Button variant="danger" onClick={() => { void run("block"); }}>{c.blockConfirm}</Button>
        </div>
      ) : (
        <div className="om-request__actions">
          <Button variant="primary" disabled={!idle} onClick={() => { void run("accept"); }}>{c.accept}</Button>
          <Button disabled={!idle} onClick={() => { onIgnore(request.id); }}>{c.ignore}</Button>
          <Button variant="quiet" disabled={!idle} onClick={() => { setPhase("confirm_block"); }}>{c.block}</Button>
        </div>
      )}
      {status === null ? null : <p className="om-request__status" role="status">{status}</p>}
    </section>
  );
}
