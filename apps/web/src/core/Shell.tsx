// The connected application shell shared by the production Core composition and the dev/e2e entries: conversation
// list, requests, conversation view, composer, Settings > Devices / Recovery. It only talks to a CoreClient.
import { useEffect, useRef, useState, type ReactNode } from "react";
import type { ConversationId, IntroductionRequest, OperationId } from "@om/platform-api";
import {
  AppShell, Button, Composer, ConnectedProfileSwitcher, ConversationList, ConversationView, DevicesScreen, HomeScreen, Icon, LiveRegion, MessageRequest, RecoveryScreen, StrangerPressure, Text, useLayoutMode,
  AcceptInvite, InviteFirst, RecoveryKeyStep, useConversationList, type ConversationServices, type HomeSettingsServices, type OnboardingPlatform,
} from "@om/ui";
import { appStrings as t, conversationCopy, onboardingCopy, deviceScannerCopy, devicesCopy, scannerCopy, homeCopy, recoveryCopy, shellLabels, switcherLabels } from "../copy/en";
import { useQrScanner } from "../qr/Scanner";
import { CopyNotice } from "../CopyNotice";
import { CHATS, placeTitle, placeUrl, usePlace, type SettingsPage } from "../route/place";
import "./shell.css";
import { InstallAffordance } from "../install/InstallAffordance";

/** The display-once safety code Core still holds: setup did not ask for it (ADR-0111), so the shell offers it on the user's time. */
export interface SafetyCodeOffer {
  readonly keyOperationId: OperationId;
  readonly platform: OnboardingPlatform;
  /** Called once Core has accepted the confirmation, or when the code is no longer available. */
  readonly onResolved: () => void;
}

export interface ShellProps {
  readonly services: ConversationServices;
  /** Extra content under the Devices screen (dev harness only). */
  readonly devicesExtra?: ReactNode;
  /** Offers "Add identity" next to the profile switcher when provided. */
  readonly onAddProfile?: () => void;
  /** Settings > Your Home, when the host can configure its Home (the production entry; not the dev harness). */
  readonly home?: HomeSettingsServices;
  readonly safetyCode?: SafetyCodeOffer;
  /** With it, an empty Chats list is the invite-first screen instead of a dead end (UI_UX_SYSTEM: first run and invitations). */
  readonly invite?: { readonly platform: OnboardingPlatform; readonly renderQr: (payload: string) => ReactNode; readonly shareLink?: (coreLink: string) => string };
  /** An invitation link that was opened here, and the chat it is expected to add. */
  readonly incoming?: {
    /** The opened link's token: the shell asks "Accept this invite?" until it is accepted or declined. */
    readonly token: string | null;
    /** The chats that existed when an invite was accepted: the first chat not among them is opened. Null: none expected. */
    readonly newChatAfter: readonly string[] | null;
    readonly onAccepted: (knownChats: readonly string[]) => void;
    readonly onDeclined: () => void;
    readonly onNewChatOpened: () => void;
  };
}

