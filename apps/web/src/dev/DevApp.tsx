// Walking skeleton: onboarding, then the shell, composed against the dev-only Core harness. Copy comes from ../copy/en.
import { useMemo, useState } from "react";
import type { DeviceVault, ProfileExecutionContext, ProfileId } from "@om/platform-api";
import { asConversationId } from "@om/platform-api";
import { HARNESS_MARKER, type Harness } from "@om/core-harness";
import {
  Button, DeviceNotices, OnboardingFlow, Portability, ProfileProvider, Text, ThemeProvider, createActionRegistry, createMemoryKeyValue, createProfileScopedState, useActiveProfile, useCaptureContext,
  type ActionRegistry, type ExpressiveHost, type NearbyService, type NearbyView, type OnboardingPlatform, type PortabilityService,
} from "@om/ui";
import { BridgeMessage, BridgeRoom, type BridgeEntryView, type BridgeMappingView, type BridgeRoomView, type BridgeService } from "@om/ui";
import { InstallAffordance } from "../install/InstallAffordance";
import { Landing } from "../landing/Landing";
import { appStrings as t, bridgeCopy, devicesCopy, onboardingCopy, portabilityCopy, scannerCopy } from "../copy/en";
import { Shell } from "../core/Shell";
import type { GroupsSetup } from "../core/GroupsGate";
import { QrCode } from "../qr/QrCode";
import { useQrScanner } from "../qr/Scanner";
import { expressiveRichText } from "./expressiveFixture";
import "./dev.css";

export interface DevAppProps {
  readonly harness: Harness;
  readonly expressiveHost?: ExpressiveHost;
  readonly vault: DeviceVault;
  readonly platform: OnboardingPlatform;
  readonly pendingProfileId: ProfileId;
  /** Profiles that are already onboarded at start. */
  readonly onboarded: readonly ProfileId[];
}

export function DevApp({ harness, vault, platform, pendingProfileId, onboarded, expressiveHost }: DevAppProps) {
  const registry = useMemo(() => createActionRegistry(), []);
  const store = useMemo(() => createProfileScopedState(createMemoryKeyValue()), []); // in memory only: nothing persisted
  const bridgeMode = new URLSearchParams(window.location.search).get("bridges");
  const portability = useMemo(devPortability, []);
  return (
    <ThemeProvider>
      <div className="om-app">
      <div className="om-dev-banner" role="note">DEV HARNESS — NOT PRODUCTION ({HARNESS_MARKER})</div>
      {portability !== undefined ? <main className="om-card"><Portability service={portability} copy={portabilityCopy} /></main> : bridgeMode === null ? (
      <ProfileProvider client={harness} contexts={harness} store={store} initialProfileId={pendingProfileId}>
        <Gate harness={harness} registry={registry} vault={vault} platform={platform} initiallyOnboarded={onboarded} expressiveHost={expressiveHost} />
      </ProfileProvider>
      ) : <DevBridgePreview mode={bridgeMode} />}
      </div>
    </ThemeProvider>
  );
}

function DevBridgePreview({ mode }: { readonly mode: string }) {
  const principal = { id: "example-bridge-id", name: "Bridge X" };
  const service = useMemo<BridgeService>(() => {
    const mapping: BridgeMappingView = {
      id: "example-mapping",
      network: "Example Chat",
      scope: "Weekend plans",
      direction: "both",
      exportedClasses: ["message.create", "reaction.set"],
      externalClaim: "Example Chat advertises encryption for this conversation.",
    };
    const proposed: BridgeEntryView = {
      principal: { id: "example-bridge-id", name: "Bridge X" },
      membership: "proposed",
      mappings: [mapping],
      action: { availability: "ready", intent: { kind: "admit", target: "example-offer" } },
    };
    const member: BridgeEntryView = {
      ...proposed,
      membership: "member",
      mappings: [{ ...mapping, state: "active" }],
      action: { availability: "ready", intent: { kind: "remove", target: "example-membership" } },
    };
    let view: BridgeRoomView = { bridges: [mode === "proposed" ? proposed : member], outcome: null };
    return {
      state: () => {
        if (mode === "slow") return new Promise<BridgeRoomView>(() => { /* Keep the loading state visible for review. */ });
        if (mode === "failed") return Promise.reject(new Error("Bridge preview unavailable"));
        return Promise.resolve(view);
      },
      change: (_room, intent) => {
        const next: BridgeEntryView = intent.kind === "admit"
          ? member
          : { ...member, membership: "removed", action: null };
        view = { bridges: [next], outcome: { kind: "Committed" } };
        return Promise.resolve(view);
      },
    };
  }, [mode]);
  return (
    <main>
      <BridgeRoom conversationId={asConversationId("example-room")} service={service} copy={bridgeCopy} />
      {mode === "active" ? <BridgeMessage copy={bridgeCopy.message} view={{
        principal,
        network: "Example Chat",
        actor: "Alex",
        identityLink: { kind: "linked", accountName: "Alex's OM identity" },
        flags: ["reply_target_unavailable"],
      }} /> : null}
    </main>
  );
}

