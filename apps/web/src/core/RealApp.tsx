// The application composed over the REAL Core (om_wasm.wasm): profile vaults and their encrypted stores, onboarding,
// unlock, the connected shell and an identity switcher. The provider is injected: production passes none and the
// shell says so; the e2e build passes a clearly-named loopback. Nothing here knows what the provider is.
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { asProfileId, type DeviceVault, type ProfileExecutionContext, type ProfileId } from "@om/platform-api";
import { type WasmCoreClient } from "@om/core-wasm";
import { browserVaultEnv, createWebDeviceVault, createWebPlatform } from "@om/web-platform";
import {
  OnboardingFlow, ProfileProvider, Text, ThemeProvider, UnlockVault, createActionRegistry, createMemoryKeyValue, createProfileScopedState,
  useActiveProfile, useCaptureContext, type ActionRegistry, type OnboardingPlatform,
} from "@om/ui";
import { InstallAffordance } from "../install/InstallAffordance";
import { Landing } from "../landing/Landing";
import { appStrings as t, onboardingCopy, scannerCopy, unlockCopy } from "../copy/en";
import { QrCode } from "../qr/QrCode";
import { useQrScanner } from "../qr/Scanner";
import { startCore } from "./loadCore";
import { unavailableProvider, type ConfiguredProvider } from "./provider";
import { createProfileRegistry, type ProfileRegistry } from "./registry";
import { Shell } from "./Shell";

export interface RealAppProps {
  /** `null`: no provider is configured; the app says so and queues everything locally. */
  readonly provider: ConfiguredProvider;
  /** Always-visible notice above the app (for example the e2e loopback warning). */
  readonly banner?: ReactNode;
  /** Mailbox polling interval; tests and e2e use a short one. */
  readonly pollMs?: number;
}

interface Ready {
  readonly rt: WasmCoreClient;
  readonly vault: DeviceVault;
  readonly registry: ProfileRegistry;
  readonly platform: OnboardingPlatform;
  readonly known: readonly ProfileId[];
}

type Boot = { readonly k: "loading" } | { readonly k: "no_vault" } | { readonly k: "failed" } | ({ readonly k: "ready" } & Ready);

const newProfileId = (): ProfileId => asProfileId("p-" + Array.from(globalThis.crypto.getRandomValues(new Uint8Array(8)), (b) => b.toString(16).padStart(2, "0")).join(""));

export function RealApp({ provider, banner, pollMs = 3000 }: RealAppProps) {
  const [boot, setBoot] = useState<Boot>({ k: "loading" });
  useEffect(() => {
    const life = { live: true };
    let rt: WasmCoreClient | undefined;
    void (async () => {
      const env = browserVaultEnv("OM");
      if (env === undefined) { if (life.live) setBoot({ k: "no_vault" }); return; } // fail closed: no durable boundary, nothing starts
      try {
        const vault = createWebDeviceVault(env);
        const registry = createProfileRegistry(env.indexedDB);
        rt = await startCore({ vault, indexedDB: env.indexedDB, provider: provider ?? unavailableProvider, pollMs });
        const known = await registry.list();
        if (life.live) setBoot({ k: "ready", rt, vault, registry, platform: createWebPlatform(), known });
        else rt.dispose();
      } catch {
        rt?.dispose();
        if (life.live) setBoot({ k: "failed" });
      }
    })();
    return () => { life.live = false; rt?.dispose(); };
  }, [provider, pollMs]);

  return (
    <ThemeProvider>
      {banner}
      {boot.k === "loading" ? <Text>{t.coreLoading}</Text> : null}
      {boot.k === "no_vault" ? <Notice heading={t.vaultUnavailable.heading} body={t.vaultUnavailable.body} /> : null}
      {boot.k === "failed" ? <Notice heading={t.coreFailed.heading} body={t.coreFailed.body} /> : null}
      {boot.k === "ready" ? (
        <Profiles rt={boot.rt} vault={boot.vault} registry={boot.registry} platform={boot.platform} known={boot.known} connected={provider !== null} />
      ) : null}
    </ThemeProvider>
  );
}

function Notice({ heading, body }: { readonly heading: string; readonly body: string }) {
  return <main className="om-dev-fail"><Text as="h1">{heading}</Text><Text as="p">{body}</Text></main>;
}

