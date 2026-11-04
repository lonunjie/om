import { useState } from "react";
import { isTerminalOutcome } from "@om/platform-api";
import type { OperationId, PendingOperationView } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { DevicesCopy } from "./copy";
import { FocusHeading } from "./FocusHeading";
import { OperationStatus, useLiveQuery, useOperation, type SettingsServices } from "./operation";

export interface LinkReviewProps {
  readonly services: SettingsServices;
  readonly copy: DevicesCopy;
  /** The OfferDeviceLink operation whose request is reviewed (as in Core, the existing device's request is keyed by its own offer, deviation D24). */
  readonly operationId: OperationId;
  readonly ops: readonly PendingOperationView[];
  readonly onDone: () => void;
}

/**
 * Existing device (NoSilentLink): show who is asking and the check code Core derived, and require an explicit decision.
 * The approval runs through the Action Registry, which fetches Core's single-use confirmation nonce for exactly this
 * request (`confirmWithCore`, UI-11). Nothing here approves automatically, and there is no option to link without
 * telling the other devices.
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
