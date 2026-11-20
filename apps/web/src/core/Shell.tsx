import type { CallMediaProvider } from "@om/provider-http";
// The connected application shell shared by the production Core composition and the dev/e2e entries: conversation
// list, requests, conversation view, composer, Settings > Devices / Recovery. It only talks to a CoreClient.
import { useCallback, useEffect, useMemo, useRef, useState, type ComponentProps, type KeyboardEvent, type ReactNode } from "react";
import { asConversationId, asGroupId, parseInviteLink, type ConversationId, type ConversationSummary, type IntroductionRequest, type MessageId, type OperationId } from "@om/platform-api";
import {
  AlertsScreen, CallTimelineNotice, useAlertsRefresh, AppShell, Avatar, Button, Composer, ExpressivePickerControl, ConnectedProfileSwitcher, ConversationList, ConversationView, DevicesScreen, ProtectionScreen, HomeFacts, HomeScreen, Icon, IconButton, LiveRegion, MessageRequest, Omnibar, OmnibarTrigger, RecoveryScreen, Sheet, StrangerPressure, Text, useActiveProfile, useLayoutMode, useLocalSearch,
  type IconName, type OmniEntry, scoreEntry, useProfiles, useConversationDigests, shownName, contactCollisions, introductionCollisions,
  GroupChat, GroupComposer, GroupInvites, GroupRejoinRequests, GroupStatus, HeldGroupInvites, NewGroup, digestOfGroup, groupApiOf, useGroupEvents, useGroups, useHeldGroupInvites,
  AcceptInvite, InviteFirst, ReconnectContacts, RecoveryKeyStep, useLiveQuery, useConversationList, type ConversationServices, type HomeSettingsServices, type OnboardingPlatform,
} from "@om/ui";
import { useScheduledSender } from "@om/ui";
import type { AlertsServices, ComposerCapture, ExpressiveHost, ReplyTarget, ScheduleControl, ScheduledItem, SearchStore, StagedItem } from "@om/ui";
import { appStrings as t, callCopy, conversationCopy, omnibarCopy, onboardingCopy, devicesCopy, scannerCopy, homeCopy, protectionCopy, alertsCopy, recoveryCopy, reconnectCopy, approveRecoveryCopy, replaceRecoveryCopy, shellLabels, switcherLabels, groupsCopy, groupsKeywords, groupsLoadCopy, expressiveCopy } from "../copy/en";
import { QrCode } from "../qr/QrCode";
import { useQrScanner } from "../qr/Scanner";
import { CopyNotice } from "../CopyNotice";
import { CHATS, placeTitle, placeUrl, usePlace, type SettingsPage } from "../route/place";
import "./shell.css";
import { InstallAffordance } from "../install/InstallAffordance";
import { HelpScreen } from "./HelpScreen";
import { BackupScreen } from "./BackupScreen";
import type { BackupServices } from "./backupServices";
import { LockScreen } from "./LockScreen";
import { GroupsGate, type GroupsSetup } from "./GroupsGate";
import { takeOpenNewGroup } from "./loadCore";
import type { LockServices } from "./lockServices";
import type { CallDriver } from "@om/core-wasm";
import { useCalls } from "./useCalls";
import { addBusyProbe } from "../sw/busy";
import { CallHost } from "./CallHost";

const useCallSurface = import.meta.env.VITE_OM_CALLS === "1" ? useCalls : () => undefined;

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
  /** When each chat was last shown on this device, so the list can count what arrived since. Without it nothing is marked unread. */
  readonly loadSeen?: () => Promise<ReadonlyMap<ConversationId, number>>;
  readonly saveSeen?: (conversationId: ConversationId, at: number) => void;
  /** Device-local scheduled send (ADR-0224). Without it no scheduling is offered. */
  readonly loadScheduled?: () => Promise<readonly ScheduledItem[]>;
  readonly saveScheduled?: (items: readonly ScheduledItem[]) => void;
}

const NO_STAGED: readonly StagedItem[] = [];

