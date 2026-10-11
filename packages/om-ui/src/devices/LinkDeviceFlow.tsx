import { useEffect, useState } from "react";
import { isTerminalOutcome, parseDeviceLink } from "@om/platform-api";
import type { OperationId, PendingOperationView } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { DevicesCopy } from "./copy";
import { FocusHeading } from "./FocusHeading";
import { OperationStatus, useLiveQuery, useOperation, type SettingsServices } from "./operation";

export interface LinkDeviceFlowProps {
  readonly services: SettingsServices;
  readonly copy: DevicesCopy;
  readonly scanQr?: () => Promise<string | null>;
  readonly onCancel: () => void;
  /** Called once Core has produced a link request to review. Opening an offer never links anything by itself. */
  readonly onReview: (operationId: OperationId) => void;
}

/** Existing device, step 1: open the new device's offer by scanning its QR code or pasting its link. */
export function LinkDeviceFlow({ services, copy, scanQr, onCancel, onReview }: LinkDeviceFlowProps) {
  const c = copy.link;
  const [pasted, setPasted] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const op = useOperation(services);
  const committed = op.state.phase === "observing" && op.state.outcome?.kind === "Committed" ? op.state.operationId : null;

  useEffect(() => { if (committed !== null) onReview(committed); }, [committed, onReview]);

  const open = (text: string): void => {
    const parsed = parseDeviceLink(text);
    if (!parsed.ok) { setNote(c.parseError[parsed.reason]); return; }
    setNote(null);
    void op.run("request-device-link", { link: text });
  };
  const scan = async (): Promise<void> => {
    if (!scanQr) return;
    const payload = await scanQr();
    if (payload === null) { setNote(c.scanNothing); return; }
    open(payload);
  };

  return (
    <section className="om-settings__step" aria-labelledby="om-link-h">
      <Button variant="quiet" onClick={onCancel}>{copy.back}</Button>
      <FocusHeading id="om-link-h">{c.heading}</FocusHeading>
      <Text as="p">{c.body}</Text>
      {scanQr ? <div className="om-settings__actions"><Button variant="primary" disabled={op.locked} onClick={() => { void scan(); }}>{c.scan}</Button></div> : null}
      <form className="om-settings__form" onSubmit={(e) => { e.preventDefault(); open(pasted); }}>
        <label className="om-settings__field">
          <span>{c.pasteLabel}</span>
          <input
            type="text" className="om-settings__input" value={pasted} aria-describedby="om-link-hint" inputMode="url"
            autoComplete="off" autoCapitalize="off" autoCorrect="off" spellCheck={false}
            onChange={(e) => { setPasted(e.target.value); }}
          />
        </label>
        <Text as="p" size="sm" tone="secondary" id="om-link-hint">{c.pasteHint}</Text>
        <div className="om-settings__actions">
          <Button type="submit" disabled={op.locked}>{c.open}</Button>
          <Button variant="quiet" onClick={onCancel}>{c.cancel}</Button>
        </div>
      </form>
      {note === null ? null : <p role="alert">{note}</p>}
      <OperationStatus state={op.state} copy={copy.outcome} done={c.done} />
    </section>
  );
}

export interface LinkReviewProps {
  readonly services: SettingsServices;
  readonly copy: DevicesCopy;
  /** The RequestDeviceLink operation whose request is reviewed. */
  readonly operationId: OperationId;
  readonly ops: readonly PendingOperationView[];
  readonly onDone: () => void;
}

/**
 * Existing device, step 2 (NoSilentLink): show who is asking and the code Core derived, and require an explicit
 * decision. Nothing here approves automatically, and there is no option to link without telling the other devices.
 */
export function LinkReview({ services, copy, operationId, ops, onDone }: LinkReviewProps) {
  const c = copy.link;
  const live = useLiveQuery(services, { kind: "GetLinkRequest", ctx: services.ctx, operationId }, `link-${operationId}`);
  const op = useOperation(services);
  const [decision, setDecision] = useState<"approve" | "reject" | null>(null);
  // After a restart, a decision may still be reconciling in the durable log: never offer a second one (UI-12, UI-13).
  const unresolved = ops.some((o) => (o.intentKind === "ApproveDeviceLink" || o.intentKind === "RejectDeviceLink") && !isTerminalOutcome(o.outcome));

  if (live.status === "loading") return <Text role="status">{copy.loading}</Text>;
  const request = live.status === "ready" ? live.value.value : null;
  if (request === null) return (
    <section className="om-settings__step" aria-labelledby="om-review-h">
      <FocusHeading id="om-review-h">{c.reviewHeading}</FocusHeading>
      <Text role="alert">{copy.loadFailed}</Text>
      <Button onClick={onDone}>{c.done}</Button>
    </section>
  );

  const decide = (which: "approve" | "reject"): void => {
    setDecision(which);
    void op.run(which === "approve" ? "approve-device-link" : "reject-device-link", { requestId: request.requestId });
  };
  const open = request.state === "awaiting_approval" && !op.locked && !unresolved && !(op.state.phase === "observing" && op.state.outcome?.kind === "Committed");

  return (
    <section className="om-settings__step" aria-labelledby="om-review-h">
      <FocusHeading id="om-review-h">{c.reviewHeading}</FocusHeading>
      <Text as="p">{c.who(request.deviceLabel)}</Text>
      <div className="om-devices__code" role="group" aria-labelledby="om-review-code-l">
        <Text as="p" size="sm" weight="bold" id="om-review-code-l">{c.codeLabel}</Text>
        <output className="om-devices__digits" translate="no">{request.verificationCode}</output>
        <Text as="p" size="sm" tone="secondary">{c.codeHint}</Text>
      </div>
      <Text as="p" className="om-devices__state" data-state={request.state}>{c.requestState[request.state]}</Text>
      {request.state === "awaiting_approval" ? (
        <>
          <Text as="p" size="sm" tone="secondary">{c.expires(request.expiresAt)}</Text>
          <p className="om-devices__warning">{c.warning}</p>
          <Text as="p">{c.announce}</Text>
        </>
      ) : null}
      {open ? (
        <div className="om-settings__actions">
          <Button variant="primary" onClick={() => { decide("approve"); }}>{c.approve}</Button>
          <Button onClick={() => { decide("reject"); }}>{c.reject}</Button>
        </div>
      ) : null}
      <OperationStatus state={op.state} copy={copy.outcome} done={decision === "reject" ? c.rejectedDone : c.linked} />
      <div className="om-settings__actions"><Button variant="quiet" onClick={onDone}>{c.done}</Button></div>
    </section>
  );
}
