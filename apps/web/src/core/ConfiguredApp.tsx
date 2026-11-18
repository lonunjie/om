// RealApp with its provider resolved from configuration (see providerConfig.ts). Until the choice is known nothing starts,
// so the Core is never opened first against "no provider" and then switched. No provider configured: RealApp gets `null`
// and shows the "Not connected to a provider" state. Settings > Your Home saves a choice; `reconnect` re-resolves it,
// which restarts the Core against the new Home (the identities are locked again, so the user unlocks once more).
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { browserVaultEnv } from "@om/web-platform";
import { Text } from "@om/ui";
import { appStrings as t } from "../copy/en";
import { createHomeSettings } from "./homeSettings";
import { createHomesProviderFor, resolveHomeSet, type HomeSet, type HomesProvider, type ProviderChoice, type ProviderEnv } from "./providerConfig";
import { RealApp } from "./RealApp";
import { createRelaySetup } from "./relaySetup";
import { browserAlerts, retireBrowserPush } from "./alertsHost";

type Resolved = { readonly k: "pending" } | { readonly k: "ready"; readonly provider: HomesProvider | null; readonly choice: ProviderChoice | null; readonly set: HomeSet | null; readonly env: ProviderEnv | undefined };

export function ConfiguredApp({ banner, pollMs }: { readonly banner?: ReactNode | undefined; readonly pollMs?: number | undefined }) {
  const [state, setState] = useState<Resolved>({ k: "pending" });
  const previousProvider = useRef<HomesProvider | null>(null);
  const [generation, setGeneration] = useState(0);
  const reconnect = useCallback(() => { setState({ k: "pending" }); setGeneration((g) => g + 1); }, []);
  useEffect(() => {
    const life = { live: true };
    void (async () => {
      const vault = browserVaultEnv("OM");
      let provider: HomesProvider | null = null;
      let choice: ProviderChoice | null = null;
      let set: HomeSet | null = null;
      let env: ProviderEnv | undefined;
      if (vault !== undefined) {
        env = {
          indexedDB: vault.indexedDB,
          buildUrl: import.meta.env.VITE_OM_RELAY_URL,
          allowLoopbackHttp: import.meta.env.VITE_OM_ALLOW_LOOPBACK_RELAY === "1",
        };
        try {
          set = await resolveHomeSet(env);
          const main = set?.homes[0];
          choice = set === null || main === undefined ? null : { config: main.config, source: set.source };
          provider = set === null ? null : await createHomesProviderFor(env, set);
        } catch { provider = null; choice = null; set = null; } // an unreadable config store means "not configured", never a guess
      }
      if (life.live) {
        const previous = previousProvider.current;
        if (previous !== null && previous.baseUrl !== provider?.baseUrl) void retireBrowserPush(previous.push);
        previousProvider.current = provider;
        setState({ k: "ready", provider, choice, set, env });
      }
    })();
    return () => { life.live = false; };
  }, [generation]);
  // A replaced or unmounted provider closes its wake sockets and stops listening to the page.
  useEffect(() => () => { if (state.k === "ready") state.provider?.dispose(); }, [state]);
  const home = useMemo(
    () => (state.k === "ready" && state.env !== undefined ? createHomeSettings({ env: state.env, choice: state.choice, set: state.set, provider: state.provider, deployUrl: import.meta.env.VITE_OM_RELAY_DEPLOY_URL, reconnect }) : undefined),
    [state, reconnect],
  );
  const relaySetup = useMemo(
    () => (state.k === "ready" && state.env !== undefined ? createRelaySetup({ env: state.env, deployUrl: import.meta.env.VITE_OM_RELAY_DEPLOY_URL, reconnect }) : undefined),
    [state, reconnect],
  );
  // A push told the page to look (the service worker's opaque wake, ADR-0210): collect now, as a wake from the Home would.
  useEffect(() => {
    if (state.k !== "ready" || state.provider === null || typeof navigator === "undefined" || !("serviceWorker" in navigator)) return;
    const p = state.provider;
    const on = (e: MessageEvent): void => { if ((e.data as { type?: unknown } | null)?.type === "om.sw/wake") p.nudge(); };
    navigator.serviceWorker.addEventListener("message", on);
    return () => { navigator.serviceWorker.removeEventListener("message", on); };
  }, [state]);
  const alerts = useMemo(() => {
    if (state.k !== "ready") return undefined;
    const p = state.provider;
    return (profileCount: () => number) => browserAlerts(p, profileCount);
  }, [state]);
  if (state.k === "pending") return <Text>{t.coreLoading}</Text>;
  return <RealApp provider={state.provider} {...(banner === undefined ? {} : { banner })} {...(pollMs === undefined ? {} : { pollMs })} {...(home === undefined ? {} : { home })} {...(alerts === undefined ? {} : { alerts })} {...(relaySetup === undefined ? {} : { relaySetup })} />;
}
