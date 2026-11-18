import { useEffect, useState } from "react";
import { FocusHeading } from "../devices/FocusHeading";
import { useLiveQuery, type SettingsServices } from "../devices/operation";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { AlertsCopy, AlertsFailure } from "./copy";
import type { AlertsAvailability, AlertsServices, AlertsStatus } from "./services";
import "../devices/devices.css";

export interface AlertsScreenProps {
  /** Core's services: only the privacy level is read from them. */
  readonly services: SettingsServices;
  readonly alerts: AlertsServices;
  readonly copy: AlertsCopy;
}

/**
 * Settings > Alerts (ADR-0210). One switch for this device, off until the person turns it on. At the Maximum protection level it asks first and says
 * the cost; nothing registers before that. A level that cannot be read is treated as Maximum (the cautious reading).
 */
export function AlertsScreen({ services, alerts, copy }: AlertsScreenProps) {
  const live = useLiveQuery(services, { kind: "GetPrivacyLevel", ctx: services.ctx }, "privacy-level");
  const [why, setWhy] = useState<AlertsAvailability | null>(null);
  const [status, setStatus] = useState<AlertsStatus | null>(null);
  const [broken, setBroken] = useState(false);
  const [ask, setAsk] = useState(false);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<AlertsFailure | null>(null);
  const [said, setSaid] = useState<"on" | "off" | null>(null);

  useEffect(() => {
    let live = true;
    void Promise.all([alerts.availability(), alerts.status()]).then(([a, s]) => { if (live) { setWhy(a); setStatus(s); } }, () => { if (live) setBroken(true); });
    return () => { live = false; };
  }, [alerts]);

  if (broken || live.status === "failed") return <Text role="alert">{copy.loadFailed}</Text>;
  if (why === null || status === null || live.status === "loading") return <Text role="status">{copy.loading}</Text>;
  const maximum = live.value.value === "maximum" || live.value.value === "custom" ? live.value.value === "maximum" : false;

  const turnOn = async (confirmedMaximum: boolean): Promise<void> => {
    setBusy(true); setFailure(null); setSaid(null); setAsk(false);
    const r = await alerts.enable({ confirmedMaximum }).catch((): "failed" => "failed");
    const s = await alerts.status().catch(() => status);
    setStatus(s); setBusy(false);
    if (r === "enabled") setSaid("on"); else setFailure(r);
  };
  const turnOff = async (): Promise<void> => {
    setBusy(true); setFailure(null);
    await alerts.disable().catch(() => undefined);
    setStatus(await alerts.status().catch(() => ({ on: false, permission: status.permission, maxOk: false })));
    setBusy(false); setSaid("off");
  };
  const paused = status.on && maximum && !status.maxOk;

  return (
    <section className="om-settings om-alerts" aria-labelledby="om-alerts-h">
      <Text as="h1" size="xl" weight="bold" id="om-alerts-h">{copy.heading}</Text>
      <Text as="p">{copy.intro}</Text>
      <Text as="p" size="sm" tone="secondary">{copy.privacy}</Text>
      <Text as="p" size="sm" tone="secondary">{copy.ios}</Text>
      <Text as="p" size="sm" tone="secondary">{copy.limit}</Text>
      {why !== "ready" ? <Text as="p" role="status">{copy.unavailable[why]}</Text> : ask ? (
        <div role="group" aria-labelledby="om-alerts-ask-h" className="om-settings__confirm">
          <FocusHeading id="om-alerts-ask-h">{copy.maximum.title}</FocusHeading>
          <Text as="p">{copy.maximum.body}</Text>
          <Button disabled={busy} onClick={() => { void turnOn(true); }}>{copy.maximum.confirm}</Button>
          <Button variant="quiet" disabled={busy} onClick={() => { setAsk(false); }}>{copy.maximum.keepOff}</Button>
        </div>
      ) : (
        <div>
          <Text as="p" weight="bold">{status.on ? copy.on : copy.off}</Text>
          {paused ? <Text as="p">{copy.paused.body}</Text> : null}
          {paused ? <Button disabled={busy} onClick={() => { void turnOn(true); }}>{copy.paused.keep}</Button> : null}
          {status.on ? <Button variant="secondary" disabled={busy} onClick={() => { void turnOff(); }}>{copy.turnOff}</Button>
            : <Button disabled={busy} onClick={() => { if (maximum) setAsk(true); else void turnOn(false); }}>{copy.turnOn}</Button>}
        </div>
      )}
      {said === null ? null : <Text role="status">{said === "on" ? copy.turnedOn : copy.turnedOff}</Text>}
      {failure === null ? null : <Text role="alert">{copy.failure[failure]}</Text>}
    </section>
  );
}

/** At start: renews the Home's registration when alerts are on (it lapses after 30 days). Waits for the level; an unreadable level counts as Maximum. */
export function useAlertsRefresh(services: SettingsServices, alerts: AlertsServices | undefined): void {
  const live = useLiveQuery(services, { kind: "GetPrivacyLevel", ctx: services.ctx }, "privacy-level");
  const level = live.status === "ready" ? live.value.value : live.status === "failed" ? "maximum" : undefined;
  useEffect(() => {
    if (alerts === undefined || level === undefined) return;
    void alerts.refresh({ maximum: level === "maximum" }).catch(() => undefined);
  }, [alerts, level]);
}
