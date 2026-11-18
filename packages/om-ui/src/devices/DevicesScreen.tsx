import { useState, type ReactNode } from "react";
import { isTerminalOutcome } from "@om/platform-api";
import type { ControllerChangeView, DeviceId, DeviceSummary, DevicesView, OperationId, PendingOperationView } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import { SecurityNoticeBanner } from "../shell/theme/SecurityNoticeBanner";
import type { DeviceActionCopy, DevicePendingKind, DevicesCopy } from "./copy";
import { FocusHeading } from "./FocusHeading";
import { AddDevice } from "./AddDevice";
import { LinkReview } from "./LinkReview";
import { OperationStatus, outcomeText, useLiveQuery, useOperation, type SettingsServices } from "./operation";
import "./devices.css";

export interface DevicesScreenProps {
  readonly services: SettingsServices;
  readonly copy: DevicesCopy;
  /** Draws the QR code for the link that adds a device (deviation D24: this device shows it, the new device scans or types it). */
  readonly renderQr?: (payload: string) => ReactNode;
  readonly scanQr?: () => Promise<string | null>;
  readonly recoveryPaused?: { readonly action: string; readonly pausedBody: string; readonly open: () => void };
  /** Whether this screen shows the enrollment notices itself. Off when the host shows them on every screen (`DeviceNotices`). Default true. */
  readonly showNotices?: boolean;
  /** "later" while Core cannot link a device: never offer a flow that ends in "unsupported" (UX brief section 5). Default "available". */
  readonly linking?: "available" | "later";
}

type View =
  | { readonly kind: "list" }
  | { readonly kind: "device"; readonly id: DeviceId }
  | { readonly kind: "add" }
  | { readonly kind: "controller_join" }
  | { readonly kind: "review"; readonly operationId: OperationId };

const PENDING_KINDS: readonly DevicePendingKind[] = ["OfferDeviceLink", "ApproveDeviceLink", "RejectDeviceLink", "SuspendDevice", "RevokeDevice"];
const isPendingKind = (k: string): k is DevicePendingKind => (PENDING_KINDS as readonly string[]).includes(k);

/**
 * Settings > Devices. Every state, allowed action and outcome comes from Core. List rows only open details:
 * security-critical actions run from their dedicated step, never from a list row (deep-link rule).
 */
export function DevicesScreen({ services, copy, renderQr, scanQr, recoveryPaused, showNotices = true, linking = "available" }: DevicesScreenProps) {
  const [view, setView] = useState<View>({ kind: "list" });
  const live = useLiveQuery(services, { kind: "GetDevices", ctx: services.ctx }, "devices");
  // The copy of older chats to a newly linked device: only a timed-out, partial transfer is worth a line (counts only, never content).
  const history = useLiveQuery(services, { kind: "GetHistoryTransfer", ctx: services.ctx }, "history");
  const degraded = history.status === "ready" && history.value.value?.state === "degraded" ? history.value.value.role : null;
  const toList = (): void => { setView({ kind: "list" }); };

  if (live.status === "loading") return <Text role="status">{copy.loading}</Text>;
  if (live.status === "failed") return <Text role="alert">{copy.loadFailed}</Text>;
  const { value, ops } = live.value;

  let body: ReactNode;
  switch (view.kind) {
    case "controller_join":
      body = <JoinControllerChange services={services} copy={copy} onBack={toList} scanQr={scanQr} />;
      break;
    case "add":
      body = <AddDevice services={services} copy={copy} onClose={toList} {...(renderQr ? { renderQr } : {})} />;
      break;
    case "review":
      body = <LinkReview services={services} copy={copy} operationId={view.operationId} ops={ops} onDone={toList} />;
      break;
    case "device": {
      const device = value.devices.find((d) => d.id === view.id);
      body = device ? <DeviceDetails services={services} copy={copy} view={value} device={device} onBack={toList} /> : <DeviceList services={services} copy={copy} view={value} ops={ops} onOpen={setView} linking={linking} />;
      break;
    }
    case "list":
      body = <DeviceList services={services} copy={copy} view={value} ops={ops} onOpen={setView} linking={linking} />;
      break;
  }
  return (
    <section className="om-settings om-devices" aria-labelledby="om-devices-h">
      <Text as="h1" size="xl" weight="bold" id="om-devices-h">{copy.heading}</Text>
      <ControlBanners copy={copy} view={value} notices={showNotices} />
      {value.recoveryKeyReplacementNeeded && recoveryPaused ? <div className="om-settings__group">
        <Text as="p" role="status">{recoveryPaused.pausedBody}</Text>
        <Button onClick={recoveryPaused.open}>{recoveryPaused.action}</Button>
      </div> : null}
      {degraded === null ? null : <Text as="p" role="status" className="om-devices__history">{copy.historyDegraded[degraded]}</Text>}
      {value.controllerChange
        ? <ControllerExchange services={services} copy={copy} change={value.controllerChange} renderQr={renderQr} />
        : body}
    </section>
  );
}

