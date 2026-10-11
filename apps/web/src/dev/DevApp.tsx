// Walking skeleton: onboarding, then the shell, composed against the dev-only Core harness. Copy comes from ../copy/en.
import { useEffect, useMemo, useState } from "react";
import type { ConversationId, DeviceVault, IntroductionRequest, ProfileExecutionContext, ProfileId } from "@om/platform-api";
import { HARNESS_MARKER, type Harness } from "@om/core-harness";
import {
  AppShell, Button, Composer, ConnectedProfileSwitcher, ConversationList, ConversationView, LiveRegion, MessageRequest, OnboardingFlow,
  ProfileProvider, Text, ThemeProvider, createActionRegistry, createMemoryKeyValue, createProfileScopedState, useActiveProfile, useCaptureContext, useConversationList,
  type ActionRegistry, type ConversationServices, type OnboardingPlatform,
} from "@om/ui";
import { appStrings as t, conversationCopy, onboardingCopy, shellLabels, switcherLabels } from "../copy/en";
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
      <div className="om-dev-banner" role="note">DEV HARNESS — NOT PRODUCTION ({HARNESS_MARKER})</div>
      <ProfileProvider client={harness} contexts={harness} store={store} initialProfileId={pendingProfileId}>
        <Gate harness={harness} registry={registry} vault={vault} platform={platform} initiallyOnboarded={onboarded} />
      </ProfileProvider>
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
  if (activeId === null || ctx === null) return <Text>{t.loading}</Text>;
  if (!done.has(activeId)) {
    return (
      <OnboardingFlow
        key={activeId} client={harness} registry={registry} ctx={ctx} vault={vault} platform={platform} copy={onboardingCopy}
        onDone={() => { setDone((d) => new Set(d).add(activeId)); }}
      />
    );
  }
  return <Shell key={activeId} services={{ client: harness, registry, ctx }} />;
}

function Shell({ services }: { readonly services: ConversationServices }) {
  const [selected, setSelected] = useState<ConversationId | null>(null);
  const [drafts, setDrafts] = useState<ReadonlyMap<ConversationId, string>>(() => new Map());
  const list = useConversationList(services);
  const select = (id: ConversationId): void => { setSelected(id); };
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
            <Requests services={services} />
            {list.status === "loading" ? <Text>{t.loading}</Text> : null}
            {list.status === "failed" ? <Text>{t.loadFailed}</Text> : null}
            {list.status === "ready" ? <ConversationList conversations={list.value} selectedId={selected} onSelect={select} copy={conversationCopy} /> : null}
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
        profileControl={<ConnectedProfileSwitcher labels={switcherLabels} />}
      />
      <LiveRegion>{selected === null ? "" : t.conversationOpened}</LiveRegion>
    </>
  );
}

function Requests({ services }: { readonly services: ConversationServices }) {
  const [items, setItems] = useState<readonly IntroductionRequest[]>([]);
  const [gone, setGone] = useState<ReadonlySet<string>>(() => new Set());
  useEffect(() => {
    let live = true;
    services.client.query({ kind: "GetIntroductions", ctx: services.ctx }).then((r) => { if (live) setItems(r); }, () => { if (live) setItems([]); });
    return () => { live = false; };
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
