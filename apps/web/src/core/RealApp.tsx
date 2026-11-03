// The application composed over the REAL Core (om_wasm.wasm): profile vaults and their encrypted stores, onboarding,
// unlock, the connected shell and an identity switcher. The provider is injected: production passes none and the
// shell says so; the e2e build passes a clearly-named loopback. Nothing here knows what the provider is.
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { asProfileId, webInviteLink, type DeviceVault, type ProfileExecutionContext, type ProfileId } from "@om/platform-api";
import { type WasmCoreClient } from "@om/core-wasm";
import { browserVaultEnv, createEncryptedKv, createWebDeviceVault, createWebPlatform } from "@om/web-platform";
import {
  OnboardingFlow, OnboardingFrame, ProfileProvider, Text, ThemeProvider, UnlockVault, createActionRegistry, createMemoryKeyValue, createProfileScopedState,
  useActiveProfile, useCaptureContext, type ActionRegistry, type HomeSettingsServices, type OnboardingPlatform, type OnboardingResult,
} from "@om/ui";
import { InstallAffordance } from "../install/InstallAffordance";
import { clearExpectedNewChat, clearPendingInvite, expectNewChat, useExpectedNewChat, usePendingInvite } from "../invite/pendingInvite";
import { Landing } from "../landing/Landing";
import { appStrings as t, onboardingCopy, scannerCopy, unlockCopy } from "../copy/en";
import { QrCode } from "../qr/QrCode";
import { CHATS, navigate } from "../route/place";
import { useQrScanner } from "../qr/Scanner";
import { startCore } from "./loadCore";
import { unavailableProvider, type ConfiguredProvider } from "./provider";
import { createDraftStore, type DraftStore } from "./draftStore";
import { browserPollPace } from "./pollPace";
import { createProfileRegistry, type ProfileRegistry } from "./registry";
import { clearPendingSafetyCode, readPendingSafetyCode, savePendingSafetyCode } from "./safetyCode";
import { Shell, type DraftSync } from "./Shell";
import { createLockServices, type LockServices } from "./lockServices";
import { readIdleMinutes, writeIdleMinutes, type IdleMinutes } from "./lockPrefs";
import { useIdleLock } from "./useIdleLock";

export interface RealAppProps {
  /** `null`: no provider is configured; the app says so and queues everything locally. */
  readonly provider: ConfiguredProvider;
  /** Always-visible notice above the app (for example the e2e loopback warning). */
  readonly banner?: ReactNode;
  /** Mailbox polling interval; tests and e2e use a short fixed one. Left out, OM slows down while nobody is looking (`pollPace.ts`). */
  readonly pollMs?: number;
  /** Settings > Your Home, when the host can change its Home (the production entry). */
  readonly home?: HomeSettingsServices;
}

interface Ready {
  readonly rt: WasmCoreClient;
  readonly vault: DeviceVault;
  readonly registry: ProfileRegistry;
  readonly drafts: DraftStore;
  readonly platform: OnboardingPlatform;
  readonly known: readonly ProfileId[];
}

type Boot = { readonly k: "loading" } | { readonly k: "no_vault" } | { readonly k: "failed" } | ({ readonly k: "ready" } & Ready);

/** What a person sends: a web link on this app's own address (a phone camera or a chat app opens it), never Core's `om:` form. */
const shareLink = (coreLink: string): string => webInviteLink(coreLink, globalThis.location.origin);

const newProfileId = (): ProfileId => asProfileId("p-" + Array.from(globalThis.crypto.getRandomValues(new Uint8Array(8)), (b) => b.toString(16).padStart(2, "0")).join(""));

export function RealApp({ provider, banner, pollMs, home }: RealAppProps) {
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
        const drafts = createDraftStore((id) => createEncryptedKv(vault, { indexedDB: env.indexedDB }, id));
        rt = await startCore({ vault, indexedDB: env.indexedDB, provider: provider ?? unavailableProvider, pollMs: pollMs ?? browserPollPace() });
        const known = await registry.list();
        if (life.live) setBoot({ k: "ready", rt, vault, registry, drafts, platform: createWebPlatform(), known });
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
      <div className="om-app">
      {banner}
      {boot.k === "loading" ? <Starting /> : null}
      {boot.k === "no_vault" ? <Notice heading={t.vaultUnavailable.heading} body={t.vaultUnavailable.body} /> : null}
      {boot.k === "failed" ? <Notice heading={t.coreFailed.heading} body={t.coreFailed.body} /> : null}
      {boot.k === "ready" ? (
        <Profiles rt={boot.rt} vault={boot.vault} registry={boot.registry} drafts={boot.drafts} platform={boot.platform} known={boot.known} connected={provider !== null} {...(home === undefined ? {} : { home })} />
      ) : null}
      </div>
    </ThemeProvider>
  );
}