/** Reserved security surface: themes cannot hide these and there is deliberately no dismiss control (R-166, UI-04). */
function ControlBanners({ copy, view, notices }: { readonly copy: DevicesCopy; readonly view: DevicesView; readonly notices: boolean }) {
  return (
    <div className="om-devices__banners">
      {view.controlState === "normal" ? null : <SecurityNoticeBanner reasonCode={view.controlState} label={copy.controlState[view.controlState]} />}
      {notices ? view.notices.map((n) => <SecurityNoticeBanner key={`${n.deviceId}-${String(n.at)}`} reasonCode={n.code} label={copy.deviceLinkedNotice(n.label, n.at)} />) : null}
    </div>
  );
}

function DeviceList({ services, copy, view, ops, onOpen, linking }: { readonly services: SettingsServices; readonly copy: DevicesCopy; readonly view: DevicesView; readonly ops: readonly PendingOperationView[]; readonly onOpen: (v: View) => void; readonly linking: "available" | "later" }) {
  const current = view.devices.filter((d) => d.isCurrent);
  const others = view.devices.filter((d) => !d.isCurrent && d.state !== "revoked");
  const removed = view.devices.filter((d) => !d.isCurrent && d.state === "revoked");
  const incoming = view.linkRequests.filter((r) => r.state === "awaiting_approval" || r.state === "pending_authorization");
  const pending = ops.filter((o) => isPendingKind(o.intentKind) && !isTerminalOutcome(o.outcome));
  // Core refuses an offer from a messaging-only or suspended device, so no flow is shown that would end in "unsupported" (UX brief section 5).
  const canAdd = current.some((d) => d.role === "control" && d.state === "active") && view.controlState === "normal";
  return (
    <>
      {incoming.length > 0 ? (
        <section className="om-settings__group" aria-labelledby="om-devices-req-h">
          <Text as="h2" size="lg" weight="bold" id="om-devices-req-h">{copy.linkRequestsHeading}</Text>
          <ul className="om-settings__rows">
            {incoming.map((r) => (
              <li key={r.requestId}>
                <Button className="om-settings__row" onClick={() => { onOpen({ kind: "review", operationId: r.operationId }); }}>
                  <span>{copy.linkRequestRow(r.deviceLabel)}</span>
                  <span className="om-settings__meta">{copy.link.requestState[r.state]}</span>
                </Button>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {pending.length > 0 ? (
        <section className="om-settings__group" aria-labelledby="om-devices-pending-h">
          <Text as="h2" size="lg" weight="bold" id="om-devices-pending-h">{copy.pendingHeading}</Text>
          <ul className="om-settings__list">
            {pending.map((o) => (
              <li key={o.operationId} data-outcome={o.outcome.kind}>
                {isPendingKind(o.intentKind) ? copy.pendingKind[o.intentKind] : null}: {outcomeText(o.outcome, copy.outcome, "")}
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      <DeviceGroup id="om-devices-current-h" heading={copy.currentHeading} devices={current} copy={copy} onOpen={onOpen} />
      <DeviceGroup id="om-devices-other-h" heading={copy.otherHeading} devices={others} copy={copy} onOpen={onOpen} empty={copy.noOtherDevices} />
      {removed.length > 0 ? <DeviceGroup id="om-devices-removed-h" heading={copy.removedHeading} devices={removed} copy={copy} onOpen={onOpen} /> : null}
      {view.canJoinControllerChange ? <div className="om-settings__actions">
        <Button onClick={() => { onOpen({ kind: "controller_join" }); }}>{copy.controller.join}</Button>
      </div> : null}
      {canAdd && linking !== "later" ? <StrictOption services={services} copy={copy} on={view.strictNotice} /> : null}
      {linking === "later" ? <Text as="p" tone="secondary">{copy.linkLater}</Text> : canAdd ? (
        <div className="om-settings__actions">
          <Button variant="primary" onClick={() => { onOpen({ kind: "add" }); }}>{copy.link.start}</Button>
        </div>
      ) : <Text as="p" tone="secondary">{copy.link.notControl}</Text>}
    </>
  );
}

/** ADR-0193 section 8: the strict reading, off by default. A device-local setting, never an account record. */
function StrictOption({ services, copy, on }: { readonly services: SettingsServices; readonly copy: DevicesCopy; readonly on: boolean }) {
  const op = useOperation(services);
  const [asking, setAsking] = useState(false);
  return (
    <div className="om-settings__group om-devices__strict">
      <label>
        <input type="checkbox" checked={on} disabled={op.locked} onChange={() => { if (on) setAsking(true); else void op.run("set-strict-device-notice", { enabled: true }); }} />
        {" "}{copy.notice.strictOption}
      </label>
      {on && asking ? (
        <div role="alertdialog" aria-label={copy.notice.strictOffAsk} className="om-devices__strict-ask">
          <Text as="p">{copy.notice.strictOffAsk}</Text>
          <Button variant="primary" disabled={op.locked} onClick={() => { setAsking(false); void op.run("set-strict-device-notice", { enabled: false }); }}>{copy.notice.strictOffConfirm}</Button>
          <Button onClick={() => { setAsking(false); }}>{copy.notice.strictOffCancel}</Button>
        </div>
      ) : null}
      <Text as="p" size="sm" tone="secondary">{copy.notice.strictHelp}</Text>
    </div>
  );
}

interface DeviceGroupProps {
  readonly id: string;
  readonly heading: string;
  readonly devices: readonly DeviceSummary[];
  readonly copy: DevicesCopy;
  readonly onOpen: (v: View) => void;
  readonly empty?: string;
}

function DeviceGroup({ id, heading, devices, copy, onOpen, empty }: DeviceGroupProps) {
  return (
    <section className="om-settings__group" aria-labelledby={id}>
      <Text as="h2" size="lg" weight="bold" id={id}>{heading}</Text>
      {devices.length === 0 && empty !== undefined ? <Text as="p" tone="secondary">{empty}</Text> : (
        <ul className="om-settings__rows">
          {devices.map((d) => (
            <li key={d.id}>
              <Button className="om-settings__row" data-state={d.state} onClick={() => { onOpen({ kind: "device", id: d.id }); }}>
                <bdi className="om-settings__name">{d.label}</bdi>
                <span className="om-settings__meta">{copy.role[d.role]} · {d.state === "active" ? copy.lastSeen[d.lastSeen] : copy.state[d.state]}{d.noticeState === "owed" ? <> · <strong data-notice="owed">{copy.notice.notTold}</strong></> : d.noticeState === "not_set_up" ? <> · <strong data-notice="not_set_up">{copy.notice.notSetUp}</strong></> : null}</span>
              </Button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

interface DeviceDetailsProps {
  readonly services: SettingsServices;
  readonly copy: DevicesCopy;
  readonly view: DevicesView;
  readonly device: DeviceSummary;
  readonly onBack: () => void;
}

type Confirming = "suspend" | "revoke" | "promote" | "revoke_with_approval" | null;

function DeviceDetails({ services, copy, view, device, onBack }: DeviceDetailsProps) {
  const [confirming, setConfirming] = useState<Confirming>(null);
  const [ran, setRan] = useState<"suspend" | "revoke" | null>(null);
  const op = useOperation(services);
  const frozen = view.controlState !== "normal";
  const controllerOp = useOperation(services);
  const actionCopy: DeviceActionCopy | null = confirming === "suspend" ? copy.suspend : confirming === "revoke" ? copy.revoke : null;

  const confirm = (which: "suspend" | "revoke"): void => {
    setRan(which);
    setConfirming(null);
    // Two distinct registry actions: a suspend confirmation can never authorize a permanent removal.
    void op.run(which === "suspend" ? "suspend-device" : "finalize-device-revocation", { deviceId: device.id });
  };

  return (
    <section className="om-settings__step" aria-labelledby="om-device-h">
      <Button variant="quiet" onClick={onBack}>{copy.back}</Button>
      <FocusHeading id="om-device-h">{copy.detailsHeading(device.label)}</FocusHeading>
      <ul className="om-settings__facts" data-state={device.state}>
        <li>{copy.role[device.role]}</li>
        {device.isCurrent ? <li>{copy.thisDevice}</li> : null}
        <li>{copy.state[device.state]}</li>
        <li>{copy.enrolled(device.enrolledAt)}</li>
        <li>{copy.lastSeen[device.lastSeen]}</li>
      </ul>
      {device.noticeState === "told" ? null : (
        <div className="om-devices__notice" role="status" data-notice={device.noticeState}>
          <Text as="p"><strong>{device.noticeState === "owed" ? copy.notice.notTold : device.noticeState === "not_set_up" ? copy.notice.notSetUp : null}</strong> {device.noticeState === "owed" ? copy.notice.notToldDetail(device.label) : device.noticeState === "not_set_up" ? copy.notice.notSetUpDetail(device.label) : copy.notice.held(device.label)}</Text>
          {device.lastAttemptAt === undefined ? null : <Text as="p" size="sm" tone="secondary">{copy.notice.lastTried(device.lastAttemptAt)}</Text>}
          {!op.locked && device.actions.includes("retry_notice") ? <Button onClick={() => { void op.run("retry-device-notice", { deviceId: device.id }); }}>{copy.notice.tryNow}</Button> : null}
        </div>
      )}
      {device.cautions.length > 0 ? (
        <ul className="om-settings__list om-devices__cautions">
          {device.cautions.map((c) => <li key={c} data-caution={c}>{copy.cautions[c]}</li>)}
        </ul>
      ) : null}
      {confirming === "promote" || confirming === "revoke_with_approval" ? (
        <div className="om-settings__confirm">
          <Text as="p">{confirming === "promote" ? copy.controller.promotionBody : copy.controller.removalBody}</Text>
          <div className="om-settings__actions">
            <Button variant="primary" disabled={controllerOp.locked} onClick={() => {
              void controllerOp.run("offer-controller-change", { deviceId: device.id, remove: confirming === "revoke_with_approval" });
            }}>{confirming === "promote" ? copy.controller.promote : copy.controller.remove}</Button>
            <Button variant="quiet" disabled={controllerOp.locked} onClick={() => { setConfirming(null); }}>{copy.back}</Button>
          </div>
          <OperationStatus state={controllerOp.state} copy={copy.outcome} done="" />
        </div>
      ) : actionCopy && confirming ? (
        <div className="om-settings__confirm" role="group" aria-labelledby="om-device-confirm-h" data-action={confirming}>
          <Text as="h3" size="md" weight="bold" id="om-device-confirm-h">{actionCopy.heading(device.label)}</Text>
          <ul className="om-settings__list">{actionCopy.consequences.map((c) => <li key={c}>{c}</li>)}</ul>
          <div className="om-settings__actions">
            <Button variant="primary" onClick={() => { confirm(confirming); }}>{actionCopy.confirm}</Button>
            <Button onClick={() => { setConfirming(null); }}>{actionCopy.cancel}</Button>
          </div>
        </div>
      ) : (
        <div className="om-settings__actions">
          {device.actions.includes("promote") ? <Button onClick={() => { setConfirming("promote"); }}>{copy.controller.promote}</Button> : null}
          {device.actions.includes("revoke_with_approval") ? <Button data-danger="true" onClick={() => { setConfirming("revoke_with_approval"); }}>{copy.controller.remove}</Button> : null}
          {!op.locked && device.actions.includes("suspend") ? <Button onClick={() => { setConfirming("suspend"); }}>{copy.suspend.action}</Button> : null}
          {!op.locked && device.actions.includes("revoke") ? <Button data-danger="true" onClick={() => { setConfirming("revoke"); }}>{copy.revoke.action}</Button> : null}
          {device.actions.length === 0 && !device.isCurrent ? <Text as="p" tone="secondary">{frozen ? copy.frozenNoActions : copy.noActions}</Text> : null}
        </div>
      )}
      <OperationStatus state={op.state} copy={copy.outcome} done={ran === "revoke" ? copy.revoke.done : copy.suspend.done} />
    </section>
  );
}

function JoinControllerChange({ services, copy, onBack, scanQr }: {
  readonly services: SettingsServices;
  readonly copy: DevicesCopy;
  readonly onBack: () => void;
  readonly scanQr: (() => Promise<string | null>) | undefined;
}) {
  const [link, setLink] = useState("");
  const op = useOperation(services);
  const c = copy.controller;
  return <section className="om-settings__step" aria-labelledby="om-controller-join">
    <Button variant="quiet" onClick={onBack}>{copy.back}</Button>
    <FocusHeading id="om-controller-join">{c.join}</FocusHeading>
    {scanQr ? <Button disabled={op.locked} onClick={() => {
      void scanQr().then((text) => { if (text !== null) setLink(text); });
    }}>{c.scan}</Button> : null}
    <form className="om-settings__form" onSubmit={(e) => {
      e.preventDefault();
      if (link.trim() && !op.locked) void op.run("join-controller-change", { link });
    }}>
      <label htmlFor="om-controller-link">{c.link}</label>
      <input id="om-controller-link" className="om-settings__input" value={link} onChange={(e) => { setLink(e.target.value); }} autoComplete="off" autoCapitalize="off" spellCheck={false} />
      <Button type="submit" disabled={!link.trim() || op.locked}>{c.open}</Button>
    </form>
    <OperationStatus state={op.state} copy={copy.outcome} done="" />
  </section>;
}

function ControllerExchange({ services, copy, change, renderQr }: {
  readonly services: SettingsServices;
  readonly copy: DevicesCopy;
  readonly change: ControllerChangeView;
  readonly renderQr: ((payload: string) => ReactNode) | undefined;
}) {
  const op = useOperation(services);
  const c = copy.controller;
  const params = { requestId: change.requestId };
  return <section className="om-settings__step" aria-labelledby="om-controller-heading">
    <FocusHeading id="om-controller-heading">{change.remove ? c.remove : c.promote}</FocusHeading>
    <Text as="p">{change.remove ? change.host ? c.removalBody : c.removalHere : c.promotionBody}</Text>
    {change.link ? <div className="om-devices__offer">
      <Text as="p">{c.linkHelp}</Text>
      {renderQr ? <figure className="om-devices__qr" aria-label={c.link}>{renderQr(change.link)}</figure> : null}
      <Text as="p" id="om-controller-link-label" weight="bold">{c.link}</Text>
      <code className="om-devices__link" aria-labelledby="om-controller-link-label" translate="no">{change.link}</code>
    </div> : null}
    {change.verificationCode ? <>
      <Text as="p">{c.compare}</Text>
      <Text as="p" size="xl" weight="bold">{change.verificationCode.replace(/^(...)(...)$/, "$1 $2")}</Text>
    </> : null}
    {change.waiting ? <Text as="p" role="status">{c.waiting}</Text>
      : <Button variant="primary" disabled={op.locked} onClick={() => { void op.run("confirm-controller-change", params); }}>{c.confirm}</Button>}
    {change.host || !change.waiting
      ? <Button variant="quiet" disabled={op.locked} onClick={() => { void op.run("cancel-controller-change", params); }}>{c.cancel}</Button>
      : null}
    <OperationStatus state={op.state} copy={copy.outcome} done="" />
  </section>;
}
