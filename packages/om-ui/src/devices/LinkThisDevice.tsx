import type { ReactNode } from "react";
import type { OperationId } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { LinkThisDeviceCopy } from "./copy";
import { FocusHeading } from "./FocusHeading";
import { OperationStatus, useLiveQuery, useOperation, type SettingsServices } from "./operation";
import "./devices.css";

export interface LinkThisDeviceProps {
  readonly services: SettingsServices;
  readonly copy: LinkThisDeviceCopy;
  readonly renderQr?: (payload: string) => ReactNode;
}

/**
 * New device: create a link offer only when asked, show it as a QR code and a link, then show the same verification
 * code the existing device shows. Approval happens only on the existing device; this screen just reports Core's state.
 */
export function LinkThisDevice({ services, copy, renderQr }: LinkThisDeviceProps) {
  const op = useOperation(services);
  const offerOp = op.state.phase === "observing" && op.state.outcome?.kind === "Committed" ? op.state.operationId : null;
  return (
    <section className="om-settings__step" aria-labelledby="om-offer-h">
      <FocusHeading id="om-offer-h">{copy.heading}</FocusHeading>
      <Text as="p">{copy.body}</Text>
      {offerOp === null ? (
        <>
          <div className="om-settings__actions">
            <Button variant="primary" disabled={op.locked} onClick={() => { void op.run("offer-device-link", {}); }}>{copy.start}</Button>
          </div>
          <OperationStatus state={op.state} copy={copy.outcome} done="" />
        </>
      ) : <Offer services={services} copy={copy} operationId={offerOp} {...(renderQr ? { renderQr } : {})} />}
    </section>
  );
}

function Offer({ services, copy, operationId, renderQr }: { readonly services: SettingsServices; readonly copy: LinkThisDeviceCopy; readonly operationId: OperationId; readonly renderQr?: (payload: string) => ReactNode }) {
  const live = useLiveQuery(services, { kind: "GetLinkOffer", ctx: services.ctx, operationId }, `offer-${operationId}`);
  if (live.status !== "ready" || live.value.value === null) return <Text role={live.status === "loading" ? "status" : "alert"}>{live.status === "loading" ? copy.outcome.working : copy.outcome.failed}</Text>;
  const offer = live.value.value;
  return (
    <>
      {offer.state === "waiting_for_scan" ? (
        <div className="om-devices__offer">
          {renderQr ? <figure className="om-devices__qr" aria-label={copy.qrLabel}>{renderQr(offer.link)}</figure> : null}
          <Text as="p" size="sm" weight="bold" id="om-offer-link-l">{copy.linkLabel}</Text>
          <code className="om-devices__link" aria-labelledby="om-offer-link-l" translate="no">{offer.link}</code>
          <Text as="p" size="sm" tone="secondary">{copy.expires(offer.expiresAt)}</Text>
        </div>
      ) : null}
      {offer.verificationCode === null ? null : (
        <div className="om-devices__code" role="group" aria-labelledby="om-offer-code-l">
          <Text as="p" size="sm" weight="bold" id="om-offer-code-l">{copy.codeLabel}</Text>
          <output className="om-devices__digits" translate="no">{offer.verificationCode}</output>
          <Text as="p" size="sm" tone="secondary">{copy.codeHint}</Text>
        </div>
      )}
      <p className="om-devices__state" role="status" data-state={offer.state}>{copy.state[offer.state]}</p>
    </>
  );
}
