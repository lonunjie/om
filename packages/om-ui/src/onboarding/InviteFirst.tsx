import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { parseInviteLink } from "@om/platform-api";
import type { InviteView, OperationId } from "@om/platform-api";
import { resolveIntent, settledOutcome } from "../conversations/services";
import type { ConversationServices } from "../conversations/services";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { InviteUnavailableCode, OnboardingCopy } from "./copy";
import type { OnboardingPlatform } from "./platform";
import { StepHeading } from "./StepHeading";

export interface InviteFirstProps {
  readonly services: ConversationServices;
  readonly platform: OnboardingPlatform;
  readonly copy: OnboardingCopy["invite"];
  /** QR encode/decode libraries are not bundled: without these slots the QR controls are hidden, not broken. */
  readonly renderQr?: (payload: string) => ReactNode;
  readonly scanQr?: () => Promise<string | null>;
  readonly onAccepted: () => void;
  readonly onSkip: () => void;
  /** Bound on waiting for the provider before the step shows the honest "queued" state (default 15 s). */
  readonly settleTimeoutMs?: number;
}

/** How long an unconfirmed invite is watched in the background (the bounded wait only decides what the step says). */
const LONG_WAIT_MS = 2 ** 31 - 1;

/** Zero-contact empty state. Contacts come only from invitations; there is no address-book path here. */
export function InviteFirst({ services, platform, copy, renderQr, scanQr, onAccepted, onSkip, settleTimeoutMs }: InviteFirstProps) {
  const [invite, setInvite] = useState<InviteView | null>(null);
  const [showQr, setShowQr] = useState(false);
  const [status, setStatus] = useState<{ readonly kind: "status" | "alert"; readonly text: string } | null>(null);
  const [pasted, setPasted] = useState("");
  const [busy, setBusy] = useState(false);
  const mounted = useRef(true);
  const inviteRef = useRef<InviteView | null>(null);
  /** An invite the provider has not confirmed yet (a slow Home): later clicks wait on it instead of creating another. */
  const pendingInvite = useRef<OperationId | null>(null);
  const abort = useRef(new AbortController());
  // Leaving the step ends any wait for the provider and drops its observer.
  useEffect(() => {
    mounted.current = true;
    abort.current = new AbortController();
    const mine = abort.current;
    return () => { mounted.current = false; mine.abort(); };
  }, []);
  const settle = (id: Parameters<typeof settledOutcome>[1]) => settledOutcome(services, id, { signal: abort.current.signal, ...(settleTimeoutMs !== undefined ? { timeoutMs: settleTimeoutMs } : {}) });
  const say = (kind: "status" | "alert", text: string): void => { if (mounted.current) setStatus({ kind, text }); };

  /** Keeps watching an unconfirmed invite after the bounded wait, so the link appears when the Home finally answers. */
  function keepWatching(operationId: OperationId): void {
    const have = (): boolean => inviteRef.current !== null || !mounted.current; // re-read after every await
    void settledOutcome(services, operationId, { signal: abort.current.signal, timeoutMs: LONG_WAIT_MS }).then(async (o) => {
      if (o.kind !== "Committed" || have()) return;
      const view = await services.client.query({ kind: "GetInvite", ctx: services.ctx, operationId });
      if (!view || have()) return;
      inviteRef.current = view;
      pendingInvite.current = null;
      setInvite(view);
      setStatus(null);
    }, () => undefined);
  }

  async function ensureInvite(): Promise<InviteView | null> {
    if (inviteRef.current) return inviteRef.current;
    try {
      let operationId = pendingInvite.current;
      if (operationId === null) {
        const r = resolveIntent(services, "create-invite", {});
        if (r.status !== "intent") throw new Error("blocked");
        operationId = (await services.client.dispatch(r.intent)).operationId;
      }
      const outcome = await settle(operationId);
      if (outcome.kind === "PendingNetwork" || outcome.kind === "PendingReconciliation") {
        // No provider yet, or a slow one: the invite is queued on this device, not usable to share. Later clicks wait on
        // this same operation, and the link shows up by itself if the provider confirms it.
        if (pendingInvite.current === null) keepWatching(operationId);
        pendingInvite.current = operationId;
        say("status", copy.queued);
        return null;
      }
      pendingInvite.current = null;
      if (outcome.kind !== "Committed") throw new Error("not committed");
      const view = await services.client.query({ kind: "GetInvite", ctx: services.ctx, operationId });
      if (!view) throw new Error("missing");
      inviteRef.current = view;
      if (mounted.current) setInvite(view);
      return view;
    } catch {
      say("alert", copy.failed);
      return null;
    }
  }

  async function acceptLink(text: string): Promise<void> {
    const parsed = parseInviteLink(text);
    if (!parsed.ok) { say("alert", copy.parseError[parsed.reason]); return; }
    setBusy(true);
    try {
      const r = resolveIntent(services, "accept-invitation", { link: text });
      if (r.status !== "intent") throw new Error("blocked");
      const handle = await services.client.dispatch(r.intent);
      const outcome = await settle(handle.operationId);
      if (!mounted.current) return;
      if (outcome.kind === "Committed") onAccepted();
      else if (outcome.kind === "PendingNetwork" || outcome.kind === "PendingReconciliation") say("status", copy.queued);
      else say("alert", outcome.kind === "Rejected" ? copy.rejected[outcome.reason] : copy.failed);
    } catch {
      say("alert", copy.failed);
    } finally {
      if (mounted.current) setBusy(false);
    }
  }

  async function share(): Promise<void> {
    setBusy(true);
    try {
      const view = await ensureInvite();
      if (!view) return;
      const cap = platform.share();
      if (cap.state === "unavailable") return; // the link text is already on screen as the fallback
      const res = await cap.api.share({ title: copy.shareTitle, text: copy.shareText, url: view.link });
      if (res === "cancelled") say("status", copy.shareCancelled);
      else if (res === "failed") say("alert", copy.shareFailed);
    } catch {
      say("alert", copy.shareFailed);
    } finally {
      if (mounted.current) setBusy(false);
    }
  }

  async function qr(): Promise<void> {
    setBusy(true);
    try { if (await ensureInvite()) setShowQr(true); } finally { if (mounted.current) setBusy(false); }
  }

  // The camera permission is requested here, from the click, never on mount.
  async function scan(): Promise<void> {
    const cam = platform.camera();
    if (cam.state === "unavailable" || !scanQr) return;
    setBusy(true);
    try {
      if ((await cam.api.requestAccess()) !== "granted") { say("alert", copy.scanDenied); return; }
      const payload = await scanQr();
      if (payload === null) { say("status", copy.scanNothing); return; }
      await acceptLink(payload);
    } catch {
      say("alert", copy.failed);
    } finally {
      if (mounted.current) setBusy(false);
    }
  }

  const hidden: InviteUnavailableCode[] = [];
  if (!renderQr) hidden.push("qr_renderer_missing");
  if (!scanQr) hidden.push("scan_slot_missing");
  else if (platform.camera().state === "unavailable") hidden.push("camera_unavailable");
  const run = (fn: () => Promise<void>) => (): void => { void fn(); };

  return (
    <section className="om-onb__step" aria-labelledby="om-onb-invite-h">
      <StepHeading id="om-onb-invite-h">{copy.heading}</StepHeading>
      <Text as="p">{copy.body}</Text>
      <div className="om-onb__actions" role="group" aria-label={copy.heading} data-hidden-actions={hidden.join(" ")}>
        <Button variant="primary" disabled={busy} onClick={run(share)}>{copy.share}</Button>
        {renderQr ? <Button disabled={busy} onClick={run(qr)}>{copy.showQr}</Button> : null}
        {scanQr && !hidden.includes("camera_unavailable") ? <Button disabled={busy} onClick={run(scan)}>{copy.scan}</Button> : null}
      </div>
      {invite ? (
        <div className="om-onb__invite">
          <Text as="p" size="sm" weight="bold" id="om-onb-invite-link-l">{copy.linkLabel}</Text>
          <code className="om-onb__link" aria-labelledby="om-onb-invite-link-l" translate="no">{invite.link}</code>
          <Text as="p" size="sm" tone="secondary">{copy.expires(invite.expiresAt)}</Text>
          {showQr && renderQr ? <figure className="om-onb__qr" aria-label={copy.qrLabel}>{renderQr(invite.link)}</figure> : null}
        </div>
      ) : null}
      <form className="om-onb__accept" aria-labelledby="om-onb-accept-h" onSubmit={(e) => { e.preventDefault(); void acceptLink(pasted); }}>
        <Text as="h3" size="md" weight="bold" id="om-onb-accept-h">{copy.acceptHeading}</Text>
        <label className="om-onb__field">
          <span>{copy.acceptLabel}</span>
          <input
            type="text" className="om-onb__input" value={pasted} aria-describedby="om-onb-accept-hint" inputMode="url"
            autoComplete="off" autoCapitalize="off" autoCorrect="off" spellCheck={false}
            onChange={(e) => { setPasted(e.target.value); }}
          />
        </label>
        <Text as="p" size="sm" tone="secondary" id="om-onb-accept-hint">{copy.acceptHint}</Text>
        <Button type="submit" disabled={busy}>{copy.accept}</Button>
      </form>
      {status ? <p role={status.kind === "alert" ? "alert" : "status"}>{status.text}</p> : null}
      <div className="om-onb__actions"><Button variant="quiet" disabled={busy} onClick={onSkip}>{copy.skip}</Button></div>
    </section>
  );
}
