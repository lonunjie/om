// RealApp with its provider resolved from configuration (see providerConfig.ts). Until the choice is known nothing starts,
// so the Core is never opened first against "no provider" and then switched. No provider configured: RealApp gets `null`
// and shows the "Not connected to a provider" state.
import { useEffect, useState, type ReactNode } from "react";
import { browserVaultEnv } from "@om/web-platform";
import { Text } from "@om/ui";
import { appStrings as t } from "../copy/en";
import { createConfiguredProvider } from "./providerConfig";
import { RealApp } from "./RealApp";
import type { ConfiguredProvider } from "./provider";

type Resolved = { readonly k: "pending" } | { readonly k: "ready"; readonly provider: ConfiguredProvider };

export function ConfiguredApp({ banner, pollMs }: { readonly banner?: ReactNode | undefined; readonly pollMs?: number | undefined }) {
  const [state, setState] = useState<Resolved>({ k: "pending" });
  useEffect(() => {
    const life = { live: true };
    void (async () => {
      const env = browserVaultEnv("OM");
      let provider: ConfiguredProvider = null;
      if (env !== undefined) {
        try {
          provider = await createConfiguredProvider({
            indexedDB: env.indexedDB,
            buildUrl: import.meta.env.VITE_OM_RELAY_URL,
            allowLoopbackHttp: import.meta.env.VITE_OM_ALLOW_LOOPBACK_RELAY === "1",
          });
        } catch { provider = null; } // an unreadable config store means "not configured", never a guess
      }
      if (life.live) setState({ k: "ready", provider });
    })();
    return () => { life.live = false; };
  }, []);
  if (state.k === "pending") return <Text>{t.coreLoading}</Text>;
  return <RealApp provider={state.provider} {...(banner === undefined ? {} : { banner })} {...(pollMs === undefined ? {} : { pollMs })} />;
}
