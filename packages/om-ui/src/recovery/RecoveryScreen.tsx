import { useState } from "react";
import { isTerminalOutcome } from "@om/platform-api";
import type { PendingOperationView, RecoveryCustodianCode, RecoveryRequestView, RecoveryScope, RecoveryStatusView } from "@om/platform-api";
import { FocusHeading } from "../devices/FocusHeading";
import { OperationStatus, outcomeText, useLiveQuery, useOperation, type SettingsServices } from "../devices/operation";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import { SecurityNoticeBanner } from "../shell/theme/SecurityNoticeBanner";
import type { RecoveryCopy, RecoveryPendingKind } from "./copy";
import "../devices/devices.css";

export interface RecoveryScreenProps {
  readonly services: SettingsServices;
  readonly copy: RecoveryCopy;
}

type View = { readonly kind: "status" } | { readonly kind: "start" } | { readonly kind: "stop"; readonly requestId: string };

const PENDING: readonly RecoveryPendingKind[] = ["StartRecovery", "VetoRecovery"];
const isPendingKind = (k: string): k is RecoveryPendingKind => (PENDING as readonly string[]).includes(k);

/**
 * Settings > Recovery. Status, custodians, scopes and timers are Core data. Starting or stopping a recovery happens in
 * its own step, never from a status row. The recovery key is display-once: this screen never offers to show it again.
 */
export function RecoveryScreen({ services, copy }: RecoveryScreenProps) {
  const [view, setView] = useState<View>({ kind: "status" });
  const live = useLiveQuery(services, { kind: "GetRecoveryStatus", ctx: services.ctx }, "recovery");
  const toStatus = (): void => { setView({ kind: "status" }); };

  if (live.status === "loading") return <Text role="status">{copy.loading}</Text>;
  if (live.status === "failed") return <Text role="alert">{copy.loadFailed}</Text>;
  const { value: status, ops } = live.value;
  const incoming = status.requests.filter((r) => r.direction === "incoming" && r.state === "waiting");
  const stopping = view.kind === "stop" ? status.requests.find((r) => r.requestId === view.requestId) : undefined;

  return (
    <section className="om-settings om-recovery" aria-labelledby="om-recovery-h">
      <Text as="h1" size="xl" weight="bold" id="om-recovery-h">{copy.heading}</Text>
      {incoming.map((r) => (
        <SecurityNoticeBanner key={r.requestId} reasonCode="recovery_in_progress" tone="danger" label={copy.incomingAlert}>
          {view.kind === "stop" ? null : <Button onClick={() => { setView({ kind: "stop", requestId: r.requestId }); }}>{copy.review}</Button>}
        </SecurityNoticeBanner>
      ))}
      {view.kind === "start" ? <StartRecovery services={services} copy={copy} status={status} onBack={toStatus} /> : null}
      {view.kind === "stop" && stopping ? <StopRecovery services={services} copy={copy} request={stopping} onBack={toStatus} /> : null}
      {view.kind === "status" || (view.kind === "stop" && !stopping) ? (
        <Status copy={copy} status={status} pending={ops.filter((o) => isPendingKind(o.intentKind) && !isTerminalOutcome(o.outcome))} onView={setView} />
      ) : null}
    </section>
  );
}

interface StatusProps {
  readonly copy: RecoveryCopy;
  readonly status: RecoveryStatusView;
  readonly pending: readonly PendingOperationView[];
  readonly onView: (v: View) => void;
}

function Status({ copy, status, pending, onView }: StatusProps) {
  return (
    <>
      <Text as="p" data-policy={status.policy}>{copy.policy[status.policy]}</Text>
      <section className="om-settings__group" aria-labelledby="om-recovery-cust-h">
        <Text as="h2" size="lg" weight="bold" id="om-recovery-cust-h">{copy.custodiansHeading}</Text>
        <ul className="om-settings__list">
          {status.custodians.map((c) => (
            <li key={c.code}>{copy.custodian[c.code]}{c.distributionBound ? ` ${copy.distributionBound}` : ""}</li>
          ))}
        </ul>
        <Text as="p">{status.recoveryKeySaved ? copy.recoveryKeySaved : copy.recoveryKeyNone}</Text>
        <Text as="p" size="sm" tone="secondary">{copy.recoveryKeyOnce}</Text>
      </section>
      <section className="om-settings__group" aria-labelledby="om-recovery-scope-h">
        <Text as="h2" size="lg" weight="bold" id="om-recovery-scope-h">{copy.scopesHeading}</Text>
        <ul className="om-settings__list">
          {status.scopes.map((s) => <li key={s}><strong>{copy.scope[s].title}</strong> {copy.scope[s].body}</li>)}
        </ul>
        <Text as="p">{copy.delay(status.activationDelayMs)}</Text>
      </section>
      {pending.length > 0 ? (
        <section className="om-settings__group" aria-labelledby="om-recovery-pending-h">
          <Text as="h2" size="lg" weight="bold" id="om-recovery-pending-h">{copy.pendingHeading}</Text>
          <ul className="om-settings__list">
            {pending.map((o) => (
              <li key={o.operationId} data-outcome={o.outcome.kind}>{isPendingKind(o.intentKind) ? copy.pendingKind[o.intentKind] : null}: {outcomeText(o.outcome, copy.outcome, "")}</li>
            ))}
          </ul>
        </section>
      ) : null}
      {status.requests.length > 0 ? (
        <section className="om-settings__group" aria-labelledby="om-recovery-req-h">
          <Text as="h2" size="lg" weight="bold" id="om-recovery-req-h">{copy.requestsHeading}</Text>
          <ul className="om-settings__rows">
            {status.requests.map((r) => <RequestRow key={r.requestId} copy={copy} request={r} onStop={() => { onView({ kind: "stop", requestId: r.requestId }); }} />)}
          </ul>
        </section>
      ) : null}
      {status.custodians.length > 0 && status.policy !== "off_explicit" ? (
        <div className="om-settings__actions"><Button onClick={() => { onView({ kind: "start" }); }}>{copy.start.action}</Button></div>
      ) : null}
    </>
  );
}

