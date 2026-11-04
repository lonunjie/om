// The connected application shell shared by the production Core composition and the dev/e2e entries: conversation
// list, requests, conversation view, composer, Settings > Devices / Recovery. It only talks to a CoreClient.
import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import type { ConversationId, IntroductionRequest, OperationId } from "@om/platform-api";
import {
  AppShell, Avatar, Button, Composer, ConnectedProfileSwitcher, ConversationList, ConversationView, DevicesScreen, HomeScreen, Icon, IconButton, LiveRegion, MessageRequest, RecoveryScreen, StrangerPressure, Text, useActiveProfile, useLayoutMode,
  type IconName,
  AcceptInvite, InviteFirst, RecoveryKeyStep, useConversationList, type ConversationServices, type HomeSettingsServices, type OnboardingPlatform,
} from "@om/ui";
import { appStrings as t, conversationCopy, onboardingCopy, deviceScannerCopy, devicesCopy, scannerCopy, homeCopy, recoveryCopy, shellLabels, switcherLabels } from "../copy/en";
import { useQrScanner } from "../qr/Scanner";
import { CopyNotice } from "../CopyNotice";
import { CHATS, placeTitle, placeUrl, usePlace, type SettingsPage } from "../route/place";
import "./shell.css";
import { InstallAffordance } from "../install/InstallAffordance";
import { HelpScreen } from "./HelpScreen";
import { BackupScreen } from "./BackupScreen";
import type { BackupServices } from "./backupServices";
import { LockScreen } from "./LockScreen";
import type { LockServices } from "./lockServices";

/** The display-once safety code Core still holds: setup did not ask for it (ADR-0111), so the shell offers it on the user's time. */
export interface SafetyCodeOffer {
  readonly keyOperationId: OperationId;
  readonly platform: OnboardingPlatform;
  /** Called once Core has accepted the confirmation, or when the code is no longer available. */
  readonly onResolved: () => void;
}

