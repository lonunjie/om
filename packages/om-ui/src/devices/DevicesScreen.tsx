import { useState, type ReactNode } from "react";
import { isTerminalOutcome } from "@om/platform-api";
import type { DeviceId, DeviceSummary, DevicesView, OperationId, PendingOperationView } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import { SecurityNoticeBanner } from "../shell/theme/SecurityNoticeBanner";
import type { DeviceActionCopy, DevicePendingKind, DevicesCopy } from "./copy";
import { FocusHeading } from "./FocusHeading";
import { LinkDeviceFlow, LinkReview } from "./LinkDeviceFlow";
import { OperationStatus, outcomeText, useLiveQuery, useOperation, type SettingsServices } from "./operation";
import "./devices.css";

export interface DevicesScreenProps {
  readonly services: SettingsServices;
  readonly copy: DevicesCopy;
  /** QR scanning slot (camera opens only from the click). Without it, pasting the link is the only path. */
  readonly scanQr?: () => Promise<string | null>;
}

type View =
  | { readonly kind: "list" }
  | { readonly kind: "device"; readonly id: DeviceId }
  | { readonly kind: "link" }
  | { readonly kind: "review"; readonly operationId: OperationId };

const PENDING_KINDS: readonly DevicePendingKind[] = ["RequestDeviceLink", "ApproveDeviceLink", "RejectDeviceLink", "SuspendDevice", "RevokeDevice"];
const isPendingKind = (k: string): k is DevicePendingKind => (PENDING_KINDS as readonly string[]).includes(k);

/**
 * Settings > Devices. Every state, allowed action and outcome comes from Core. List rows only open details:
 * security-critical actions run from their dedicated step, never from a list row (deep-link rule).
 */
export function DevicesScreen({ services, copy, scanQr }: DevicesScreenProps) {
  const [view, setView] = useState<View>({ kind: "list" });
  const live = useLiveQuery(services, { kind: "GetDevices", ctx: services.ctx }, "devices");
  const toList = (): void => { setView({ kind: "list" }); };

  if (live.status === "loading") return <Text role="status">{copy.loading}</Text>;
  if (live.status === "failed") return <Text role="alert">{copy.loadFailed}</Text>;
  const { value, ops } = live.value;

  let body: ReactNode;
  switch (view.kind) {
    case "link":
      body = <LinkDeviceFlow services={services} copy={copy} onCancel={toList} onReview={(operationId) => { setView({ kind: "review", operationId }); }} {...(scanQr ? { scanQr } : {})} />;
      break;
    case "review":
      body = <LinkReview services={services} copy={copy} operationId={view.operationId} ops={ops} onDone={toList} />;
      break;
    case "device": {
      const device = value.devices.find((d) => d.id === view.id);
      body = device ? <DeviceDetails services={services} copy={copy} view={value} device={device} onBack={toList} /> : <DeviceList copy={copy} view={value} ops={ops} onOpen={setView} />;
      break;
    }
    case "list":
      body = <DeviceList copy={copy} view={value} ops={ops} onOpen={setView} />;
      break;
  }
  return (
    <section className="om-settings om-devices" aria-labelledby="om-devices-h">
      <Text as="h1" size="xl" weight="bold" id="om-devices-h">{copy.heading}</Text>
      <ControlBanners copy={copy} view={value} />
      {body}
    </section>
  );
}

/** Reserved security surface: themes cannot hide these and there is deliberately no dismiss control (R-166, UI-04). */
function ControlBanners({ copy, view }: { readonly copy: DevicesCopy; readonly view: DevicesView }) {
  return (
    <div className="om-devices__banners">
      {view.controlState === "normal" ? null : <SecurityNoticeBanner reasonCode={view.controlState} label={copy.controlState[view.controlState]} />}
      {view.notices.map((n) => <SecurityNoticeBanner key={`${n.deviceId}-${String(n.at)}`} reasonCode={n.code} label={copy.deviceLinkedNotice(n.label, n.at)} />)}
    </div>
  );
}

function DeviceList({ copy, view, ops, onOpen }: { readonly copy: DevicesCopy; readonly view: DevicesView; readonly ops: readonly PendingOperationView[]; readonly onOpen: (v: View) => void }) {
  const current = view.devices.filter((d) => d.isCurrent);
  const others = view.devices.filter((d) => !d.isCurrent && d.state !== "revoked");
  const removed = view.devices.filter((d) => !d.isCurrent && d.state === "revoked");
  const incoming = view.linkRequests.filter((r) => r.state === "awaiting_approval" || r.state === "pending_authorization");
  const pending = ops.filter((o) => isPendingKind(o.intentKind) && !isTerminalOutcome(o.outcome));
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
      <div className="om-settings__actions">
        <Button variant="primary" onClick={() => { onOpen({ kind: "link" }); }}>{copy.link.start}</Button>
      </div>
    </>
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
                <span className="om-settings__meta">{copy.role[d.role]} · {d.state === "active" ? copy.lastSeen[d.lastSeen] : copy.state[d.state]}</span>
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

type Confirming = "suspend" | "revoke" | null;

function DeviceDetails({ services, copy, view, device, onBack }: DeviceDetailsProps) {
  const [confirming, setConfirming] = useState<Confirming>(null);
  const [ran, setRan] = useState<"suspend" | "revoke" | null>(null);
  const op = useOperation(services);
  const frozen = view.controlState !== "normal";
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
      {device.cautions.length > 0 ? (
        <ul className="om-settings__list om-devices__cautions">
          {device.cautions.map((c) => <li key={c} data-caution={c}>{copy.cautions[c]}</li>)}
        </ul>
      ) : null}
      {actionCopy && confirming ? (
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
          {!op.locked && device.actions.includes("suspend") ? <Button onClick={() => { setConfirming("suspend"); }}>{copy.suspend.action}</Button> : null}
          {!op.locked && device.actions.includes("revoke") ? <Button data-danger="true" onClick={() => { setConfirming("revoke"); }}>{copy.revoke.action}</Button> : null}
          {device.actions.length === 0 && !device.isCurrent ? <Text as="p" tone="secondary">{frozen ? copy.frozenNoActions : copy.noActions}</Text> : null}
        </div>
      )}
      <OperationStatus state={op.state} copy={copy.outcome} done={ran === "revoke" ? copy.revoke.done : copy.suspend.done} />
    </section>
  );
}
