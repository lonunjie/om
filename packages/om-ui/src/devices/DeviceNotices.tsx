import { SecurityNoticeBanner } from "../shell/theme/SecurityNoticeBanner";
import type { DevicesCopy } from "./copy";
import { useLiveQuery, type SettingsServices } from "./operation";
import "./devices.css";

/**
 * The enrollment notice (R-166, ACCOUNT_CONTINUITY_AND_CONTROL 4.1): when another device joins this account, every
 * other device says so, on a reserved surface that has no dismiss control and that themes cannot restyle (UI-04). It
 * is a strip of its own so it shows on every screen, not only in Settings > Devices. Core owns the list; nothing here hides, ages out or acknowledges one.
 */
export function DeviceNotices({ services, copy }: { readonly services: SettingsServices; readonly copy: DevicesCopy }) {
  const live = useLiveQuery(services, { kind: "GetDevices", ctx: services.ctx }, "device-notices");
  if (live.status !== "ready" || live.value.value.notices.length === 0) return null;
  return (
    <div className="om-devices__banners om-devices__notices" aria-label={copy.noticesLabel} role="group">
      {live.value.value.notices.map((n) => <SecurityNoticeBanner key={`${n.deviceId}-${String(n.at)}`} reasonCode={n.code} label={copy.deviceLinkedNotice(n.label, n.at)} />)}
    </div>
  );
}
