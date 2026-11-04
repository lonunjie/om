import type { ReactNode } from "react";
import type { OperationId, PendingOperationView } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { DevicesCopy } from "./copy";
import { FocusHeading } from "./FocusHeading";
import { LinkReview } from "./LinkReview";
import { OperationStatus, useLiveQuery, useOperation, type SettingsServices } from "./operation";
import "./devices.css";

export interface AddDeviceProps {
  readonly services: SettingsServices;
  readonly copy: DevicesCopy;
  /** Draws the QR code for the link. Without it the link is shown as text only. */
  readonly renderQr?: (payload: string) => ReactNode;
  readonly onClose: () => void;
}

/**
 * Existing device (deviation D24): offer a link only when asked, show it as a QR code and as text, then hand over to the
 * review once the new device has asked. The new device never approves itself; approval happens here, in `LinkReview`.
 */
export function AddDevice({ services, copy, renderQr, onClose }: AddDeviceProps) {
  const c = copy.link;
  const op = useOperation(services);
  const offerOp = op.state.phase === "observing" && op.state.outcome?.kind === "Committed" ? op.state.operationId : null;
  return (
    <section className="om-settings__step" aria-labelledby="om-offer-h">
      <Button variant="quiet" onClick={onClose}>{copy.back}</Button>
      <FocusHeading id="om-offer-h">{c.heading}</FocusHeading>
      {offerOp === null ? (
        <>
          <Text as="p">{c.body}</Text>
          <div className="om-settings__actions">
            <Button variant="primary" disabled={op.locked} onClick={() => { void op.run("offer-device-link", {}); }}>{c.show}</Button>
            <Button variant="quiet" onClick={onClose}>{c.cancel}</Button>
          </div>
          <OperationStatus state={op.state} copy={copy.outcome} done="" />
        </>
      ) : <Offer services={services} copy={copy} operationId={offerOp} onClose={onClose} onAgain={op.reset} {...(renderQr ? { renderQr } : {})} />}
    </section>
  );
}

interface OfferProps {
  readonly services: SettingsServices;
  readonly copy: DevicesCopy;
  readonly operationId: OperationId;
  readonly renderQr?: (payload: string) => ReactNode;
  readonly onClose: () => void;
  readonly onAgain: () => void;
}

function Offer({ services, copy, operationId, renderQr, onClose, onAgain }: OfferProps) {
  const c = copy.link;
  const live = useLiveQuery(services, { kind: "GetLinkOffer", ctx: services.ctx, operationId }, `offer-${operationId}`);
  if (live.status !== "ready" || live.value.value === null) return <Text role={live.status === "loading" ? "status" : "alert"}>{live.status === "loading" ? copy.outcome.working : copy.outcome.failed}</Text>;
  const offer = live.value.value;
  const ops: readonly PendingOperationView[] = live.value.ops;
  return (
    <>
      {offer.state === "waiting_for_scan" ? (
        <div className="om-devices__offer">
          <Text as="p">{c.nextStep}</Text>
          <figure className="om-devices__qr" data-kind="device-link" aria-label={c.qrLabel}>
            <figcaption className="om-devices__qr-badge">{c.qrBadge}</figcaption>
            {renderQr ? renderQr(offer.link) : null}
          </figure>
          <p className="om-devices__warning">{c.qrNotInvite}</p>
          <Text as="p" size="sm" weight="bold" id="om-offer-link-l">{c.linkLabel}</Text>
          <code className="om-devices__link" aria-labelledby="om-offer-link-l" translate="no">{offer.link}</code>
          <Text as="p" size="sm" tone="secondary">{c.expires(offer.expiresAt)}</Text>
        </div>
      ) : null}
      {offer.state === "awaiting_approval" ? <LinkReview services={services} copy={copy} operationId={operationId} ops={ops} onDone={onClose} /> : null}
      <p className="om-devices__state" role="status" data-state={offer.state}>{c.offerState[offer.state]}</p>
      {offer.state === "rejected" || offer.state === "expired" ? (
        <div className="om-settings__actions">
          <Button variant="primary" onClick={onAgain}>{c.tryAgain}</Button>
          <Button variant="quiet" onClick={onClose}>{c.done}</Button>
        </div>
      ) : null}
      {offer.state === "waiting_for_scan" ? <div className="om-settings__actions"><Button variant="quiet" onClick={onClose}>{c.cancel}</Button></div> : null}
      {offer.state === "linked" ? <div className="om-settings__actions"><Button variant="primary" onClick={onClose}>{c.done}</Button></div> : null}
    </>
  );
}
