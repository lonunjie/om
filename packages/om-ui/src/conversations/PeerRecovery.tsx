import { useId, useState } from "react";
import type { ConversationId, PeerRecoveryView } from "@om/platform-api";
import { OperationStatus, useOperation } from "../devices/operation";
import type { RecoveryContactCopy } from "../recovery/copy";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { ConversationServices } from "./services";

export interface PeerRecoveryProps {
  readonly services: ConversationServices;
  readonly conversationId: ConversationId;
  readonly name: string;
  readonly recovery: PeerRecoveryView;
  readonly copy: RecoveryContactCopy;
}

/**
 * One calm banner in a conversation with a contact who is getting their account back on a new device (ADR-0190 sections 4 and 9).
 * It shows Core's state and days left and never computes time. "This isn't them" and "Yes, it's them" are typed intents
 * through the Action Registry from a direct tap, each with its Core-issued confirmation. Nothing from the new device is shown here.
 * `stale` is dropped by Core's view and shows nothing; `accepted`, `vetoed` and `declined` are one quiet line, not a banner.
 */
export function PeerRecovery({ services, conversationId, name, recovery, copy }: PeerRecoveryProps) {
  const op = useOperation(services);
  const [check, setCheck] = useState(false);
  const detailId = useId();
  const titleId = useId();
  const params = { conversationId, requestId: recovery.requestId };

  switch (recovery.state) {
    case "stale": return null;
    case "accepted": return recovery.notifyOnCompletion ? <p className="om-conv-view__relationship" role="status" data-peer-recovery="accepted">{copy.accepted(name)}</p> : null;
    case "vetoed": return <p className="om-conv-view__relationship" role="status" data-peer-recovery="vetoed">{copy.vetoed(name)}</p>;
    case "declined": return <p className="om-conv-view__relationship" role="status" data-peer-recovery="declined">{copy.declined(name)}</p>;
    case "waiting":
    case "ready": break;
  }

  const asks = recovery.state === "ready" && recovery.needsConfirm;
  const decline = recovery.canDecline ? (
    <Button variant={asks ? "secondary" : "quiet"} disabled={op.locked} onClick={() => { void op.run("decline-peer-recovery", params); }}>{asks ? copy.ready.decline : copy.waiting.decline}</Button>
  ) : null;

  return (
    <section className="om-peer-recovery" aria-labelledby={titleId} data-peer-recovery={recovery.state} data-needs-confirm={asks ? "true" : undefined}>
      <Text as="h3" size="md" weight="bold" id={titleId}>{asks ? copy.ready.title(name) : copy.waiting.title(name)}</Text>
      {asks ? (
        <>
          <Text as="p">{copy.ready.consent(name)}</Text>
          <div className="om-peer-recovery__actions">
            <Button variant="primary" disabled={op.locked} onClick={() => { void op.run("confirm-peer-recovery", params); }}>{copy.ready.confirm}</Button>
            {decline}
          </div>
        </>
      ) : (
        <>
          <Text as="p">{recovery.state === "ready" ? copy.ready.checking : copy.waiting.body(copy.days(recovery.daysLeft))}</Text>
          <div className="om-peer-recovery__actions">
            <Button variant="secondary" aria-expanded={check} aria-controls={detailId} onClick={() => { setCheck((v) => !v); }}>{copy.waiting.check(name)}</Button>
            {decline}
          </div>
          <Text as="p" size="sm" tone="secondary" id={detailId} hidden={!check}>{copy.waiting.checkDetail(name)}</Text>
          {recovery.canDecline ? <Text as="p" size="sm" tone="secondary">{copy.waiting.declineNote(name)}</Text> : null}
        </>
      )}
      <OperationStatus state={op.state} copy={copy.outcome} done="" />
    </section>
  );
}
