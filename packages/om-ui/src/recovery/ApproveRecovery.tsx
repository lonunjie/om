import type { ReactNode } from "react";
import type { OperationId } from "@om/platform-api";
import { FocusHeading } from "../devices/FocusHeading";
import { OperationStatus, useLiveQuery, useOperation, type SettingsServices } from "../devices/operation";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { ApproveRecoveryCopy } from "./lostCopy";
import "../devices/devices.css";

export interface ApproveRecoveryProps {
  readonly services: SettingsServices;
  readonly copy: ApproveRecoveryCopy;
  /** Draws the QR code for the link. Without it the link is shown as text only. */
  readonly renderQr?: (payload: string) => ReactNode;
  readonly onClose: () => void;
}

/**
 * A device that still works helps the person get the account back on a new device (ADR-0203): it shows a private link for the new
 * device, compares the six digits with it, and only then does the person confirm the exact request. Confirming is the controller's
 * signature; it is never offered from a suggestion and never before the digits are shown.
 */
export function ApproveRecovery({ services, copy, renderQr, onClose }: ApproveRecoveryProps) {
  const op = useOperation(services);
  const offerOp = op.state.phase === "observing" && op.state.outcome?.kind === "Committed" ? op.state.operationId : null;
  return (
    <section className="om-settings__step" aria-labelledby="om-approve-h">
      <Button variant="quiet" onClick={onClose}>{copy.back}</Button>
      <FocusHeading id="om-approve-h">{copy.heading}</FocusHeading>
      {offerOp === null ? (
        <>
          <Text as="p">{copy.body}</Text>
          <div className="om-settings__actions">
            <Button variant="primary" disabled={op.locked} onClick={() => { void op.run("offer-recovery-approval", {}); }}>{copy.show}</Button>
            <Button variant="quiet" onClick={onClose}>{copy.back}</Button>
          </div>
          <OperationStatus state={op.state} copy={copy.outcome} done="" />
        </>
      ) : <Offer services={services} copy={copy} operationId={offerOp} onClose={onClose} {...(renderQr ? { renderQr } : {})} />}
    </section>
  );
}

function Offer({ services, copy, operationId, renderQr, onClose }: { readonly services: SettingsServices; readonly copy: ApproveRecoveryCopy; readonly operationId: OperationId; readonly renderQr?: (payload: string) => ReactNode; readonly onClose: () => void }) {
  const live = useLiveQuery(services, { kind: "GetRecoveryApprovalOffer", ctx: services.ctx, operationId }, `approve-${operationId}`);
  const op = useOperation(services);
  if (live.status !== "ready" || live.value.value === null) return <Text role={live.status === "loading" ? "status" : "alert"}>{live.status === "loading" ? copy.outcome.working : copy.outcome.failed}</Text>;
  const offer = live.value.value;
  const asking = offer.state === "awaiting_confirmation" && offer.requestId != null;
  return (
    <>
      {offer.link ? (
        <section className="om-settings__group" aria-labelledby="om-approve-link-h">
          <Text as="h2" size="lg" weight="bold" id="om-approve-link-h">{copy.linkHeading}</Text>
          {renderQr ? <figure data-kind="recovery-approval">{renderQr(offer.link)}</figure> : null}
          <Text as="p" size="sm" tone="secondary">{copy.notInvitation}</Text>
          <code className="om-devices__link" translate="no">{offer.link}</code>
        </section>
      ) : null}
      {offer.state === "waiting_for_device" ? <Text role="status">{copy.waiting}</Text> : null}
      {asking && offer.verificationCode ? (
        <div className="om-devices__code" role="group" aria-labelledby="om-approve-code-l">
          <Text as="p" size="sm" weight="bold" id="om-approve-code-l">{copy.codeLabel}</Text>
          <output className="om-devices__digits" translate="no">{offer.verificationCode}</output>
          <Text as="p" size="sm" tone="secondary">{copy.codeHint}</Text>
          <Text as="p" size="sm">{copy.confirmNote}</Text>
          <div className="om-settings__actions">
            <Button variant="primary" disabled={op.locked} onClick={() => { void op.run("confirm-recovery", { requestId: offer.requestId ?? "" }); }}>{copy.confirm}</Button>
            <Button variant="quiet" disabled={op.locked} onClick={() => { void op.run("veto-recovery", { requestId: offer.requestId ?? "" }); }}>{copy.decline}</Button>
          </div>
          <OperationStatus state={op.state} copy={copy.outcome} done="" />
        </div>
      ) : null}
      <p className="om-devices__state" role={offer.state === "failed" || offer.state === "expired" || offer.state === "declined" ? "alert" : "status"} data-state={offer.state}>{copy.state[offer.state]}</p>
      <div className="om-settings__actions"><Button variant="quiet" onClick={onClose}>{copy.back}</Button></div>
    </>
  );
}