interface ProfilesProps { readonly rt: WasmCoreClient; readonly vault: DeviceVault; readonly registry: ProfileRegistry; readonly platform: OnboardingPlatform; readonly known: readonly ProfileId[]; readonly connected: boolean }

/** Which identities exist, which are unlocked, and which one is being created. */
function Profiles({ rt, vault, registry, platform, known: initial, connected }: ProfilesProps) {
  const [known, setKnown] = useState<readonly ProfileId[]>(initial);
  const [open, setOpen] = useState<ReadonlySet<ProfileId>>(() => new Set());
  const [pending, setPending] = useState<{ readonly id: ProfileId; readonly ctx: ProfileExecutionContext } | null>(null);
  const [error, setError] = useState(false);
  const store = useMemo(() => createProfileScopedState(createMemoryKeyValue()), []); // in memory only: nothing persisted
  const registryRef = useRef(registry);

  const begin = useCallback((): void => {
    const id = newProfileId();
    rt.openProfile(id).then((ctx) => { setPending({ id, ctx }); }, () => { setError(true); });
  }, [rt]);

  // First run: an identity is created straight away. Later runs unlock the known identities, one at a time.
  const started = useRef(false);
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    if (initial.length === 0) begin();
  }, [initial, begin]);

  const firstLocked = known.find((p) => !open.has(p));
  if (error) return <Notice heading={t.coreFailed.heading} body={t.coreFailed.body} />;
  if (pending !== null) {
    return (
      <Onboard
        rt={rt} vault={vault} platform={platform} pending={pending} first={known.length === 0}
        onDone={() => {
          registryRef.current.add(pending.id).then(() => {
            setKnown((k) => [...k, pending.id]);
            setOpen((o) => new Set(o).add(pending.id));
            setPending(null);
          }, () => { setError(true); });
        }}
      />
    );
  }
  if (firstLocked !== undefined) {
    return (
      <main>
        <UnlockVault
          key={firstLocked} vault={vault} profileId={firstLocked} copy={unlockCopy}
          onUnlocked={() => { rt.openProfile(firstLocked).then(() => { setOpen((o) => new Set(o).add(firstLocked)); }, () => { setError(true); }); }}
        />
      </main>
    );
  }
  if (known.length === 0) return <Text>{t.coreLoading}</Text>;
  return (
    <ProfileProvider client={rt} contexts={rt} store={store} {...(known[0] === undefined ? {} : { initialProfileId: known[0] })}>
      {connected ? null : <p role="status" className="om-dev-banner">{t.providerStatus.none}</p>}
      <Connected rt={rt} onAddProfile={begin} />
    </ProfileProvider>
  );
}

function Connected({ rt, onAddProfile }: { readonly rt: WasmCoreClient; readonly onAddProfile: () => void }) {
  const registry = useMemo(() => createActionRegistry(), []);
  const active = useActiveProfile();
  const capture = useCaptureContext();
  const activeId = active?.id ?? null;
  // One frozen context per selection: a new object each render would re-run every query effect.
  const ctx = useMemo<ProfileExecutionContext | null>(() => (activeId === null ? null : capture()), [activeId, capture]);
  if (activeId === null || ctx === null) return <Text>{t.loading}</Text>;
  return <Shell key={activeId} services={{ client: rt, registry, ctx }} onAddProfile={onAddProfile} />;
}

interface OnboardProps { readonly rt: WasmCoreClient; readonly vault: DeviceVault; readonly platform: OnboardingPlatform; readonly pending: { readonly id: ProfileId; readonly ctx: ProfileExecutionContext }; readonly first: boolean; readonly onDone: () => void }

function Onboard({ rt, vault, platform, pending, first, onDone }: OnboardProps) {
  const registry = useMemo<ActionRegistry>(() => createActionRegistry(), []);
  const { scanQr, dialog } = useQrScanner(scannerCopy);
  const [started, setStarted] = useState(!first);
  // The very first run opens with the landing page; adding a further identity goes straight into setup.
  if (!started) return <Landing onGetStarted={() => { setStarted(true); }} installSlot={<InstallAffordance />} />;
  return (
    <>
      <OnboardingFlow
        key={pending.id} client={rt} registry={registry} ctx={pending.ctx} vault={vault} platform={platform} copy={onboardingCopy}
        renderQr={(payload) => <QrCode payload={payload} />} scanQr={scanQr} skipWelcome onDone={onDone}
      />
      {dialog}
    </>
  );
}