export function Shell({ services, devicesExtra, onAddProfile, home, safetyCode, invite, incoming }: ShellProps) {
  const inviteScan = useQrScanner(scannerCopy);
  const deviceScan = useQrScanner(deviceScannerCopy);
  const { place, raw, go, back } = usePlace();
  const mode = useLayoutMode();
  const [drafts, setDrafts] = useState<ReadonlyMap<ConversationId, string>>(() => new Map());
  const list = useConversationList(services);
  const chatsBeforeAccept = useRef<readonly string[]>([]);
  const selected = place.kind === "chats" ? (place.chat as ConversationId | null) : null;
  const settings: SettingsPage | null = place.kind === "settings" ? place.page : null;
  const savingCode = place.kind === "safety-code" && safetyCode !== undefined;
  const select = (id: ConversationId): void => { go({ kind: "chats", chat: id }); };
  const toChats = (): void => { back(CHATS); };

  // An accepted invite adds its chat at once (it says it is waiting for the other person): open it so the person sees what happened.
  const newChatAfter = incoming?.newChatAfter ?? null;
  const newChatOpened = incoming?.onNewChatOpened;
  useEffect(() => {
    if (newChatAfter === null || list.status !== "ready") return;
    const fresh = list.value.find((c) => !newChatAfter.includes(c.id));
    if (fresh === undefined) return;
    go({ kind: "chats", chat: fresh.id });
    newChatOpened?.();
  }, [newChatAfter, list, newChatOpened, go]);

  // An address that names nothing real (a chat that is not here, a step this host does not have) falls back to the root, not to an error.
  const chatGone = selected !== null && newChatAfter === null && list.status === "ready" && !list.value.some((c) => c.id === selected);
  const noSuchPlace = (place.kind === "safety-code" && safetyCode === undefined) || (place.kind === "settings" && place.page === "delivery" && home === undefined) || chatGone;
  useEffect(() => { if (noSuchPlace) go(CHATS, { replace: true }); }, [noSuchPlace, go]);
  // The address bar shows the place's own address ("/index.html" and "/nope" become "/"), so what is shown can be copied and reopened.
  useEffect(() => { if (!noSuchPlace && placeUrl(place) !== raw) go(place, { replace: true }); }, [raw, place, noSuchPlace, go]);

  // Each place has its own tab title (never a chat name), and a change of place moves focus to the page, not to a control that left.
  const placeKey = JSON.stringify(place);
  const first = useRef(true);
  useEffect(() => {
    document.title = placeTitle(place, { app: t.appName, chats: t.chats, settings: t.settingsHeading, safetyCode: t.safetyCode.action });
    if (first.current) { first.current = false; return; }
    document.getElementById("om-main")?.focus({ preventScroll: true });
  }, [placeKey]);

  const profileControl = (
    <>
      <ConnectedProfileSwitcher labels={switcherLabels} />
      {settings !== null && mode === "compact" ? (
        <Button variant="quiet" onClick={() => { back(CHATS); }}><Icon name="back" size={20} />{t.chats}</Button>
      ) : (
        <Button variant="quiet" aria-pressed={settings !== null} onClick={() => { if (settings === null) go({ kind: "settings", page: "menu" }); else back(CHATS); }}>
          <Icon name="settings" size={20} />{t.settings}
        </Button>
      )}
    </>
  );
  const common = { destinations: [{ id: "chats", label: t.chats }], activeDestination: "chats", onDestinationChange: () => { if (settings !== null) back(CHATS); }, labels: shellLabels, profileControl } as const;

  if (savingCode) {
    return (
      <AppShell
        {...common}
        conversationOpen
        list={<Text>{t.loading}</Text>}
        detail={(
          <div className="om-page">
            <BackBar label={t.backToChats} onBack={toChats} />
            <RecoveryKeyStep
              services={services} platform={safetyCode.platform} keyOperationId={safetyCode.keyOperationId} copy={onboardingCopy.recovery}
              onConfirmed={() => { toChats(); safetyCode.onResolved(); }}
            />
          </div>
        )}
      />
    );
  }
  if (settings !== null) {
    return (
      <>
        <AppShell
          {...common}
          conversationOpen={settings !== "menu"}
          list={<SettingsMenu page={settings} onOpen={(page) => { go({ kind: "settings", page }); }} onAddProfile={onAddProfile} hasHome={home !== undefined} />}
          detail={settings === "menu" ? <div className="om-page"><Text>{t.settingsHint}</Text></div> : (
            <div className="om-page">
              <BackBar label={t.backToSettings} onBack={() => { back({ kind: "settings", page: "menu" }); }} />
              {settings === "devices" ? (
                <>
                  <DevicesScreen services={services} copy={devicesCopy} scanQr={deviceScan.scanQr} />
                  {devicesExtra}
                </>
              ) : settings === "delivery" && home !== undefined ? <HomeScreen services={home} copy={homeCopy} /> : <RecoveryScreen services={services} copy={recoveryCopy} />}
            </div>
          )}
        />
        {deviceScan.dialog}
      </>
    );
  }
  const emptyInviteFirst = list.status === "ready" && list.value.length === 0 && invite !== undefined && incoming?.token == null;
  return (
    <>
      <AppShell
        {...common}
        conversationOpen={selected !== null}
        list={
          <>
            <header className="om-page-head"><Text as="h1" size="xl" weight="bold">{t.chats}</Text></header>
            {incoming?.token != null ? (
              <section className="om-page om-stack">
                <AcceptInvite
                  key={incoming.token} services={services} copy={onboardingCopy.incoming} token={incoming.token}
                  onSending={() => { chatsBeforeAccept.current = list.status === "ready" ? list.value.map((c) => c.id) : []; }}
                  onAccepted={() => { incoming.onAccepted(chatsBeforeAccept.current); }} onDecline={incoming.onDeclined}
                />
              </section>
            ) : null}
            <StrangerPressure services={services} copy={conversationCopy.pressure} />
            <Requests services={services} />
            {safetyCode === undefined ? null : (
              <section className="om-page om-stack" aria-label={t.safetyCode.action}>
                <div className="om-card">
                  <Text as="p">{t.safetyCode.reminder}</Text>
                  <Button onClick={() => { go({ kind: "safety-code" }); }}>{t.safetyCode.action}</Button>
                </div>
              </section>
            )}
            {list.status === "loading" ? <div className="om-page"><Text>{t.loading}</Text></div> : null}
            {list.status === "failed" ? <div className="om-page"><Text role="alert">{t.loadFailed}</Text></div> : null}
            {emptyInviteFirst ? (
              <div className="om-page">
                <InviteFirst
                  services={services} platform={invite.platform} copy={onboardingCopy.invite} renderQr={invite.renderQr} scanQr={inviteScan.scanQr} {...(invite.shareLink ? { shareLink: invite.shareLink } : {})}
                  onAccepted={() => undefined}
                />
              </div>
            ) : null}
            {list.status === "ready" && !emptyInviteFirst ? <ConversationList conversations={list.value} selectedId={selected} onSelect={select} copy={conversationCopy} /> : null}
          </>
        }
        detail={
          selected === null ? <div className="om-page"><Text tone="secondary">{t.selectConversation}</Text></div> : (
            <ConversationView key={selected} services={services} conversationId={selected} copy={conversationCopy} onBack={toChats} />
          )
        }
        composerZone={selected === null ? undefined : (
          <Composer
            key={selected} services={services} conversationId={selected} copy={conversationCopy} draft={drafts.get(selected) ?? ""}
            onDraftChange={(next) => { setDrafts((d) => new Map(d).set(selected, next)); }}
          />
        )}
      />
      {inviteScan.dialog}
      <LiveRegion>{selected === null ? "" : t.conversationOpened}</LiveRegion>
    </>
  );
}

