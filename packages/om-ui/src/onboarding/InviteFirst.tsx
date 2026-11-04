import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { canonicalInviteLink, parseInviteLink } from "@om/platform-api";
import type { InviteView, OperationId } from "@om/platform-api";
import { resolveIntent, settledOutcome } from "../conversations/services";
import type { ConversationServices } from "../conversations/services";
import { Button } from "../shell/primitives/Button";
import { Icon } from "../shell/primitives/icons";
import { Text } from "../shell/primitives/Text";
import { VisuallyHidden } from "../shell/primitives/VisuallyHidden";
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
  /** The form of the invite a person sends (Core's own `om:` link is not something a phone camera or a chat app opens). */
  readonly shareLink?: (coreLink: string) => string;
  readonly onAccepted: () => void;
  /** Omit where the step is not skippable because it is the screen itself (the empty Chats list). */
  readonly onSkip?: () => void;
  /** Bound on waiting for the provider before the step shows the honest "queued" state (default 15 s). */
  readonly settleTimeoutMs?: number;
  /** Which choice takes focus first when this opens inside a sheet (a result chosen in the Omnibar). Default: share. */
  readonly focusChoice?: "share" | "scan" | "qr";
  /** Text to start the "have an invite" field with (a link typed into the Omnibar). */
  readonly initialLink?: string;
}

/** How long an unconfirmed invite is watched in the background (the bounded wait only decides what the step says). */
const LONG_WAIT_MS = 2 ** 31 - 1;

/** Zero-contact empty state. Contacts come only from invitations; there is no address-book path here. */
export function InviteFirst({ services, platform, copy, renderQr, scanQr, shareLink, onAccepted, onSkip, settleTimeoutMs, focusChoice = "share", initialLink }: InviteFirstProps) {
  const [invite, setInvite] = useState<InviteView | null>(null);
  const [showQr, setShowQr] = useState(false);
  const [status, setStatus] = useState<{ readonly kind: "status" | "alert" | "done"; readonly text: string } | null>(null);
  const [pasted, setPasted] = useState(initialLink ?? "");
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
  const say = (kind: "status" | "alert" | "done", text: string): void => { if (mounted.current) setStatus({ kind, text }); };
  // A confirmation is a green tick that fades by itself; it is not text to read or dismiss.
  useEffect(() => {
    if (status?.kind !== "done") return;
    const t = setTimeout(() => { setStatus(null); }, 2600);
    return () => { clearTimeout(t); };
  }, [status]);

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
      const r = resolveIntent(services, "accept-invitation", { link: canonicalInviteLink(parsed.token) });
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
      if (cap.state === "unavailable") { await copyLink(view, copy.sharedByCopy); return; } // no share sheet here: the link is copied and shown
      const res = await cap.api.share({ title: copy.shareTitle, text: copy.shareText, url: linkOf(view) });
      if (res === "failed") say("alert", copy.shareFailed);
    } catch {
      say("alert", copy.shareFailed);
    } finally {
      if (mounted.current) setBusy(false);
    }
  }

  const linkOf = (view: InviteView): string => (shareLink ? shareLink(view.link) : view.link);

  /** Copy goes through the Action Registry like every platform capability; the invite link is not secret, so it is not marked sensitive. */
  async function copyLink(view: InviteView | null, done: string): Promise<void> {
    const clipboard = platform.clipboard();
    if (!view) return;
    const r = services.registry.resolve({ actionId: "copy-text", params: { sensitive: false }, ctx: services.ctx, origin: "direct" });
    if (clipboard.state === "unavailable" || r.status !== "platform" || r.request.capability !== "Clipboard") { say("alert", copy.copyFailed); return; }
    try {
      const res = await clipboard.api.write(linkOf(view), { sensitive: false });
      if (res === "written") say("done", done); else say("alert", copy.copyFailed);
    } catch { say("alert", copy.copyFailed); }
  }

  async function copyClick(): Promise<void> {
    setBusy(true);
    try { await copyLink(await ensureInvite(), copy.copied); } finally { if (mounted.current) setBusy(false); }
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
      {copy.body ? <Text as="p">{copy.body}</Text> : null}
      <div className="om-onb__actions" role="group" aria-label={copy.heading} data-hidden-actions={hidden.join(" ")}>
        <Button variant="primary" disabled={busy} data-sheet-initial={focusChoice === "share" ? "" : undefined} onClick={run(share)}><Icon name="share" size={20} />{copy.share}</Button>
        {scanQr && !hidden.includes("camera_unavailable") ? <Button disabled={busy} data-sheet-initial={focusChoice === "scan" ? "" : undefined} onClick={run(scan)}><Icon name="scan" size={20} />{copy.scan}</Button> : null}
        {renderQr ? <Button disabled={busy} data-sheet-initial={focusChoice === "qr" ? "" : undefined} onClick={run(qr)}><Icon name="qr" size={20} />{copy.showQr}</Button> : null}
        <Button variant="quiet" disabled={busy} onClick={run(copyClick)}><Icon name="link" size={20} />{copy.copyLink}</Button>
      </div>
      {status?.kind === "done" ? (
        <p className="om-onb__done" role="status"><span className="om-onb__done-mark" aria-hidden="true"><Icon name="check" size={20} /></span><VisuallyHidden>{status.text}</VisuallyHidden></p>
      ) : status ? <p className="om-onb__status" role={status.kind === "alert" ? "alert" : "status"}>{status.text}</p> : null}
      {invite ? (
        <div className="om-onb__invite">
          <Text as="p" size="sm" weight="bold" id="om-onb-invite-link-l">{copy.linkLabel}</Text>
          <code className="om-onb__link" aria-labelledby="om-onb-invite-link-l" translate="no">{linkOf(invite)}</code>
          <Text as="p" size="sm" tone="secondary">{copy.expires(invite.expiresAt)}</Text>
          {showQr && renderQr ? <figure className="om-onb__qr" aria-label={copy.qrLabel}>{renderQr(linkOf(invite))}</figure> : null}
        </div>
      ) : null}
      <details className="om-onb__more" open={initialLink !== undefined && initialLink !== "" ? true : undefined}>
      <summary>{copy.haveInvite}</summary>
      <form className="om-onb__accept" aria-labelledby="om-onb-accept-h" onSubmit={(e) => { e.preventDefault(); void acceptLink(pasted); }}>
        <Text as="h3" size="md" weight="bold" id="om-onb-accept-h">{copy.acceptHeading}</Text>
        <label className="om-onb__field">
          <span>{copy.acceptLabel}</span>
          <input
            type="text" className="om-onb__input" value={pasted} inputMode="url"
            autoComplete="off" autoCapitalize="off" autoCorrect="off" spellCheck={false}
            onChange={(e) => { setPasted(e.target.value); }}
          />
        </label>
        <Button type="submit" disabled={busy}>{copy.accept}</Button>
      </form>
      </details>
      {onSkip ? <div className="om-onb__actions"><Button variant="quiet" disabled={busy} onClick={onSkip}>{copy.skip}</Button></div> : null}
    </section>
  );
}