export interface ShellProps {
  readonly services: ConversationServices;
  readonly callDriver?: CallDriver | undefined;
  readonly callMedia?: CallMediaProvider | undefined;
  /** Present while this device has not loaded the groups module: groups are then offered, and the first use fetches it (ADR-0187). */
  readonly groupsSetup?: GroupsSetup;
  /** What this device can attach or record. Each control is offered only where it exists. */
  readonly capture?: ComposerCapture;
  /** Extra content under the Devices screen (dev harness only). */
  readonly devicesExtra?: ReactNode;
  readonly expressiveHost?: ExpressiveHost | undefined;
  readonly richTextFor?: ComponentProps<typeof ConversationView>["richTextFor"];
  /** Offers "Add identity" next to the profile switcher when provided. */
  readonly onAddProfile?: () => void;
  /** Settings > Your Home, when the host can configure its Home (the production entry; not the dev harness). */
  readonly home?: HomeSettingsServices;
  readonly safetyCode?: SafetyCodeOffer;
  /** Settings > Lock, when the host can read how this device opens OM (the production entry). */
  readonly lock?: LockServices;
  /** Settings > Backup file, when the host can save and restore one (the production entry). */
  readonly backup?: BackupServices;
  /** Settings > Alerts: a notice when OM is closed (ADR-0210), when the host can offer it (the production entry). */
  readonly alerts?: AlertsServices;
  /** Where drafts are kept between visits (the production entry); without it they last until the page closes. */
  readonly draftSync?: DraftSync;
  /** Where the search index is kept at rest, sealed by the identity's vault (ADR-0208); without it the index lasts until the screen goes away. */
  readonly searchStore?: SearchStore;
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

export function Shell({ services, callDriver, callMedia, groupsSetup, capture, devicesExtra, expressiveHost, richTextFor, onAddProfile, home, lock, backup, alerts, draftSync, searchStore, safetyCode, invite, incoming }: ShellProps) {
  useAlertsRefresh(services, alerts); // renews the Home's registration at start when alerts are on (ADR-0210)
  const inviteScan = useQrScanner(scannerCopy);
  const { place, raw, go, back } = usePlace();
  const me = useActiveProfile();
  const { profiles } = useProfiles();
  const expressive = expressiveHost !== undefined && services.client.expressive?.available() === true
    ? { ...expressiveHost, ...(capture?.files === undefined ? {} : { files: capture.files }), copy: expressiveCopy } : undefined;
  const calls = useCallSurface(services, callDriver, callMedia);
  const mode = useLayoutMode();
  // A replacement safety code (ADR-0205) being shown from Settings > Recovery; nothing changes until the person confirms it is saved.
  const [replacingKey, setReplacingKey] = useState<OperationId | null>(null);
  // Showing a new safety code is a ceremony: a reload before it is confirmed saved would lose the only copy, so an update waits.
  useEffect(() => (replacingKey === null ? undefined : addBusyProbe(() => true)), [replacingKey]);
  const [drafts, setDrafts] = useState<ReadonlyMap<ConversationId, string>>(() => new Map());
  // Files and voice notes waiting to be sent. Memory only: they survive switching chats but never reach disk, and locking or switching identity drops them.
  const [replies, setReplies] = useState<ReadonlyMap<ConversationId, ReplyTarget>>(() => new Map());
  const [staged, setStaged] = useState<ReadonlyMap<ConversationId, readonly StagedItem[]>>(() => new Map());
  // Scheduled send (ADR-0224): the queue lives on this device only, sealed with the identity's other private state, and is released into the
  // ordinary send path at the time chosen, or at the next opportunity if OM was not open then.
  const [scheduled, setScheduledItems] = useState<readonly ScheduledItem[]>([]);
  useEffect(() => {
    if (draftSync?.loadScheduled === undefined) return undefined;
    let live = true;
    draftSync.loadScheduled().then((saved) => { if (live) setScheduledItems((now) => (now.length === 0 ? saved : [...saved.filter((s) => !now.some((n) => n.id === s.id)), ...now])); }, () => undefined);
    return () => { live = false; };
  }, [draftSync]);
  const keepScheduled = useCallback((next: readonly ScheduledItem[]): void => { setScheduledItems(next); draftSync?.saveScheduled?.(next); }, [draftSync]);
  const sender = useScheduledSender({ services, items: draftSync?.saveScheduled === undefined ? [] : scheduled, onItems: keepScheduled });
  const scheduleControl = useMemo<ScheduleControl | undefined>(() => draftSync?.saveScheduled === undefined ? undefined : ({
    copy: conversationCopy.schedule, items: scheduled, notices: sender.notices, now: Date.now, dismiss: sender.dismiss,
    add: (conversationId, text, dueAt) => { keepScheduled([...scheduled, { id: crypto.randomUUID(), conversationId: conversationId as ConversationId, text, dueAt, createdAt: Date.now() }]); },
    cancel: (id) => { keepScheduled(scheduled.filter((x) => x.id !== id)); },
  }), [draftSync, scheduled, sender.notices, sender.dismiss, keepScheduled]);
  // Drafts saved on an earlier visit (or before a lock) come back; anything typed since this screen opened wins.
  useEffect(() => {
    if (draftSync === undefined) return;
    let live = true;
    draftSync.load().then((saved) => { if (live && saved.size > 0) setDrafts((now) => new Map([...saved, ...now])); }, () => undefined);
    return () => { live = false; };
  }, [draftSync]);
  const list = useConversationList(services);
  const reconnect = useLiveQuery(services, { kind: "GetRelationshipRestore", ctx: services.ctx }, "reconnect");
  const reconnectContacts = reconnect.status === "ready" ? reconnect.value.value : [];
  // What each row shows beyond name and time: the latest message, and what has arrived since the chat was last shown here.
  const groups = useGroups(services);
  // Invitations held before this device has the group files (D70): listed by a base-module query, never accepted.
  const heldGroups = useHeldGroupInvites(services);
  const groupApi = groupApiOf(services);
  // What the group carriage reports in the background (held, undeliverable, blocked), shown as honest states; a driver-less build reports nothing.
  const groupEvents = useGroupEvents(services, () => { groups.refresh(); });
  const oneToOneIds = useMemo(() => (list.status === "ready" ? list.value.map((c) => c.id) : []), [list]);
  const chatDigests = useConversationDigests(services, oneToOneIds);
  // Groups sit in the same list as chats (they are conversations with more people). Their rows are read from the group views, newest first.
  const groupRows = useMemo(() => {
    const rows: ConversationSummary[] = [];
    const digs = new Map<ConversationId, ReturnType<typeof digestOfGroup>>();
    const marks = new Map<ConversationId, string>();
    if (groupApi === null) return { rows, digs, marks };
    for (const g of groups.groups) {
      const id = asConversationId(g.conversationId);
      let messages: ReturnType<typeof groupApi.messages> = [];
      try { messages = groupApi.messages(services.ctx, g.conversationId); } catch { /* stale context */ }
      const d = digestOfGroup(messages);
      digs.set(id, d);
      marks.set(id, groupsCopy.list.mark(g.members));
      rows.push({ id, personId: id as unknown as ConversationSummary["personId"], displayName: g.name, lastMessageAt: d.lastSentAt, notices: [], relationship: "active" });
    }
    return { rows, digs, marks };
  }, [groupApi, groups.groups, services.ctx]);
  const digests = useMemo(() => (groupRows.digs.size === 0 ? chatDigests : new Map([...chatDigests, ...groupRows.digs])), [chatDigests, groupRows]);
  const chatRows = useMemo<readonly ConversationSummary[] | null>(() => {
    if (list.status !== "ready") return null;
    if (groupRows.rows.length === 0) return list.value;
    return [...list.value, ...groupRows.rows].sort((a, b) => (b.lastMessageAt ?? 0) - (a.lastMessageAt ?? 0));
  }, [list, groupRows]);
  // Contacts who show the same name or names that look alike (ADR-0197), compared on the shown names; groups are not people and are left out.
  const contactNames = useMemo(() => contactCollisions(chatRows ?? [], (id) => groupRows.marks.has(id)), [chatRows, groupRows.marks]);
  const requestContacts = useMemo(() => (chatRows ?? []).filter((c) => !groupRows.marks.has(c.id)).map((c) => ({ id: c.id, name: shownName(c) })), [chatRows, groupRows.marks]);
  const chatIds = useMemo(() => (chatRows === null ? [] : chatRows.map((c) => c.id)), [chatRows]);
  const selectedIsGroup = (id: ConversationId | null): boolean => id !== null && groupRows.marks.has(id);
  const [seen, setSeenTimes] = useState<ReadonlyMap<ConversationId, number> | null>(null);
  useEffect(() => {
    if (draftSync?.loadSeen === undefined) return;
    let live = true;
    draftSync.loadSeen().then((saved) => { if (live) setSeenTimes(saved); }, () => { if (live) setSeenTimes(new Map()); });
    return () => { live = false; };
  }, [draftSync]);
  const chatsBeforeAccept = useRef<readonly string[]>([]);
  // A search hit names a message too; it is kept in memory only (never in the address) and dropped when the chat changes.
  const [hitFocus, setHitFocus] = useState<{ readonly chat: string; readonly messageId: MessageId; readonly ctx: unknown } | null>(null);
  const selected = place.kind === "chats" ? (place.chat as ConversationId | null) : null;
  const peerAccount = selected !== null && list.status === "ready" ? list.value.find((x) => x.id === selected)?.peerAccount : undefined;
  const peerName = selected !== null && list.status === "ready" ? (() => { const c = list.value.find((x) => x.id === selected); return c === undefined ? undefined : shownName(c); })() : undefined;
  useEffect(() => { if (hitFocus !== null && hitFocus.chat !== selected) setHitFocus(null); }, [hitFocus, selected]);
  const settings: SettingsPage | null = place.kind === "settings" ? place.page : null;
  const savingCode = place.kind === "safety-code" && safetyCode !== undefined;
  // The open chat is shown, so it is seen; a chat met for the first time starts as seen (history that was here before is not "new").
  useEffect(() => {
    if (seen === null || draftSync?.saveSeen === undefined) return;
    const next = new Map(seen);
    for (const id of chatIds) {
      const d = digests.get(id);
      if (d === undefined) continue;
      const latest = d.lastSentAt ?? 0;
      if (id === selected ? (next.get(id) ?? -1) < latest : !next.has(id)) { next.set(id, latest); draftSync.saveSeen(id, latest); }
    }
    if (next.size !== seen.size || [...next].some(([id, at]) => seen.get(id) !== at)) setSeenTimes(next);
  }, [seen, digests, chatIds, selected, draftSync]);
  const unread = useMemo(() => {
    const out = new Map<ConversationId, number>();
    if (seen === null) return out;
    for (const [id, d] of digests) {
      const mark = seen.get(id);
      if (mark === undefined || id === selected) continue;
      const n = d.incomingAt.filter((at) => at > mark).length;
      if (n > 0) out.set(id, n);
    }
    return out;
  }, [seen, digests, selected]);
  const select = (id: ConversationId): void => { go({ kind: "chats", chat: id }); };
  const toChats = (): void => { back(CHATS); };

  // Omnibar (find or do) and the new-conversation sheet. Both are local: nothing typed in the Omnibar leaves this device.
  const [omniOpen, setOmniOpen] = useState(false);
  const [newGroupOpen, setNewGroupOpen] = useState(false);
  const [groupsGateOpen, setGroupsGateOpen] = useState<"new_group" | "invitation" | "rejoin" | null>(null);
  // Groups are offered when this device has them, or can get them on first use. Without the module the first tap fetches it; with it, it opens the sheet.
  const groupsOffered = groups.available || groupsSetup !== undefined;
  // Once the groups are ready the held invitations become ordinary pending ones: the loader has done its job.
  useEffect(() => { if (groups.available) setGroupsGateOpen(null); }, [groups.available]);
  const startNewGroup = (): void => { if (groups.available) setNewGroupOpen(true); else setGroupsGateOpen("new_group"); };
  // The app restarts onto the groups module after its first load: take the person back to the sheet they asked for.
  useEffect(() => { if (groups.available && takeOpenNewGroup()) setNewGroupOpen(true); }, [groups.available]);
  const [groupTick, setGroupTick] = useState(0);
  const [newChat, setNewChat] = useState<{ readonly focus: "share" | "scan" | "qr"; readonly link: string | undefined } | null>(null);
  const [acceptedAfter, setAcceptedAfter] = useState<readonly string[] | null>(null);
  const chatIdsNow = useRef<readonly string[]>([]);
  chatIdsNow.current = list.status === "ready" ? list.value.map((c) => c.id) : [];
  const chatIdsAtOpen = useRef<readonly string[]>([]);
  const openNew = useCallback((focus: "share" | "scan" | "qr", link?: string): void => {
    chatIdsAtOpen.current = chatIdsNow.current;
    setOmniOpen(false);
    setNewChat({ focus, link });
  }, []);
  // Cmd/Ctrl+K opens the same Omnibar as the visible control (and closes it again).
  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent): void => {
      if (!(e.metaKey || e.ctrlKey) || e.altKey || e.shiftKey || e.key.toLowerCase() !== "k") return;
      e.preventDefault();
      if (newChat === null) setOmniOpen((open) => !open);
    };
    globalThis.addEventListener("keydown", onKey);
    return () => { globalThis.removeEventListener("keydown", onKey); };
  }, [newChat]);
  // A chat added from the sheet opens, like one added from an invitation link.
  useEffect(() => {
    if (acceptedAfter === null || list.status !== "ready") return;
    const fresh = list.value.find((c) => !acceptedAfter.includes(c.id));
    if (fresh === undefined) return;
    go({ kind: "chats", chat: fresh.id });
    setAcceptedAfter(null);
  }, [acceptedAfter, list, go]);

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
  const relinked = selected === null ? undefined : chatRows?.find((c) => c.relinkedFrom === selected);
  useEffect(() => {
    if (relinked !== undefined) go({ kind: "chats", chat: relinked.id }, { replace: true });
  }, [relinked, go]);
  const chatGone = selected !== null && newChatAfter === null && relinked === undefined && chatRows !== null && !chatRows.some((c) => c.id === selected);
  /** Copies a public line (the setup value for a backup delivery service) through the Action Registry like every platform capability. */
  const copyText = async (text: string): Promise<boolean> => {
    if (invite === undefined) return false;
    const clipboard = invite.platform.clipboard();
    const r = services.registry.resolve({ actionId: "copy-text", params: { sensitive: false }, ctx: services.ctx, origin: "direct" });
    if (clipboard.state === "unavailable" || r.status !== "platform" || r.request.capability !== "Clipboard") return false;
    return (await clipboard.api.write(text, { sensitive: false })) === "written";
  };
  const noSuchPlace = (place.kind === "safety-code" && safetyCode === undefined) || (place.kind === "settings" && place.page === "delivery" && home === undefined) || (place.kind === "settings" && place.page === "lock" && lock === undefined) || (place.kind === "settings" && place.page === "backup" && backup === undefined) || (place.kind === "settings" && place.page === "alerts" && alerts === undefined) || chatGone;
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
  const shortcut = typeof navigator !== "undefined" && /mac|iphone|ipad/i.test(navigator.platform) ? "\u2318K" : "Ctrl K";
  const omniEntries = useMemo<readonly OmniEntry[]>(() => {
    const out: OmniEntry[] = [];
    chatRows?.forEach((c, i) => {
      const n = contactNames.of(c.id);
      const name = shownName(c);
      out.push({
        id: `chat-${c.id}`, kind: "conversation", label: n === null ? name : groupsCopy.sameName.of(name, n.index, n.of),
        ...(n === null ? {} : { cue: t.omniNameCue[n.kind] }),
        suggested: i < 3, run: () => { go({ kind: "chats", chat: c.id }); },
      });
    });
    if (invite !== undefined) {
      out.push({ id: "invite", kind: "action", label: t.omniActions.invite, keywords: t.omniKeywords.invite, suggested: true, run: () => { openNew("share"); } });
      out.push({ id: "scan", kind: "action", label: t.omniActions.scan, keywords: t.omniKeywords.scan, suggested: true, run: () => { openNew("scan"); } });
      out.push({ id: "qr", kind: "action", label: t.omniActions.showQr, keywords: t.omniKeywords.showQr, suggested: true, run: () => { openNew("qr"); } });
    }
    if (groupsOffered) {
      // UI-10: the entry is bound to the profile it was made for; run after a switch and it does nothing.
      const made = services.ctx;
      out.push({ id: "new-group", kind: "action", label: groupsCopy.newGroup.action, keywords: groupsKeywords, run: () => { if (made === services.ctx) { setOmniOpen(false); startNewGroup(); } } });
    }
    out.push({ id: "chats", kind: "action", label: t.omniActions.chats, keywords: t.omniKeywords.chats, run: () => { go(CHATS); } });
    out.push({ id: "settings", kind: "setting", label: t.omniActions.settings, keywords: t.omniKeywords.settings, run: () => { go({ kind: "settings", page: "menu" }); } });
    const pages: readonly { readonly page: SettingsPage; readonly title: string; readonly words: string; readonly on: boolean }[] = [
      { page: "lock", title: t.settingsLock, words: t.settingsKeywords.lock, on: lock !== undefined },
      { page: "devices", title: t.settingsDevices, words: t.settingsKeywords.devices, on: true },
      { page: "recovery", title: t.settingsRecovery, words: t.settingsKeywords.recovery, on: true },
      { page: "protection", title: t.settingsProtection, words: t.settingsKeywords.protection, on: true },
      { page: "backup", title: t.settingsBackup, words: t.settingsKeywords.backup, on: backup !== undefined },
      { page: "alerts", title: t.settingsAlerts, words: t.settingsKeywords.alerts, on: alerts !== undefined },
      { page: "delivery", title: t.settingsHome, words: t.settingsKeywords.delivery, on: home !== undefined },
      { page: "help", title: t.settingsHelp, words: t.settingsKeywords.help, on: true },
    ];
    for (const p of pages) if (p.on) out.push({ id: `settings-${p.page}`, kind: "setting", label: p.title, keywords: p.words, run: () => { go({ kind: "settings", page: p.page }); } });
    if (onAddProfile !== undefined) out.push({ id: "add-profile", kind: "action", label: t.addProfile, keywords: t.settingsKeywords.addProfile, run: onAddProfile });
    if (safetyCode !== undefined) out.push({ id: "safety-code", kind: "setting", label: t.safetyCode.action, keywords: t.settingsKeywords.recovery, run: () => { go({ kind: "safety-code" }); } });
    return out;
  }, [chatRows, contactNames, groupsOffered, services.ctx, invite, lock, backup, alerts, home, onAddProfile, safetyCode, go, openNew]);
  // A pasted invitation link is explicit intent: it is offered first and leads to the sheet with the link already in place.
  const omniDynamic = useCallback((query: string): readonly OmniEntry[] => {
    const [command, ...words] = query.trim().toLowerCase().split(/\s+/);
    const kind = command === callCopy.commands.audio ? "audio" : command === callCopy.commands.video ? "video" : null;
    if (calls && command !== undefined && kind && words.length > 0 && list.status === "ready") {
      return list.value.flatMap((contact) => {
        const label = callCopy.commandLabel(command, shownName(contact));
        const entry: OmniEntry = { id: `${command}-${contact.id}`, kind: "action", label, run: () => { calls.start(contact, kind); } };
        return contact.allowCalls !== undefined && scoreEntry(entry, query) !== null ? [entry] : [];
      });
    }
    if (invite === undefined || !parseInviteLink(query).ok) return [];
    return [{ id: "use-link", kind: "action", label: t.omniActions.useLink, keywords: t.omniKeywords.useLink, run: () => { openNew("share", query.trim()); } }];
  }, [invite, openNew, calls, list]);
  // Search over what this device has already decrypted, chats and groups: kept at rest only as ciphertext in this identity's vault (ADR-0208), read back after unlock, gone with the identity.
  const localSearch = useLocalSearch(services, searchStore);
  // The search function is pinned while the Omnibar is open, so a message arriving meanwhile cannot move a result under the person's finger (NV-06).
  const searchWhileOpen = useRef({ search: localSearch.search, names: contactNames, rows: chatRows });
  if (!omniOpen) searchWhileOpen.current = { search: localSearch.search, names: contactNames, rows: chatRows };
  const omniFound = useCallback((query: string): readonly OmniEntry[] => {
    const { search, names, rows } = searchWhileOpen.current;
    return search(query).map((hit, i) => {
      const n = names.of(hit.conversationId);
      const row = rows?.find((c) => c.id === hit.conversationId);
      const name = row === undefined ? hit.displayName : shownName(row);
      return {
        id: `found-${String(i)}`, kind: hit.kind, label: hit.snippet,
        detail: n === null ? name : groupsCopy.sameName.of(name, n.index, n.of),
        ...(n === null ? {} : { cue: t.omniNameCue[n.kind] }),
        run: () => { setHitFocus({ chat: hit.conversationId, messageId: hit.messageId, ctx: services.ctx }); go({ kind: "chats", chat: hit.conversationId }); },
      };
    });
  }, [go, services.ctx]);
  const openOmni = (): void => { setOmniOpen(true); };
  const trigger = <OmnibarTrigger copy={omnibarCopy} shortcut={shortcut} onOpen={openOmni} />;
  const callContact = calls && list.status === "ready" ? list.value.find((row) => row.id === calls.view?.conversationId) : undefined;
  const hasCall = calls && (calls.view !== null || calls.pending || calls.failure !== null);
  const common = {
    liveCall: hasCall ? <CallHost calls={calls} contact={callContact} name={callContact ? shownName(callContact) : callCopy.unknownContact}
      identity={profiles.length > 1 ? me?.label : undefined} /> : undefined,
    destinations: [{ id: "chats", label: t.chats, icon: <Icon name="chat" size={20} /> }], activeDestination: settings === null && !savingCode ? "chats" : "",
    onDestinationChange: () => { if (place.kind !== "chats" || place.chat !== null) toChatsRoot(); }, labels: shellLabels, profileControl,
    omnibarTrigger: trigger,
    omnibar: omniOpen ? <Omnibar entries={omniEntries} dynamic={omniDynamic} found={omniFound} copy={omnibarCopy} onClose={() => { setOmniOpen(false); }} /> : undefined,
  } as const;
  // Starting a conversation is an action within Chats, so it is a sheet over the place, not another place (COMPONENT_BEHAVIOR_AND_STATES.md s8).
  const newChatSheet = newChat !== null && invite !== undefined ? (
    <Sheet title={t.newConversation.title} closeLabel={t.newConversation.close} onClose={() => { setNewChat(null); }}>
      <InviteFirst
        services={services} platform={invite.platform} copy={onboardingCopy.invite} renderQr={invite.renderQr} scanQr={inviteScan.scanQr} {...(invite.shareLink ? { shareLink: invite.shareLink } : {})}
        focusChoice={newChat.focus} {...(newChat.link === undefined ? {} : { initialLink: newChat.link })}
        onAccepted={() => { setAcceptedAfter(chatIdsAtOpen.current); setNewChat(null); }}
      />
    </Sheet>
  ) : null;

  if (replacingKey !== null && invite !== undefined) {
    const done = (): void => { setReplacingKey(null); };
    return (
      <>
        <AppShell
          {...common}
          conversationOpen
          onBack={done}
          list={<Text>{t.loading}</Text>}
          detail={(
            <div className="om-page">
              <SubPage label={t.backToSettings} onBack={done}>
                <RecoveryKeyStep services={services} platform={invite.platform} keyOperationId={replacingKey} copy={onboardingCopy.recovery} onConfirmed={done} />
              </SubPage>
            </div>
          )}
        />
      </>
    );
  }
  if (savingCode) {
    return (
      <>
        <AppShell
          {...common}
          conversationOpen
          onBack={toChats}
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
        {inviteScan.dialog}
        {newChatSheet}
      </>
    );
  }
  if (settings !== null) {
    return (
      <>
        <AppShell
          {...common}
          conversationOpen={settings !== "menu"}
          onBack={() => { back({ kind: "settings", page: "menu" }); }}
          list={(
            <SettingsMenu
              page={settings} onOpen={(page) => { go({ kind: "settings", page }); }} onAddProfile={onAddProfile} hasHome={home !== undefined} hasLock={lock !== undefined} hasBackup={backup !== undefined} hasAlerts={alerts !== undefined}
              onSafetyCode={safetyCode === undefined ? undefined : () => { go({ kind: "safety-code" }); }}
            />
          )}
          detail={settings === "menu" ? <div className="om-page om-page--empty"><span className="om-empty-art" aria-hidden="true"><Icon name="settings" size={72} /></span></div> : (
            <div className="om-page">
              <SubPage label={t.backToSettings} onBack={() => { back({ kind: "settings", page: "menu" }); }}>
                {settings === "devices" ? (
                  <>
                    <DevicesScreen services={services} copy={devicesCopy} showNotices={false} scanQr={inviteScan.scanQr} recoveryPaused={{ ...replaceRecoveryCopy, open: () => { go({ kind: "settings", page: "recovery" }); } }} renderQr={(payload) => <QrCode payload={payload} />} />
                    {expressive === undefined ? null : <Text as="p">{expressive.copy.perDevice}</Text>}
                    {devicesExtra}
                  </>
                ) : settings === "delivery" && home !== undefined ? <HomeScreen services={home} copy={homeCopy} {...(invite === undefined ? {} : { copyText })} facts={<HomeFacts client={services.client} ctx={services.ctx} copy={onboardingCopy.home} alwaysOpen configured={home.current.baseUrl !== undefined} />} />
                  : settings === "protection" ? <ProtectionScreen services={services} copy={protectionCopy} />
                  : settings === "lock" && lock !== undefined ? <LockScreen services={lock} />
                  : settings === "backup" && backup !== undefined ? <BackupScreen services={backup} />
                  : settings === "alerts" && alerts !== undefined ? <><AlertsScreen services={services} alerts={alerts} copy={alertsCopy} />{calls ? <Text as="p">{callCopy.openApp}</Text> : null}</>
                  : settings === "help" ? <HelpScreen />
                  : <RecoveryScreen services={services} copy={recoveryCopy} approve={{ copy: approveRecoveryCopy, renderQr: (payload) => <QrCode payload={payload} /> }} {...(invite ? { replace: { copy: replaceRecoveryCopy, onStarted: setReplacingKey } } : {})} />}
              </SubPage>
            </div>
          )}
        />
        {inviteScan.dialog}
        {newChatSheet}
      </>
    );
  }
  const emptyInviteFirst = reconnectContacts.length === 0 && list.status === "ready" && list.value.length === 0 && groups.groups.length === 0 && groups.invites.length === 0 && heldGroups.held.invitations.length === 0 && heldGroups.held.overflow.invites === 0 && heldGroups.held.overflow.replies === 0 && invite !== undefined && incoming?.token == null;
  return (
    <>
      <AppShell
        {...common}
        conversationOpen={selected !== null}
        onBack={toChats}
        list={
          <>
            <header className="om-page-head om-page-head--row">
              <Text as="h1" size="xl" weight="bold">{t.chats}</Text>
              <span className="om-page-head__actions">
                {groupsOffered ? <Button variant="quiet" onClick={startNewGroup}><Icon name="users" size={20} />{groupsCopy.newGroup.action}</Button> : null}
                {invite !== undefined && !emptyInviteFirst ? <Button variant="quiet" onClick={() => { openNew("share"); }}><Icon name="plus" size={20} />{t.newConversation.action}</Button> : null}
              </span>
            </header>
            <HeldGroupInvites
              copy={groupsCopy} chats={list.status === "ready" ? list.value : []} groupsReady={groups.available || groupsSetup === undefined}
              held={heldGroups.held} onOpen={() => { setGroupsGateOpen("invitation"); }} onAcknowledge={heldGroups.acknowledge}
            />
            {groups.available || groupsSetup !== undefined ? (
              <GroupRejoinRequests services={services} copy={groupsCopy} rejoin={groups.rejoin} invites={groups.invites} chats={list.status === "ready" ? list.value : []}
                onChanged={groups.refresh} {...(groups.available ? {} : { onEnable: () => { setGroupsGateOpen("rejoin"); } })} />
            ) : null}
            {groups.available ? <GroupInvites services={services} copy={groupsCopy} invites={groups.invites} chats={list.status === "ready" ? list.value : []} onAnswered={groups.refresh} /> : null}
            {groups.available ? (
              <GroupStatus
                copy={groupsCopy} states={groupEvents.states} scope="all"
                inviteName={(id) => { const i = groups.invites.find((x) => x.id === id); return i === undefined ? null : (list.status === "ready" ? (() => { const c = list.value.find((x) => x.id === i.relationshipId); return c === undefined ? null : shownName(c); })() : null); }}
                groupName={(id) => groups.groups.find((g) => g.conversationId === id)?.name ?? null}
                onDismissUndeliverable={groupEvents.dismissUndeliverable} onDismissSettingBlocked={groupEvents.dismissSettingBlocked} onDismissAdmitBlocked={groupEvents.dismissAdmitBlocked}
              />
            ) : null}
            {incoming?.token != null ? (
              <section className="om-page om-stack">
                <AcceptInvite
                  key={incoming.token} services={services} copy={onboardingCopy.incoming} token={incoming.token}
                  onSending={() => { chatsBeforeAccept.current = list.status === "ready" ? list.value.map((c) => c.id) : []; }}
                  onAccepted={() => { incoming.onAccepted(chatsBeforeAccept.current); }} onDecline={incoming.onDeclined}
                />
              </section>
            ) : null}
            {invite === undefined ? null : (
              <div className="om-page" hidden={reconnectContacts.length === 0}>
                <ReconnectContacts services={services} copy={reconnectCopy} contacts={reconnectContacts} inviteLink={invite.shareLink ?? ((link) => link)} copyText={copyText} onOpen={select} />
              </div>
            )}
            <StrangerPressure services={services} copy={conversationCopy.pressure} />
            <Requests services={services} contacts={requestContacts} />
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
            {calls && list.status === "ready" ? calls.notices.slice(-1).map((notice) => {
              const contact = list.value.find((row) => row.id === notice.conversationId);
              if (!contact) return null;
              return <div className="om-page" key={notice.callSessionId}>
                <Button variant="quiet" onClick={() => { select(contact.id); }}>{callCopy.missedFrom(shownName(contact))}</Button>
              </div>;
            }) : null}
            {chatRows !== null && !emptyInviteFirst ? <ConversationList conversations={chatRows} selectedId={selected} onSelect={select} copy={conversationCopy} digests={digests} drafts={drafts} unread={unread} marks={groupRows.marks} sameNameCopy={groupsCopy.sameName} /> : null}
          </>
        }
        detail={
          selected === null ? <div className="om-page om-page--empty"><span className="om-empty-art" aria-hidden="true"><Icon name="chat" size={72} /></span></div> : selectedIsGroup(selected) ? (
            <GroupChat
              key={selected} services={services} copy={groupsCopy} groupId={asGroupId(selected)} chats={list.status === "ready" ? list.value : []}
              onBack={toChats} onLeft={() => { groups.refresh(); toChats(); }} refreshKey={groupTick} onChanged={groups.refresh} events={groupEvents}
              rejoinReviews={groups.rejoin.reviews.filter((r) => r.conversationId === asGroupId(selected))}
              onReply={(target) => { setReplies((m) => new Map(m).set(selected, target)); }} replyingTo={replies.get(selected)?.messageId ?? null}
              focusMessageId={hitFocus !== null && hitFocus.chat === selected && hitFocus.ctx === services.ctx ? hitFocus.messageId : undefined}
            />
          ) : (
            <ConversationView key={selected} sameName={contactNames.of(selected)} sameNameCopy={groupsCopy.sameName} services={services} conversationId={selected} copy={conversationCopy} recoveryCopy={recoveryCopy.contact} onBack={toChats} expressive={expressive} {...(richTextFor === undefined ? {} : { richTextFor })}
              callActions={calls === undefined ? undefined : (contact) => contact.allowCalls === undefined ? null : (
                <div className="om-call__controls">
                  <IconButton label={callCopy.audio} icon={<Icon name="phone" />} onClick={() => { calls.start(contact, "audio"); }} />
                  <IconButton label={callCopy.video} icon={<Icon name="video" />} onClick={() => { calls.start(contact, "video"); }} />
                </div>
              )}
              callNotices={calls?.notices.some((notice) => notice.conversationId === selected) ? (
                <ul aria-label={callCopy.missedSeveral} className="om-conv-view__timeline">
                  {calls.notices.filter((notice) => notice.conversationId === selected).map((notice, index) => (
                    <CallTimelineNotice key={`${notice.callSessionId}-${String(index)}`} notice={notice} copy={callCopy} />
                  ))}
                </ul>
              ) : undefined}
              onReply={(target) => { setReplies((m) => new Map(m).set(selected, target)); }} replyingTo={replies.get(selected)?.messageId ?? null} focusMessageId={hitFocus !== null && hitFocus.chat === selected && hitFocus.ctx === services.ctx ? hitFocus.messageId : undefined} />
          )
        }
        composerZone={selected !== null && selectedIsGroup(selected) ? (
          <GroupComposer
            key={selected} services={services} copy={groupsCopy} groupId={asGroupId(selected)} draft={drafts.get(selected) ?? ""}
            onDraftChange={(next) => { setDrafts((d) => new Map(d).set(selected, next)); draftSync?.save(selected, next); }}
            onSent={() => { setGroupTick((n) => n + 1); groups.refresh(); }}
            replyTo={replies.get(selected) ?? null} onCancelReply={() => { setReplies((m) => { const n = new Map(m); n.delete(selected); return n; }); }}
            trailing={<OmnibarTrigger variant="icon" copy={omnibarCopy} onOpen={openOmni} />}
          />
        ) : selected === null || (list.status === "ready" && list.value.some((c) => c.id === selected && c.relationship === "restored")) ? undefined : (
          <Composer
            key={selected} services={services} conversationId={selected} copy={conversationCopy} {...(peerName === undefined ? {} : { peerName })} {...(peerAccount === undefined ? {} : { peerAccount })} {...(scheduleControl === undefined ? {} : { schedule: scheduleControl })} draft={drafts.get(selected) ?? ""}
            onDraftChange={(next) => { setDrafts((d) => new Map(d).set(selected, next)); draftSync?.save(selected, next); }}
            {...(capture === undefined ? {} : { capture })} staged={staged.get(selected) ?? NO_STAGED}
            onStagedChange={(next) => { setStaged((m) => new Map(m).set(selected, next)); }}
            replyTo={replies.get(selected) ?? null} onCancelReply={() => { setReplies((m) => { const n = new Map(m); n.delete(selected); return n; }); }}
            trailing={<>
              {expressive !== undefined && list.status === "ready" && list.value.some((c) => c.id === selected && c.relationship === "active")
                ? <ExpressivePickerControl services={services} surface={expressive} conversationId={selected} /> : null}
              <OmnibarTrigger variant="icon" copy={omnibarCopy} onOpen={openOmni} />
            </>}
          />
        )}
      />
      {inviteScan.dialog}
      {newChatSheet}
      {newGroupOpen && list.status === "ready" ? (
        <Sheet title={groupsCopy.newGroup.action} closeLabel={groupsCopy.newGroup.close} onClose={() => { setNewGroupOpen(false); }}>
          <NewGroup services={services} copy={groupsCopy} chats={list.value} onCreated={() => { groups.refresh(); setNewGroupOpen(false); }} />
        </Sheet>
      ) : null}
      {groupsGateOpen !== null && groupsSetup !== undefined ? <GroupsGate setup={groupsSetup} copy={groupsLoadCopy} newGroup={groupsGateOpen === "new_group"} onClose={() => { setGroupsGateOpen(null); }} /> : null}
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

function Requests({ services, contacts }: { readonly services: ConversationServices; readonly contacts: readonly { readonly id: string; readonly name: string }[] }) {
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
  const collisionOf = introductionCollisions(contacts, shown);
  const hide = (id: string): void => { setGone((g) => new Set(g).add(id)); };
  return (
    <section className="om-page om-stack" aria-label={t.requestsHeading}>
      {shown.map((r) => (
        <div key={r.id} className="om-card"><MessageRequest services={services} request={r} copy={conversationCopy} sameName={collisionOf(r.id)} sameNameCopy={groupsCopy.sameName} onIgnore={hide} /></div>
      ))}
    </section>
  );
}

interface MenuRow { readonly id: SettingsPage; readonly title: string; readonly hint: string; readonly icon: IconName; readonly words: string }

/** Settings is reached from the profile area (Profile -> Settings), not a tab: Chats stays the compact root. Typing anywhere searches it. */
function SettingsMenu({ page, onOpen, onAddProfile, hasHome, hasLock, hasBackup, hasAlerts, onSafetyCode }: {
  readonly page: SettingsPage; readonly onOpen: (p: SettingsPage) => void; readonly onAddProfile: (() => void) | undefined; readonly hasHome: boolean; readonly hasLock: boolean; readonly hasBackup: boolean; readonly hasAlerts: boolean; readonly onSafetyCode: (() => void) | undefined;
}) {
  const [query, setQuery] = useState("");
  const search = useRef<HTMLInputElement>(null);
  const rows: readonly MenuRow[] = [
    ...(hasLock ? [{ id: "lock" as const, title: t.settingsLock, hint: t.settingsRows.lock, icon: "lock" as const, words: t.settingsKeywords.lock }] : []),
    { id: "devices", title: t.settingsDevices, hint: t.settingsRows.devices, icon: "devices", words: t.settingsKeywords.devices },
    { id: "recovery", title: t.settingsRecovery, hint: t.settingsRows.recovery, icon: "shield", words: t.settingsKeywords.recovery },
    { id: "protection" as const, title: t.settingsProtection, hint: t.settingsRows.protection, icon: "shield" as const, words: t.settingsKeywords.protection },
    ...(hasBackup ? [{ id: "backup" as const, title: t.settingsBackup, hint: t.settingsRows.backup, icon: "download" as const, words: t.settingsKeywords.backup }] : []),
    ...(hasAlerts ? [{ id: "alerts" as const, title: t.settingsAlerts, hint: t.settingsRows.alerts, icon: "chat" as const, words: t.settingsKeywords.alerts }] : []),
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