/** Leaves a page that replaced the list (phones only: beside the list there is nothing to go back to). */
function BackBar({ label, onBack }: { readonly label: string; readonly onBack: () => void }) {
  return <div className="om-backbar"><Button variant="quiet" onClick={onBack}><Icon name="back" size={20} />{label}</Button></div>;
}

function Requests({ services }: { readonly services: ConversationServices }) {
  const [items, setItems] = useState<readonly IntroductionRequest[]>([]);
  const [gone, setGone] = useState<ReadonlySet<string>>(() => new Set());
  useEffect(() => {
    let live = true;
    // Live: a request can arrive while this list is on screen (the host polls the provider in the background).
    let unsubscribe: (() => void) | undefined;
    try { unsubscribe = services.client.subscribe({ kind: "GetIntroductions", ctx: services.ctx }, (r) => { if (live) setItems(r); }); } catch { /* stale context */ }
    services.client.query({ kind: "GetIntroductions", ctx: services.ctx }).then((r) => { if (live) setItems(r); }, () => { if (live) setItems([]); });
    return () => { live = false; unsubscribe?.(); };
  }, [services.client, services.ctx]);
  const shown = items.filter((r) => !gone.has(r.id));
  if (shown.length === 0) return null;
  const hide = (id: string): void => { setGone((g) => new Set(g).add(id)); };
  return (
    <section className="om-page om-stack" aria-label={t.requestsHeading}>
      {shown.map((r) => (
        <div key={r.id} className="om-card"><MessageRequest services={services} request={r} copy={conversationCopy} onIgnore={hide} /></div>
      ))}
    </section>
  );
}

/** Settings is reached from the profile area (Profile -> Settings), not a tab: Chats stays the compact root. */
function SettingsMenu({ page, onOpen, onAddProfile, hasHome }: { readonly page: SettingsPage; readonly onOpen: (p: SettingsPage) => void; readonly onAddProfile: (() => void) | undefined; readonly hasHome: boolean }) {
  const rows: readonly { readonly id: SettingsPage; readonly title: string; readonly hint: string }[] = [
    { id: "devices", title: t.settingsDevices, hint: t.settingsRows.devices },
    { id: "recovery", title: t.settingsRecovery, hint: t.settingsRows.recovery },
    ...(hasHome ? [{ id: "delivery" as const, title: t.settingsHome, hint: t.settingsRows.delivery }] : []),
  ];
  return (
    <nav aria-label={t.settingsHeading} className="om-menu">
      <header className="om-page-head"><Text as="h1" size="xl" weight="bold">{t.settingsHeading}</Text></header>
      <ul className="om-menu__list">
        {rows.map((r) => (
          <li key={r.id}>
            <button type="button" className="om-menu__row" aria-current={page === r.id ? "page" : undefined} onClick={() => { onOpen(r.id); }}>
              <span className="om-menu__text"><span className="om-menu__title">{r.title}</span><span className="om-menu__hint">{r.hint}</span></span>
              <Icon name="chevron" size={20} />
            </button>
          </li>
        ))}
        {onAddProfile ? (
          <li>
            <button type="button" className="om-menu__row" onClick={onAddProfile}>
              <span className="om-menu__text"><span className="om-menu__title">{t.addProfile}</span><span className="om-menu__hint">{t.settingsRows.addProfile}</span></span>
              <Icon name="plus" size={20} />
            </button>
          </li>
        ) : null}
      </ul>
      <div className="om-page om-stack">
        <InstallAffordance />
        <CopyNotice />
      </div>
    </nav>
  );
}
