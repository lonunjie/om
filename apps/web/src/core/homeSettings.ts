// The Settings > Your Home controls over the real configuration: validate the typed address, ask it who it is, store the
// choice, report mailboxes this install lost. A choice is only ever made from text the user typed on this screen (there
// is no link, query string or deep link that reaches saveRelayUrl), and only for an address that passed the same checks
// as every other source (providerConfig.ts).
import { checkProviderUrl, probeHome, type HttpProvider } from "@om/provider-http";
import type { HomeSettingsServices } from "@om/ui";
import { hasBuildDefault, saveRelayUrl, type ProviderChoice, type ProviderEnv } from "./providerConfig";

export interface HomeSettingsInput {
  readonly env: ProviderEnv;
  readonly choice: ProviderChoice | null;
  readonly provider: HttpProvider | null;
  /** Restarts the connection with whatever is saved (the host asks the user to unlock again). */
  readonly reconnect: () => void;
}

export function createHomeSettings({ env, choice, provider, reconnect }: HomeSettingsInput): HomeSettingsServices {
  const loopback = env.allowLoopbackHttp === true;
  return {
    current: { baseUrl: choice?.config.baseUrl, source: choice === null ? "none" : choice.source, hasDefault: hasBuildDefault(env) },
    async probe(input) {
      const url = checkProviderUrl(input, { allowLoopbackHttp: loopback });
      if (!url.ok) return { kind: "invalid_url", reason: url.reason };
      return probeHome(url.config.baseUrl, env.fetch === undefined ? {} : { fetch: env.fetch });
    },
    save: (baseUrl) => saveRelayUrl(env, baseUrl).catch(() => false),
    clear: () => saveRelayUrl(env, undefined).catch(() => false),
    lostMailboxes: () => provider?.lostMailboxes() ?? [],
    reconnect,
  };
}