/** Keeps unsent drafts on this device (encrypted), so locking or reloading does not lose them. */
export interface DraftSync {
  readonly load: () => Promise<ReadonlyMap<ConversationId, string>>;
  readonly save: (conversationId: ConversationId, text: string) => void;
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
  /** Settings > Lock, when the host can read how this device opens OM (the production entry). */
  readonly lock?: LockServices;
  /** Settings > Backup file, when the host can save and restore one (the production entry). */
  readonly backup?: BackupServices;
  /** Where drafts are kept between visits (the production entry); without it they last until the page closes. */
  readonly draftSync?: DraftSync;
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

export function Shell({ services, devicesExtra, onAddProfile, home, lock, backup, draftSync, safetyCode, invite, incoming }: ShellProps) {
  const inviteScan = useQrScanner(scannerCopy);
  const deviceScan = useQrScanner(deviceScannerCopy);
  const { place, raw, go, back } = usePlace();
  const me = useActiveProfile();
  const mode = useLayoutMode();
  const [drafts, setDrafts] = useState<ReadonlyMap<ConversationId, string>>(() => new Map());
  // Drafts saved on an earlier visit (or before a lock) come back; anything typed since this screen opened wins.
  useEffect(() => {
    if (draftSync === undefined) return;
    let live = true;
    draftSync.load().then((saved) => { if (live && saved.size > 0) setDrafts((now) => new Map([...saved, ...now])); }, () => undefined);
    return () => { live = false; };
  }, [draftSync]);
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
  const noSuchPlace = (place.kind === "safety-code" && safetyCode === undefined) || (place.kind === "settings" && place.page === "delivery" && home === undefined) || (place.kind === "settings" && place.page === "lock" && lock === undefined) || (place.kind === "settings" && place.page === "backup" && backup === undefined) || chatGone;
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

  // The person, not the word "Settings": their name and initial open Settings. A dot says something there needs them (the safety code).
  const name = me?.label ?? t.settings;
  const attention = safetyCode !== undefined;
  // Chats is a place, not "back": going there must not walk the history through the settings pages that were open.
  const toChatsRoot = (): void => { go(CHATS); };
  const profileControl = (
    <>
      <ConnectedProfileSwitcher labels={switcherLabels} />
      {settings !== null && mode === "compact" ? (
        <Button variant="quiet" onClick={toChatsRoot}><Icon name="chat" size={20} />{t.chats}</Button>
      ) : (
        <Button
          variant="quiet" className="om-me" aria-pressed={settings !== null}
          aria-label={`${name}, ${t.settings}${attention ? `, ${t.settingsNeedsAttention}` : ""}`}
          onClick={() => { if (settings === null) go({ kind: "settings", page: "menu" }); else toChatsRoot(); }}
        >
          <span className="om-me__avatar"><Avatar name={name} /></span>
          <span className="om-me__name">{name}</span>
        </Button>
      )}
    </>
  );
  const common = {
    destinations: [{ id: "chats", label: t.chats, icon: <Icon name="chat" size={20} /> }], activeDestination: settings === null && !savingCode ? "chats" : "",
    onDestinationChange: () => { if (place.kind !== "chats" || place.chat !== null) toChatsRoot(); }, labels: shellLabels, profileControl,
  } as const;

  if (savingCode) {
    return (
      <AppShell
        {...common}
        conversationOpen
        list={<Text>{t.loading}</Text>}
        detail={(
          <div className="om-page">
            <SubPage label={t.backToChats} onBack={toChats}>
              <RecoveryKeyStep
                services={services} platform={safetyCode.platform} keyOperationId={safetyCode.keyOperationId} copy={onboardingCopy.recovery}
                onConfirmed={() => { toChats(); safetyCode.onResolved(); }}
              />
            </SubPage>
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
          list={(
            <SettingsMenu
              page={settings} onOpen={(page) => { go({ kind: "settings", page }); }} onAddProfile={onAddProfile} hasHome={home !== undefined} hasLock={lock !== undefined} hasBackup={backup !== undefined}
              onSafetyCode={safetyCode === undefined ? undefined : () => { go({ kind: "safety-code" }); }}
            />
          )}
          detail={settings === "menu" ? <div className="om-page om-page--empty"><span className="om-empty-art" aria-hidden="true"><Icon name="settings" size={72} /></span></div> : (
            <div className="om-page">
              <SubPage label={t.backToSettings} onBack={() => { back({ kind: "settings", page: "menu" }); }}>
                {settings === "devices" ? (
                  <>
                    <DevicesScreen services={services} copy={devicesCopy} scanQr={deviceScan.scanQr} linking="later" />
                    {devicesExtra}
                  </>
                ) : settings === "delivery" && home !== undefined ? <HomeScreen services={home} copy={homeCopy} />
                  : settings === "lock" && lock !== undefined ? <LockScreen services={lock} />
                  : settings === "backup" && backup !== undefined ? <BackupScreen services={backup} />
                  : settings === "help" ? <HelpScreen />
                  : <RecoveryScreen services={services} copy={recoveryCopy} />}
              </SubPage>
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
          selected === null ? <div className="om-page om-page--empty"><span className="om-empty-art" aria-hidden="true"><Icon name="chat" size={72} /></span></div> : (
            <ConversationView key={selected} services={services} conversationId={selected} copy={conversationCopy} onBack={toChats} />
          )
        }
        composerZone={selected === null || (list.status === "ready" && list.value.some((c) => c.id === selected && c.relationship === "restored")) ? undefined : (
          <Composer
            key={selected} services={services} conversationId={selected} copy={conversationCopy} draft={drafts.get(selected) ?? ""}
            onDraftChange={(next) => { setDrafts((d) => new Map(d).set(selected, next)); draftSync?.save(selected, next); }}
          />
        )}
      />
      {inviteScan.dialog}
      <LiveRegion>{selected === null ? "" : t.conversationOpened}</LiveRegion>
    </>
  );
}

/**
 * A page opened from a menu: a round arrow beside the heading (like X's settings), no words. Phones only: beside the list (900 px and
 * up) there is nothing to go back to. The arrow carries its name for screen readers and a circle highlight for sighted users.
 */
function SubPage({ label, onBack, children }: { readonly label: string; readonly onBack: () => void; readonly children: ReactNode }) {
  return (
    <div className="om-subpage">
      <IconButton className="om-subpage__back" label={label} icon={<Icon name="back" />} onClick={onBack} />
      <div className="om-subpage__body">{children}</div>
    </div>
  );
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

interface MenuRow { readonly id: SettingsPage; readonly title: string; readonly hint: string; readonly icon: IconName; readonly words: string }

/** Settings is reached from the profile area (Profile -> Settings), not a tab: Chats stays the compact root. Typing anywhere searches it. */
function SettingsMenu({ page, onOpen, onAddProfile, hasHome, hasLock, hasBackup, onSafetyCode }: {
  readonly page: SettingsPage; readonly onOpen: (p: SettingsPage) => void; readonly onAddProfile: (() => void) | undefined; readonly hasHome: boolean; readonly hasLock: boolean; readonly hasBackup: boolean; readonly onSafetyCode: (() => void) | undefined;
}) {
  const [query, setQuery] = useState("");
  const search = useRef<HTMLInputElement>(null);
  const rows: readonly MenuRow[] = [
    ...(hasLock ? [{ id: "lock" as const, title: t.settingsLock, hint: t.settingsRows.lock, icon: "lock" as const, words: t.settingsKeywords.lock }] : []),
    { id: "devices", title: t.settingsDevices, hint: t.settingsRows.devices, icon: "devices", words: t.settingsKeywords.devices },
    { id: "recovery", title: t.settingsRecovery, hint: t.settingsRows.recovery, icon: "shield", words: t.settingsKeywords.recovery },
    ...(hasBackup ? [{ id: "backup" as const, title: t.settingsBackup, hint: t.settingsRows.backup, icon: "download" as const, words: t.settingsKeywords.backup }] : []),
    ...(hasHome ? [{ id: "delivery" as const, title: t.settingsHome, hint: t.settingsRows.delivery, icon: "delivery" as const, words: t.settingsKeywords.delivery }] : []),
    { id: "help", title: t.settingsHelp, hint: t.settingsRows.help, icon: "chat", words: t.settingsKeywords.help },
  ];
  const terms = query.toLowerCase().split(/\s+/).filter((w) => w !== "");
  const matches = (text: string): boolean => terms.every((w) => text.toLowerCase().includes(w));
  const shown = rows.filter((r) => matches(`${r.title} ${r.hint} ${r.words}`));
  const showAdd = onAddProfile !== undefined && matches(`${t.addProfile} ${t.settingsRows.addProfile} ${t.settingsKeywords.addProfile}`);
  const nothing = terms.length > 0 && shown.length === 0 && !showAdd;

  // Type to search: a letter typed while nothing else takes text goes to the search field (the keypress then lands in it).
  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent): void => {
      if (e.ctrlKey || e.metaKey || e.altKey || e.key.length !== 1 || e.key === " ") return;
      const el = e.target as HTMLElement | null;
      if (el !== null && (el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName) || el.closest("[role=dialog]") !== null)) return;
      search.current?.focus();
    };
    globalThis.addEventListener("keydown", onKey);
    return () => { globalThis.removeEventListener("keydown", onKey); };
  }, []);
  const onSearchKey = (e: KeyboardEvent<HTMLInputElement>): void => {
    if (e.key === "Escape") { setQuery(""); return; }
    if (e.key !== "Enter") return;
    const first = shown[0];
    if (first !== undefined) onOpen(first.id);
    else if (showAdd) onAddProfile();
  };
  return (
    <nav aria-label={t.settingsHeading} className="om-menu">
      <header className="om-page-head"><Text as="h1" size="xl" weight="bold">{t.settingsHeading}</Text></header>
      <div className="om-page om-menu__search">
        <label className="om-search">
          <span className="om-search__icon" aria-hidden="true"><Icon name="search" size={20} /></span>
          <span className="om-visually-hidden">{t.settingsSearch.label}</span>
          <input
            ref={search} type="search" className="om-search__input" value={query} placeholder={t.settingsSearch.label} enterKeyHint="go"
            autoComplete="off" autoCapitalize="off" autoCorrect="off" spellCheck={false}
            onChange={(e) => { setQuery(e.target.value); }} onKeyDown={onSearchKey}
          />
        </label>
      </div>
      {onSafetyCode !== undefined && terms.length === 0 ? (
        <section className="om-page" aria-label={t.safetyCode.action}>
          <div className="om-card">
            <Text as="p">{t.safetyCode.reminder}</Text>
            <Button variant="primary" onClick={onSafetyCode}><Icon name="shield" size={20} />{t.safetyCode.action}</Button>
          </div>
        </section>
      ) : null}
      <ul className="om-menu__list">
        {shown.map((r) => (
          <li key={r.id}>
            <button type="button" className="om-menu__row" aria-current={page === r.id ? "page" : undefined} onClick={() => { onOpen(r.id); }}>
              <span className="om-menu__icon" aria-hidden="true"><Icon name={r.icon} size={22} /></span>
              <span className="om-menu__text"><span className="om-menu__title">{r.title}</span><span className="om-menu__hint">{r.hint}</span></span>
              <Icon name="chevron" size={20} />
            </button>
          </li>
        ))}
        {showAdd ? (
          <li>
            <button type="button" className="om-menu__row" onClick={onAddProfile}>
              <span className="om-menu__icon" aria-hidden="true"><Icon name="userPlus" size={22} /></span>
              <span className="om-menu__text"><span className="om-menu__title">{t.addProfile}</span><span className="om-menu__hint">{t.settingsRows.addProfile}</span></span>
              <Icon name="plus" size={20} />
            </button>
          </li>
        ) : null}
      </ul>
      {nothing ? <p className="om-page om-menu__none" role="status">{t.settingsSearch.none(query.trim())}</p> : null}
      <div className="om-page om-stack">
        <InstallAffordance />
        <CopyNotice />
      </div>
    </nav>
  );
}
