// RealApp with its provider resolved from configuration (see providerConfig.ts). Until the choice is known nothing starts,
// so the Core is never opened first against "no provider" and then switched. No provider configured: RealApp gets `null`
// and shows the "Not connected to a provider" state. Settings > Your Home saves a choice; `reconnect` re-resolves it,
// which restarts the Core against the new Home (the identities are locked again, so the user unlocks once more).
import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { browserVaultEnv } from "@om/web-platform";
import type { HttpProvider } from "@om/provider-http";
import { Text } from "@om/ui";
import { appStrings as t } from "../copy/en";
import { createHomeSettings } from "./homeSettings";
import { createHttpProviderFor, resolveProviderChoice, type ProviderChoice, type ProviderEnv } from "./providerConfig";
import { RealApp } from "./RealApp";

type Resolved = { readonly k: "pending" } | { readonly k: "ready"; readonly provider: HttpProvider | null; readonly choice: ProviderChoice | null; readonly env: ProviderEnv | undefined };

export function ConfiguredApp({ banner, pollMs }: { readonly banner?: ReactNode | undefined; readonly pollMs?: number | undefined }) {
  const [state, setState] = useState<Resolved>({ k: "pending" });
  const [generation, setGeneration] = useState(0);
  const reconnect = useCallback(() => { setState({ k: "pending" }); setGeneration((g) => g + 1); }, []);
  useEffect(() => {
    const life = { live: true };
    void (async () => {
      const vault = browserVaultEnv("OM");
      let provider: HttpProvider | null = null;
      let choice: ProviderChoice | null = null;
      let env: ProviderEnv | undefined;
      if (vault !== undefined) {
        env = {
          indexedDB: vault.indexedDB,
          buildUrl: import.meta.env.VITE_OM_RELAY_URL,
          allowLoopbackHttp: import.meta.env.VITE_OM_ALLOW_LOOPBACK_RELAY === "1",
        };
        try {
          choice = await resolveProviderChoice(env);
          provider = choice === null ? null : createHttpProviderFor(env, choice.config);
        } catch { provider = null; choice = null; } // an unreadable config store means "not configured", never a guess
      }
      if (life.live) setState({ k: "ready", provider, choice, env });
    })();
    return () => { life.live = false; };
  }, [generation]);
  const home = useMemo(
    () => (state.k === "ready" && state.env !== undefined ? createHomeSettings({ env: state.env, choice: state.choice, provider: state.provider, reconnect }) : undefined),
    [state, reconnect],
  );
  if (state.k === "pending") return <Text>{t.coreLoading}</Text>;
  return <RealApp provider={state.provider} {...(banner === undefined ? {} : { banner })} {...(pollMs === undefined ? {} : { pollMs })} {...(home === undefined ? {} : { home })} />;
}