/** The wait while Core loads: the round OM mark, a ring that turns, and one calm line. */
function Starting() {
  return (
    <main className="om-starting" role="status" aria-live="polite">
      <span className="om-starting__mark" aria-hidden="true" translate="no">OM</span>
      <p className="om-starting__text">{t.coreLoading}</p>
      <p className="om-starting__text om-starting__note">{t.coreLoadingNote}</p>
    </main>
  );
}

function Notice({ heading, body }: { readonly heading: string; readonly body: string }) {
  return <main className="om-dev-fail"><Text as="h1">{heading}</Text><Text as="p">{body}</Text></main>;
}

interface ProfilesProps { readonly rt: WasmCoreClient; readonly vault: DeviceVault; readonly registry: ProfileRegistry; readonly drafts: DraftStore; readonly platform: OnboardingPlatform; readonly known: readonly ProfileId[]; readonly connected: boolean; readonly home?: HomeSettingsServices }

/** Which identities exist, which are unlocked, and which one is being created. */
function Profiles({ rt, vault, registry, drafts, platform, known: initial, connected, home }: ProfilesProps) {
  const [known, setKnown] = useState<readonly ProfileId[]>(initial);
  const [open, setOpen] = useState<ReadonlySet<ProfileId>>(() => new Set());
  const [pending, setPending] = useState<{ readonly id: ProfileId; readonly ctx: ProfileExecutionContext } | null>(null);
  const [error, setError] = useState(false);
  const store = useMemo(() => createProfileScopedState(createMemoryKeyValue()), []); // in memory only: nothing persisted
  const registryRef = useRef(registry);
  const [idle, setIdleState] = useState<IdleMinutes>(readIdleMinutes);
  const openRef = useRef(open);
  openRef.current = open;

  /** Locks every open identity that has something to unlock with (a password or a passkey). A key kept in this browser has nothing to ask for, so locking it would only flicker. */
  const lockOpen = useCallback(async (): Promise<void> => {
    const lockable: ProfileId[] = [];
    for (const id of openRef.current) {
      const w = await vault.wrapping(id);
      if (w.ok && (w.wrapping.passphraseProtected || w.wrapping.method === "webauthn-prf")) lockable.push(id);
    }
    if (lockable.length === 0) return;
    await drafts.flush(); // a locked vault cannot seal, so what was typed is written first and comes back on unlock
    for (const id of lockable) { rt.closeProfile(id); vault.lock(id); drafts.forget(id); }
    setOpen((o) => { const n = new Set(o); for (const id of lockable) n.delete(id); return n; });
  }, [rt, vault, drafts]);
  const setIdle = useCallback((minutes: IdleMinutes): void => { writeIdleMinutes(minutes); setIdleState(minutes); }, []);

  // Closing or hiding the tab can end the page without a lock: write drafts while the vault is still open.
  useEffect(() => {
    const save = (): void => { if (document.visibilityState === "hidden") void drafts.flush(); };
    document.addEventListener("visibilitychange", save);
    globalThis.addEventListener("pagehide", save);
    return () => { document.removeEventListener("visibilitychange", save); globalThis.removeEventListener("pagehide", save); };
  }, [drafts]);

  // Coming back to the app (or the network) is the moment to ask at once, whatever pace OM was keeping.
  useEffect(() => {
    const now = (): void => { if (document.visibilityState !== "hidden") for (const id of openRef.current) void rt.flush(id); };
    document.addEventListener("visibilitychange", now);
    globalThis.addEventListener("online", now);
    return () => { document.removeEventListener("visibilitychange", now); globalThis.removeEventListener("online", now); };
  }, [rt]);

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
  useIdleLock({ minutes: idle, enabled: pending === null && firstLocked === undefined && open.size > 0, onIdle: () => { void lockOpen(); } });
  if (error) return <Notice heading={t.coreFailed.heading} body={t.coreFailed.body} />;
  if (pending !== null) {
    return (
      <Onboard
        rt={rt} vault={vault} platform={platform} pending={pending} first={known.length === 0} onCancel={() => { setPending(null); navigate({ kind: "settings", page: "menu" }, { replace: true }); }}
        onDone={(result) => {
          if (result.safetyCodePending !== null) savePendingSafetyCode(pending.id, result.safetyCodePending);
          if (result.inviteAccepted) { clearPendingInvite(); expectNewChat([]); }
          navigate(CHATS, { replace: true }); // first run can start at any address; the app proper starts at the root
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
      <OnboardingFrame copy={onboardingCopy.frame} stepKey="unlock">
        <UnlockVault
          key={firstLocked} vault={vault} profileId={firstLocked} copy={unlockCopy} autoUnlock
          onUnlocked={() => { rt.openProfile(firstLocked).then(() => { setOpen((o) => new Set(o).add(firstLocked)); }, () => { setError(true); }); }}
        />
      </OnboardingFrame>
    );
  }
  if (known.length === 0) return <Starting />;
  return (
    <ProfileProvider client={rt} contexts={rt} store={store} {...(known[0] === undefined ? {} : { initialProfileId: known[0] })}>
      {connected ? null : <p role="status" className="om-notice-bar">{t.providerStatus.none}</p>}
      <Connected rt={rt} vault={vault} drafts={drafts} platform={platform} onAddProfile={begin} lockOpen={lockOpen} idle={idle} setIdle={setIdle} {...(home === undefined ? {} : { home })} />
    </ProfileProvider>
  );
}

interface ConnectedProps {
  readonly rt: WasmCoreClient; readonly vault: DeviceVault; readonly drafts: DraftStore; readonly platform: OnboardingPlatform; readonly onAddProfile: () => void; readonly home?: HomeSettingsServices;
  readonly lockOpen: () => Promise<void>; readonly idle: IdleMinutes; readonly setIdle: (minutes: IdleMinutes) => void;
}

function Connected({ rt, vault, drafts, platform, onAddProfile, home, lockOpen, idle, setIdle }: ConnectedProps) {
  const registry = useMemo(() => createActionRegistry(), []);
  const active = useActiveProfile();
  const capture = useCaptureContext();
  const activeId = active?.id ?? null;
  // One frozen context per selection: a new object each render would re-run every query effect.
  const ctx = useMemo<ProfileExecutionContext | null>(() => (activeId === null ? null : capture()), [activeId, capture]);
  const [resolved, setResolved] = useState(0);
  const token = usePendingInvite();
  const newChatAfter = useExpectedNewChat();
  const pending = useMemo(() => (activeId === null ? null : readPendingSafetyCode(activeId)), [activeId, resolved]);
  // A code Core no longer holds (already saved elsewhere, or the data was reset) is not worth offering.
  useEffect(() => {
    if (activeId === null || ctx === null || pending === null) return;
    let live = true;
    rt.query({ kind: "GetRecoveryKey", ctx, keyOperationId: pending }).then(
      (v) => { if (live && v === null) { clearPendingSafetyCode(activeId); setResolved((n) => n + 1); } },
      () => undefined,
    );
    return () => { live = false; };
  }, [rt, activeId, ctx, pending]);
  const lock = useMemo<LockServices | null>(
    () => (activeId === null ? null : createLockServices({ vault, profileId: activeId, passkeyUser: onboardingCopy.protect.passkeyUser, lockOpen, idleMinutes: () => idle, setIdleMinutes: setIdle })),
    [vault, activeId, lockOpen, idle, setIdle],
  );
  const draftSync = useMemo<DraftSync | null>(
    () => (activeId === null ? null : { load: () => drafts.load(activeId), save: (conversation, text) => { drafts.set(activeId, conversation, text); } }),
    [drafts, activeId],
  );
  if (activeId === null || ctx === null || lock === null || draftSync === null) return <Text>{t.loading}</Text>;
  const resolve = (): void => { clearPendingSafetyCode(activeId); setResolved((n) => n + 1); };
  return (
    <Shell
      key={activeId} services={{ client: rt, registry, ctx }} onAddProfile={onAddProfile} lock={lock} draftSync={draftSync} invite={{ platform, renderQr: (payload) => <QrCode payload={payload} />, shareLink }} {...(home === undefined ? {} : { home })}
      incoming={{ token, newChatAfter, onAccepted: (known) => { clearPendingInvite(); expectNewChat(known); }, onDeclined: clearPendingInvite, onNewChatOpened: clearExpectedNewChat }}
      {...(pending === null ? {} : { safetyCode: { keyOperationId: pending, platform, onResolved: resolve } })}
    />
  );
}

interface OnboardProps { readonly onCancel: () => void; readonly rt: WasmCoreClient; readonly vault: DeviceVault; readonly platform: OnboardingPlatform; readonly pending: { readonly id: ProfileId; readonly ctx: ProfileExecutionContext }; readonly first: boolean; readonly onDone: (result: OnboardingResult) => void }

function Onboard({ rt, vault, platform, pending, first, onDone, onCancel }: OnboardProps) {
  const registry = useMemo<ActionRegistry>(() => createActionRegistry(), []);
  const { scanQr, dialog } = useQrScanner(scannerCopy);
  const token = usePendingInvite();
  // The very first run opens with the landing page; adding a further identity, or opening an invite link, goes straight into setup.
  const [started, setStarted] = useState(!first || token !== null);
  if (!started) return <Landing onGetStarted={() => { setStarted(true); }} installSlot={<InstallAffordance />} />;
  return (
    <>
      <OnboardingFlow
        key={pending.id} client={rt} registry={registry} ctx={pending.ctx} vault={vault} platform={platform} copy={onboardingCopy}
        renderQr={(payload) => <QrCode payload={payload} />} scanQr={scanQr} shareLink={shareLink} incomingInvite={first ? token : null} onIncomingInviteDeclined={clearPendingInvite}
        skipWelcome onDone={onDone} {...(first ? {} : { addingIdentity: { backLabel: t.backToSettings, onBack: onCancel } })}
      />
      {dialog}
    </>
  );
}