function RequestRow({ copy, request, onStop }: { readonly copy: RecoveryCopy; readonly request: RecoveryRequestView; readonly onStop: () => void }) {
  return (
    <li className="om-recovery__request" data-state={request.state} data-direction={request.direction}>
      <Text as="p" weight="bold">{copy.direction[request.direction]}: {copy.scope[request.scope].title}</Text>
      <Text as="p" size="sm">{copy.requestState[request.state]} · {copy.custodian[request.custodian]}</Text>
      <Text as="p" size="sm" tone="secondary">{copy.started(request.startedAt)}</Text>
      {request.state === "waiting" ? (
        <Text as="p" size="sm" tone="secondary">{request.activatesAt === null ? copy.activatesUnknown : copy.activatesAt(request.activatesAt)}</Text>
      ) : null}
      {request.canStop ? <Button onClick={onStop}>{request.direction === "incoming" ? copy.stopIncoming : copy.cancelOutgoing}</Button> : null}
    </li>
  );
}

function StartRecovery({ services, copy, status, onBack }: { readonly services: SettingsServices; readonly copy: RecoveryCopy; readonly status: RecoveryStatusView; readonly onBack: () => void }) {
  const c = copy.start;
  const [scope, setScope] = useState<RecoveryScope | null>(null);
  const [custodian, setCustodian] = useState<RecoveryCustodianCode | null>(status.custodians.length === 1 ? status.custodians[0]?.code ?? null : null);
  const op = useOperation(services);
  const started = op.state.phase !== "idle" && op.state.phase !== "failed";
  return (
    <section className="om-settings__step" aria-labelledby="om-recovery-start-h">
      <Button variant="quiet" onClick={onBack}>{copy.back}</Button>
      <FocusHeading id="om-recovery-start-h">{c.heading}</FocusHeading>
      <Text as="p">{c.body}</Text>
      <form className="om-settings__form" onSubmit={(e) => { e.preventDefault(); if (scope !== null && custodian !== null && !op.locked) void op.run("start-recovery", { scope, custodian }); }}>
        <fieldset className="om-settings__radios" disabled={started}>
          <legend>{c.scopeLegend}</legend>
          {status.scopes.map((s) => (
            <label key={s} className="om-settings__radio">
              <input type="radio" name="om-recovery-scope" value={s} checked={scope === s} onChange={() => { setScope(s); }} />
              <span className="om-settings__radio-text"><strong>{copy.scope[s].title}</strong><span>{copy.scope[s].body}</span></span>
            </label>
          ))}
        </fieldset>
        <fieldset className="om-settings__radios" disabled={started}>
          <legend>{c.custodianLegend}</legend>
          {status.custodians.map((k) => (
            <label key={k.code} className="om-settings__radio">
              <input type="radio" name="om-recovery-custodian" value={k.code} checked={custodian === k.code} onChange={() => { setCustodian(k.code); }} />
              <span>{copy.custodian[k.code]}</span>
            </label>
          ))}
        </fieldset>
        {scope === null ? null : (
          <div className="om-settings__confirm" role="group" aria-label={copy.scope[scope].title} data-scope={scope}>
            <ul className="om-settings__list">{c.consequences[scope].map((t) => <li key={t}>{t}</li>)}</ul>
          </div>
        )}
        {started ? null : (
          <div className="om-settings__actions">
            <Button type="submit" variant="primary" disabled={scope === null || custodian === null}>{c.confirm}</Button>
            <Button variant="quiet" onClick={onBack}>{c.cancel}</Button>
          </div>
        )}
      </form>
      <OperationStatus state={op.state} copy={copy.outcome} done={c.done} />
    </section>
  );
}

function StopRecovery({ services, copy, request, onBack }: { readonly services: SettingsServices; readonly copy: RecoveryCopy; readonly request: RecoveryRequestView; readonly onBack: () => void }) {
  const c = copy.veto;
  const op = useOperation(services);
  const dir = request.direction;
  return (
    <section className="om-settings__step" aria-labelledby="om-recovery-stop-h">
      <Button variant="quiet" onClick={onBack}>{copy.back}</Button>
      <FocusHeading id="om-recovery-stop-h">{c.heading[dir]}</FocusHeading>
      <Text as="p">{copy.scope[request.scope].title} · {copy.started(request.startedAt)}</Text>
      <ul className="om-settings__list">{c.consequences[dir].map((t) => <li key={t}>{t}</li>)}</ul>
      {request.canStop && op.state.phase === "idle" ? (
        <div className="om-settings__actions">
          <Button variant="primary" onClick={() => { void op.run("veto-recovery", { requestId: request.requestId }); }}>{c.confirm[dir]}</Button>
          <Button onClick={onBack}>{c.cancel}</Button>
        </div>
      ) : <Text as="p" className="om-devices__state" data-state={request.state}>{copy.requestState[request.state]}</Text>}
      <OperationStatus state={op.state} copy={copy.outcome} done={c.done} />
    </section>
  );
}
