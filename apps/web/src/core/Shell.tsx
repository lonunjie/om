// The connected application shell shared by the production Core composition and the dev/e2e entries: conversation
// list, requests, conversation view, composer, Settings > Devices / Recovery. It only talks to a CoreClient.
import { useEffect, useRef, useState, type ReactNode } from "react";
import type { ConversationId, IntroductionRequest, OperationId } from "@om/platform-api";
import {
  AppShell, Button, Composer, ConnectedProfileSwitcher, ConversationList, ConversationView, DevicesScreen, HomeScreen, LiveRegion, MessageRequest, RecoveryScreen, StrangerPressure, Text,
  AcceptInvite, InviteFirst, RecoveryKeyStep, useConversationList, type ConversationServices, type HomeSettingsServices, type OnboardingPlatform,
} from "@om/ui";
import { appStrings as t, conversationCopy, onboardingCopy, deviceScannerCopy, devicesCopy, scannerCopy, homeCopy, recoveryCopy, shellLabels, switcherLabels } from "../copy/en";
import { useQrScanner } from "../qr/Scanner";
import { CopyNotice } from "../CopyNotice";
import "./shell.css";
import { InstallAffordance } from "../install/InstallAffordance";

type SettingsView = "menu" | "devices" | "recovery" | "home";

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
  const [selected, setSelected] = useState<ConversationId | null>(null);
  const [settings, setSettings] = useState<SettingsView | null>(null);
  const [savingCode, setSavingCode] = useState(false);
  const deviceScan = useQrScanner(deviceScannerCopy);
  const [drafts, setDrafts] = useState<ReadonlyMap<ConversationId, string>>(() => new Map());
  const list = useConversationList(services);
  const chatsBeforeAccept = useRef<readonly string[]>([]);
  const select = (id: ConversationId): void => { setSelected(id); };
  // An accepted invite adds its chat at once (it says it is waiting for the other person): open it so the person sees what happened.
  const newChatAfter = incoming?.newChatAfter ?? null;
  const newChatOpened = incoming?.onNewChatOpened;
  useEffect(() => {
    if (newChatAfter === null || list.status !== "ready") return;
    const fresh = list.value.find((c) => !newChatAfter.includes(c.id));
    if (fresh === undefined) return;
    setSelected(fresh.id);
    newChatOpened?.();
  }, [newChatAfter, list, newChatOpened]);
  const profileControl = (
    <>
      <ConnectedProfileSwitcher labels={switcherLabels} />
      {onAddProfile ? <Button variant="quiet" onClick={onAddProfile}>{t.addProfile}</Button> : null}
      <Button variant="quiet" aria-pressed={settings !== null} onClick={() => { setSettings(settings === null ? "menu" : null); }}>{t.settings}</Button>
    </>
  );
  if (savingCode && safetyCode !== undefined) {
    return (
      <AppShell
        destinations={[{ id: "chats", label: t.chats }]}
        activeDestination="chats"
        onDestinationChange={() => undefined}
        conversationOpen
        labels={shellLabels}
        list={<Text>{t.loading}</Text>}
        detail={(
          <>
            <Button variant="quiet" onClick={() => { setSavingCode(false); }}>{t.backToChats}</Button>
            <RecoveryKeyStep
              services={services} platform={safetyCode.platform} keyOperationId={safetyCode.keyOperationId} copy={onboardingCopy.recovery}
              onConfirmed={() => { setSavingCode(false); safetyCode.onResolved(); }}
            />
          </>
        )}
        omnibarTrigger={<Button disabled>{t.omnibar}</Button>}
        profileControl={profileControl}
      />
    );
  }
  if (settings !== null) {
    return (
      <>
        <AppShell
          destinations={[{ id: "chats", label: t.chats }]}
          activeDestination="chats"
          onDestinationChange={() => undefined}
          conversationOpen={settings !== "menu"}
          labels={shellLabels}
          list={<SettingsMenu onOpen={setSettings} onClose={() => { setSettings(null); }} hasHome={home !== undefined} />}
          detail={settings === "menu" ? <Text>{t.settingsHeading}</Text> : (
            <>
              <Button variant="quiet" onClick={() => { setSettings("menu"); }}>{t.backToSettings}</Button>
              {settings === "devices" ? (
                <>
                  <DevicesScreen services={services} copy={devicesCopy} scanQr={deviceScan.scanQr} />
                  {devicesExtra}
                </>
              ) : settings === "home" && home !== undefined ? <HomeScreen services={home} copy={homeCopy} /> : <RecoveryScreen services={services} copy={recoveryCopy} />}
            </>
          )}
          omnibarTrigger={<Button disabled>{t.omnibar}</Button>}
          profileControl={profileControl}
        />
        {deviceScan.dialog}
      </>
    );
  }
  return (
    <>
      <AppShell
        destinations={[{ id: "chats", label: t.chats }]}
        activeDestination="chats"
        onDestinationChange={() => undefined}
        conversationOpen={selected !== null}
        labels={shellLabels}
        list={
          <>
            {incoming?.token != null ? (
              <section className="om-dev-requests">
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
              <section className="om-dev-requests" aria-label={t.safetyCode.action}>
                <Text as="p">{t.safetyCode.reminder}</Text>
                <Button onClick={() => { setSavingCode(true); }}>{t.safetyCode.action}</Button>
              </section>
            )}
            {list.status === "loading" ? <Text>{t.loading}</Text> : null}
            {list.status === "failed" ? <Text>{t.loadFailed}</Text> : null}
            {list.status === "ready" && list.value.length === 0 && invite !== undefined && incoming?.token == null ? (
              <InviteFirst
                services={services} platform={invite.platform} copy={onboardingCopy.invite} renderQr={invite.renderQr} scanQr={inviteScan.scanQr} {...(invite.shareLink ? { shareLink: invite.shareLink } : {})}
                onAccepted={() => undefined}
              />
            ) : null}
            {list.status === "ready" && !(list.value.length === 0 && invite !== undefined && incoming?.token == null) ? <ConversationList conversations={list.value} selectedId={selected} onSelect={select} copy={conversationCopy} /> : null}
          </>
        }
        detail={
          selected === null ? <Text>{t.selectConversation}</Text> : (
            <>
              <Button variant="quiet" onClick={() => { setSelected(null); }}>{t.back}</Button>
              <ConversationView key={selected} services={services} conversationId={selected} copy={conversationCopy} />
            </>
          )
        }
        composerZone={selected === null ? undefined : (
          <Composer
            key={selected} services={services} conversationId={selected} copy={conversationCopy} draft={drafts.get(selected) ?? ""}
            onDraftChange={(next) => { setDrafts((d) => new Map(d).set(selected, next)); }}
          />
        )}
        omnibarTrigger={<Button disabled>{t.omnibar}</Button>}
        profileControl={profileControl}
      />
      {inviteScan.dialog}
      <LiveRegion>{selected === null ? "" : t.conversationOpened}</LiveRegion>
    </>
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
    <section className="om-dev-requests" aria-label={t.requestsHeading}>
      {shown.map((r) => (
        <MessageRequest key={r.id} services={services} request={r} copy={conversationCopy} onIgnore={hide} />
      ))}
    </section>
  );
}

/** Settings is reached from the profile area (Profile -> Settings), not a tab: Chats stays the compact root. */
function SettingsMenu({ onOpen, onClose, hasHome }: { readonly onOpen: (v: SettingsView) => void; readonly onClose: () => void; readonly hasHome: boolean }) {
  return (
    <nav aria-label={t.settingsHeading} className="om-dev-settings">
      <Text as="h1" size="xl" weight="bold">{t.settingsHeading}</Text>
      <ul className="om-dev-settings__list">
        <li><Button className="om-dev-settings__row" onClick={() => { onOpen("devices"); }}>{t.settingsDevices}</Button></li>
        <li><Button className="om-dev-settings__row" onClick={() => { onOpen("recovery"); }}>{t.settingsRecovery}</Button></li>
        {hasHome ? <li><Button className="om-dev-settings__row" onClick={() => { onOpen("home"); }}>{t.settingsHome}</Button></li> : null}
      </ul>
      <InstallAffordance />
      <CopyNotice />
      <Button variant="quiet" onClick={onClose}>{t.backToChats}</Button>
    </nav>
  );
}

