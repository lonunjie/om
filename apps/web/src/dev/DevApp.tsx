// Walking skeleton: onboarding, then the shell, composed against the dev-only Core harness. Copy comes from ../copy/en.
import { useMemo, useState } from "react";
import type { DeviceVault, ProfileExecutionContext, ProfileId } from "@om/platform-api";
import { HARNESS_MARKER, type Harness } from "@om/core-harness";
import {
  Button, DeviceNotices, OnboardingFlow, ProfileProvider, Text, ThemeProvider, createActionRegistry, createMemoryKeyValue, createProfileScopedState, useActiveProfile, useCaptureContext,
  type ActionRegistry, type OnboardingPlatform,
} from "@om/ui";
import { InstallAffordance } from "../install/InstallAffordance";
import { Landing } from "../landing/Landing";
import { appStrings as t, devicesCopy, onboardingCopy, scannerCopy } from "../copy/en";
import { Shell } from "../core/Shell";
import { QrCode } from "../qr/QrCode";
import { useQrScanner } from "../qr/Scanner";
import "./dev.css";

export interface DevAppProps {
  readonly harness: Harness;
  readonly vault: DeviceVault;
  readonly platform: OnboardingPlatform;
  readonly pendingProfileId: ProfileId;
  /** Profiles that are already onboarded at start. */
  readonly onboarded: readonly ProfileId[];
}

export function DevApp({ harness, vault, platform, pendingProfileId, onboarded }: DevAppProps) {
  const registry = useMemo(() => createActionRegistry(), []);
  const store = useMemo(() => createProfileScopedState(createMemoryKeyValue()), []); // in memory only: nothing persisted
  return (
    <ThemeProvider>
      <div className="om-app">
      <div className="om-dev-banner" role="note">DEV HARNESS — NOT PRODUCTION ({HARNESS_MARKER})</div>
      <ProfileProvider client={harness} contexts={harness} store={store} initialProfileId={pendingProfileId}>
        <Gate harness={harness} registry={registry} vault={vault} platform={platform} initiallyOnboarded={onboarded} />
      </ProfileProvider>
      </div>
    </ThemeProvider>
  );
}

interface GateProps { readonly harness: Harness; readonly registry: ActionRegistry; readonly vault: DeviceVault; readonly platform: OnboardingPlatform; readonly initiallyOnboarded: readonly ProfileId[] }

function Gate({ harness, registry, vault, platform, initiallyOnboarded }: GateProps) {
  const active = useActiveProfile();
  const capture = useCaptureContext();
  const [done, setDone] = useState<ReadonlySet<ProfileId>>(() => new Set(initiallyOnboarded));
  const activeId = active?.id ?? null;
  // One frozen context per selection: a new object each render would re-run every query effect.
  const ctx = useMemo<ProfileExecutionContext | null>(() => (activeId === null ? null : capture()), [activeId, capture]);
  const { scanQr, dialog } = useQrScanner(scannerCopy);
  const [started, setStarted] = useState(false);
  if (activeId === null || ctx === null) return <Text>{t.loading}</Text>;
  if (!done.has(activeId)) {
    // No identity on this device yet: the landing page comes first and its call to action starts onboarding.
    if (!started) return <Landing onGetStarted={() => { setStarted(true); }} installSlot={<InstallAffordance />} />;
    return (
      <>
        <OnboardingFlow
          key={activeId} client={harness} registry={registry} ctx={ctx} vault={vault} platform={platform} copy={onboardingCopy}
          renderQr={(payload) => <QrCode payload={payload} />} scanQr={scanQr}
          skipWelcome onDone={() => { setDone((d) => new Set(d).add(activeId)); }}
        />
        {dialog}
      </>
    );
  }
  return <><DeviceNotices services={{ client: harness, registry, ctx }} copy={devicesCopy} /><Shell key={activeId} services={{ client: harness, registry, ctx }} devicesExtra={<DevNewDevice harness={harness} />} /></>;
}

/** DEV ONLY: stands in for the screen of a second, NEW device (deviation D24) so "Add a device" can be tried in one browser. */
function DevNewDevice({ harness }: { readonly harness: Harness }) {
  const [link, setLink] = useState("");
  const [said, setSaid] = useState("");
  return (
    <aside className="om-dev-newdevice" aria-label="Dev harness: simulated new device">
      <p className="om-dev-newdevice__tag">DEV HARNESS ONLY: simulated new device</p>
      <label>
        Link shown by Add a device
        <input type="text" value={link} onChange={(e) => { setLink(e.target.value); }} autoComplete="off" spellCheck={false} />
      </label>
      <Button onClick={() => { setSaid(harness.simulateNewDeviceRequest(link.trim(), "Simulated laptop") ? "Asked to join." : "No open offer for that link."); }}>Simulate a new device asking to join</Button>
      <p role="status">{said}</p>
    </aside>
  );
}
