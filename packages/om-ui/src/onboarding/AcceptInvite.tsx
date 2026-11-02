import { useEffect, useRef, useState } from "react";
import { canonicalInviteLink } from "@om/platform-api";
import { resolveIntent, settledOutcome } from "../conversations/services";
import type { ConversationServices } from "../conversations/services";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { OnboardingCopy } from "./copy";
import { StepHeading } from "./StepHeading";

export interface AcceptInviteProps {
  readonly services: ConversationServices;
  readonly copy: OnboardingCopy["incoming"];
  /** The token from a link that was opened (the host read it from the page address, never from a server). */
  readonly token: string;
  /** The request went out (or is saved to go out): the host shows the waiting state and drops the link. */
  readonly onAccepted: () => void;
  /** Called as the answer goes out, before anything can change: hosts note the state the answer starts from. */
  readonly onSending?: () => void;
  /** "Not now": the host forgets the link. */
  readonly onDecline: () => void;
  /** Bound on waiting for the provider before the card shows the honest "saved" state (default 15 s). */
  readonly settleTimeoutMs?: number;
}

/** How long a still-unconfirmed answer is watched in the background (the bounded wait only decides what the card says). */
const LONG_WAIT_MS = 2 ** 31 - 1;

/** "Accept this invite?" for a link someone opened. One explicit decision; nothing is sent before the person taps Accept. */
export function AcceptInvite({ services, copy, token, onAccepted, onSending, onDecline, settleTimeoutMs }: AcceptInviteProps) {
  const [busy, setBusy] = useState(false);
  /** The answer is out but the service has not confirmed it: tapping again would send a second one, so Accept stays off. */
  const [watching, setWatching] = useState(false);
  const [status, setStatus] = useState<{ readonly kind: "status" | "alert"; readonly text: string } | null>(null);
  const mounted = useRef(true);
  const abort = useRef(new AbortController());
  useEffect(() => {
    mounted.current = true;
    abort.current = new AbortController();
    const mine = abort.current;
    return () => { mounted.current = false; mine.abort(); };
  }, []);

  async function accept(): Promise<void> {
    setBusy(true);
    setStatus(null);
    try {
      const r = resolveIntent(services, "accept-invitation", { link: canonicalInviteLink(token) });
      if (r.status !== "intent") { setStatus({ kind: "alert", text: copy.parseError }); return; }
      onSending?.();
      const handle = await services.client.dispatch(r.intent);
      const settle = (timeoutMs?: number) => settledOutcome(services, handle.operationId, { signal: abort.current.signal, ...(timeoutMs !== undefined ? { timeoutMs } : settleTimeoutMs !== undefined ? { timeoutMs: settleTimeoutMs } : {}) });
      const outcome = await settle();
      if (!mounted.current) return;
      if (outcome.kind === "PendingNetwork" || outcome.kind === "PendingReconciliation") {
        // A slow or unreachable service: say so, and finish by itself if it answers while this card is still open.
        setStatus({ kind: "status", text: copy.queued });
        setWatching(true);
        void settle(LONG_WAIT_MS).then((late) => {
          if (!mounted.current) return;
          if (late.kind === "Committed") onAccepted();
          else if (late.kind === "Rejected") { setWatching(false); setStatus({ kind: "alert", text: copy.rejected[late.reason] }); }
        }, () => undefined);
        return;
      }
      if (outcome.kind === "Committed") onAccepted();
      else setStatus({ kind: "alert", text: outcome.kind === "Rejected" ? copy.rejected[outcome.reason] : copy.failed });
    } catch {
      if (mounted.current) setStatus({ kind: "alert", text: copy.failed });
    } finally {
      if (mounted.current) setBusy(false);
    }
  }

  return (
    <section className="om-onb__step om-onb__incoming" aria-labelledby="om-onb-incoming-h">
      <StepHeading id="om-onb-incoming-h">{copy.heading}</StepHeading>
      <Text as="p">{copy.body}</Text>
      <div className="om-onb__actions">
        <Button variant="primary" disabled={busy || watching} onClick={() => { void accept(); }}>{busy || watching ? copy.accepting : copy.accept}</Button>
        <Button variant="quiet" disabled={busy || watching} onClick={onDecline}>{copy.decline}</Button>
      </div>
      {status ? <p role={status.kind === "alert" ? "alert" : "status"}>{status.text}</p> : null}
    </section>
  );
}
