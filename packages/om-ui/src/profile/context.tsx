// ProfileProvider: the active-profile selection is local UI state. Work never reads it after capture (ADR-0149):
// callers capture an immutable ProfileExecutionContext at dispatch and bind the intent to it.
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { captureProfileContext } from "@om/platform-api";
import type { CoreClient, CoreIntent, OperationHandle, ProfileExecutionContext, ProfileId, ProfileSummary } from "@om/platform-api";
import type { ProfileScopedState } from "./scoped-state";

/** Supplies the current context (profile + generation) for a profile id; the platform/Core owns generations. */
export interface ProfileContextSource { contextFor(profileId: ProfileId): ProfileExecutionContext }

export type ContextCapture = () => ProfileExecutionContext;

interface ProfileValue {
  readonly client: CoreClient;
  readonly store: ProfileScopedState;
  readonly profiles: readonly ProfileSummary[];
  readonly activeProfileId: ProfileId | null;
  readonly switchTo: (profileId: ProfileId) => boolean;
  captureContext: ContextCapture;
}
const Ctx = createContext<ProfileValue | null>(null);

function useValue(): ProfileValue {
  const v = useContext(Ctx);
  if (!v) throw new Error("ProfileProvider is missing");
  return v;
}

export interface ProfileProviderProps {
  readonly client: CoreClient;
  readonly contexts: ProfileContextSource;
  readonly store: ProfileScopedState;
  readonly initialProfileId?: ProfileId;
  readonly children: ReactNode;
}

export function ProfileProvider({ client, contexts, store, initialProfileId, children }: ProfileProviderProps) {
  const [profiles, setProfiles] = useState<readonly ProfileSummary[]>([]);
  const [active, setActive] = useState<ProfileId | null>(initialProfileId ?? null);
  const activeRef = useRef<ProfileId | null>(active);
  const profilesRef = useRef<readonly ProfileSummary[]>(profiles);

  const select = useCallback((id: ProfileId | null) => { activeRef.current = id; setActive(id); }, []);
  const apply = useCallback((list: readonly ProfileSummary[]) => {
    profilesRef.current = list;
    setProfiles(list);
    const cur = activeRef.current;
    if (cur === null || !list.some((p) => p.id === cur)) select(list[0]?.id ?? null); // fail closed if the active profile vanished
  }, [select]);

  useEffect(() => {
    let live = true;
    const unsub = client.subscribe({ kind: "GetProfileList" }, (l) => { if (live) apply(l); });
    void client.query({ kind: "GetProfileList" }).then((l) => { if (live) apply(l); }, () => { if (live) apply([]); });
    return () => { live = false; unsub(); };
  }, [client, apply]);

  const switchTo = useCallback((id: ProfileId): boolean => {
    if (!profilesRef.current.some((p) => p.id === id)) return false;
    select(id);
    return true;
  }, [select]);

  const captureContext = useCallback((): ProfileExecutionContext => {
    const id = activeRef.current;
    if (id === null) throw new Error("no active profile");
    const c = contexts.contextFor(id);
    return captureProfileContext(c.profileId, c.generation);
  }, [contexts]);

  const value = useMemo<ProfileValue>(
    () => ({ client, store, profiles, activeProfileId: active, switchTo, captureContext }),
    [client, store, profiles, active, switchTo, captureContext],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export const useProfiles = (): { readonly profiles: readonly ProfileSummary[]; readonly switchTo: (id: ProfileId) => boolean } => {
  const { profiles, switchTo } = useValue();
  return { profiles, switchTo };
};
export const useActiveProfile = (): ProfileSummary | null => {
  const { profiles, activeProfileId } = useValue();
  return profiles.find((p) => p.id === activeProfileId) ?? null;
};
/** The frozen context of the active profile at call time. Pass it into intents; never cache it across a switch. */
export const useProfileContext = (): ProfileExecutionContext => useValue().captureContext();
export const useCaptureContext = (): ContextCapture => useValue().captureContext;
export const useScopedState = (): ProfileScopedState => useValue().store;

/** Captures the context synchronously, builds the intent from it, and dispatches. Later switches cannot retarget it. */
export async function dispatchWithCapturedContext(
  client: CoreClient,
  capture: ContextCapture,
  intentFactory: (ctx: ProfileExecutionContext) => CoreIntent,
): Promise<{ readonly ctx: ProfileExecutionContext; readonly handle: OperationHandle }> {
  const ctx = capture();
  const intent = intentFactory(ctx);
  if (intent.ctx !== ctx) throw new Error("intent is not bound to the captured profile context");
  return { ctx, handle: await client.dispatch(intent) };
}

/** Hook form: dispatch(intentFactory) with capture bound to the provider. */
export function useDispatch(): (intentFactory: (ctx: ProfileExecutionContext) => CoreIntent) => ReturnType<typeof dispatchWithCapturedContext> {
  const { client, captureContext } = useValue();
  return useCallback((f) => dispatchWithCapturedContext(client, captureContext, f), [client, captureContext]);
}