/** Dev gallery only: `?nearby=on|policy|device` shows the "same Wi-Fi" row with a fake service so the screen can be looked at. */
function devNearby(): NearbyService | undefined {
  const mode = new URLSearchParams(window.location.search).get("nearby");
  if (mode === null) return undefined;
  let view: NearbyView = { enabled: false, available: mode !== "policy" };
  return {
    platform: mode === "device" ? "unavailable" : "available",
    state: () => Promise.resolve(view),
    set: (_id, on) => { view = { ...view, enabled: view.available ? on : false }; return Promise.resolve(view); },
  };
}

interface GateProps { readonly harness: Harness; readonly expressiveHost: ExpressiveHost | undefined; readonly registry: ActionRegistry; readonly vault: DeviceVault; readonly platform: OnboardingPlatform; readonly initiallyOnboarded: readonly ProfileId[] }

function Gate({ harness, registry, vault, platform, initiallyOnboarded, expressiveHost }: GateProps) {
  const active = useActiveProfile();
  const capture = useCaptureContext();
  const [done, setDone] = useState<ReadonlySet<ProfileId>>(() => new Set(initiallyOnboarded));
  const activeId = active?.id ?? null;
  // One frozen context per selection: a new object each render would re-run every query effect.
  const ctx = useMemo<ProfileExecutionContext | null>(() => (activeId === null ? null : capture()), [activeId, capture]);
  const { scanQr, dialog } = useQrScanner(scannerCopy);
  const [started, setStarted] = useState(false);
  const nearbyOnce = useMemo(() => { const n = devNearby(); return n === undefined ? {} : { nearby: n }; }, []);
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
  return <><DeviceNotices services={{ client: harness, registry, ctx }} copy={devicesCopy} /><Shell key={activeId} services={{ client: harness, registry, ctx, ...nearbyOnce }} groupsSetup={devGroupsSetup(harness)} expressiveHost={expressiveHost} {...(expressiveHost === undefined ? {} : { richTextFor: expressiveRichText })} devicesExtra={<DevNewDevice harness={harness} />} /></>;
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

/** DEV ONLY: the harness has no module to fetch, so "getting groups ready" just marks the simulated module as loaded (the real app restarts onto it). One object per harness: the gate runs it once. */
const setups = new WeakMap<Harness, GroupsSetup>();
function devGroupsSetup(harness: Harness): GroupsSetup {
  let s = setups.get(harness);
  if (s === undefined) { s = { enable: () => { harness.groups.setModuleLoaded(true); return Promise.resolve({ ok: true as const }); } }; setups.set(harness, s); }
  return s;
}

/** Dev gallery only. The injected service uses sample data and never opens or writes a file. */
function devPortability(): PortabilityService | undefined {
  const mode = new URLSearchParams(window.location.search).get("portability");
  if (mode !== "ready" && mode !== "slow" && mode !== "refused" && mode !== "unavailable") return undefined;
  const pause = () => mode === "slow" ? new Promise<void>((resolve) => { setTimeout(resolve, 2500); }) : Promise.resolve();
  const history = [
    { id: "demo-chat", name: "Example chat", kind: "chat" },
    { id: "demo-space", name: "Book club", kind: "space" },
    { id: "demo-publication", name: "Community announcements", kind: "publication" },
  ] as const;
  return {
    state: () => Promise.resolve(mode === "unavailable" ? {
      formats: { archive: "policy_denied", readable: "unsupported" },
      import: "device_unavailable",
    } : {
      formats: {
        archive: { history, contents: {
          media: "available", contacts: "available", aliases: "available",
          provenance: "available", local_preferences: "available",
        } },
        readable: { history: [history[0]], contents: {
          media: "unsupported", contacts: "unsupported", aliases: "unsupported",
          provenance: "unsupported", local_preferences: "unsupported",
        } },
      },
      import: "available",
    }),
    exportHistory: async (intent) => {
      await pause();
      if (mode === "refused") return { status: "rejected", reason: "write_failed" };
      return { status: "saved", report: {
        source: "local_history",
        destination: intent.format,
        details: {
          preserved: [{ code: "history", count: 5 }],
          lossy: [{ code: "view_once", count: 1 }],
        },
      } };
    },
    chooseImportSource: () => Promise.resolve(mode === "refused" ? { status: "rejected", reason: "invalid_archive" } : {
      status: "selected",
      source: {
        handle: "demo-archive",
        name: "example-history.zip",
        modes: { read_only: "available", same_account: "wrong_account" },
      },
    }),
    importHistory: async ({ mode: requested }) => {
      await pause();
      if (requested === "same_account") return { status: "rejected", reason: "wrong_account" };
      return { status: "imported", mode: "read_only", report: {
        source: "archive",
        destination: "read_only",
        details: { preserved: [{ code: "history", count: 5 }], missingMedia: [{ code: "missing_media", count: 1 }] },
      } };
    },
  };
}
