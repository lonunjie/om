// The Settings > Message delivery controls over the real configuration: validate the typed address, ask it who it is, store the
// choice, report mailboxes this install lost. A choice is only ever made from text the user typed on this screen (there
// is no link, query string or deep link that reaches saveRelayUrl or addBackupHome), and only for an address that passed the same checks
// as every other source (providerConfig.ts).
// Advanced: more than one delivery service. Adding a backup reuses the first-run setup (a setup secret made on this device, bound to the typed
// address once it answers as a delivery service that asks for it). Moving away calls Core's `retire_home` for every unlocked identity and then
// records the service as leaving, so the next connection passes it to the provider as "draining" and Core does not add a route there again.
import { adoptAdmission, checkProviderUrl, createIndexedDbSecretStore, pendingAdmissionHash, probeHome, readAdmission, type HttpProvider } from "@om/provider-http";
import type { DeliveryServiceItem, HomeSettingsServices, MoreHomesServices } from "@om/ui";
import { MAX_HOMES, addBackupHome, hasBuildDefault, markHomeLeaving, resolveHomeSet, saveRelayUrl, type HomeSet, type ProviderChoice, type ProviderEnv } from "./providerConfig";
import { httpsOnly } from "./relaySetup";

/** Core's `retire_home` for the route name of one service, over every unlocked identity: how many chats had no other service and so stay on it. */
export type RetireHome = (relayUri: string) => Promise<number>;

export interface HomeSettingsInput {
  readonly env: ProviderEnv;
  readonly choice: ProviderChoice | null;
  /** Every configured service. Left out, the main one in `choice` is the only one. */
  readonly set?: HomeSet | null | undefined;
  /** The provider in use; when it can stop offering a service at once (`leave`), moving away does that before it asks Core. */
  readonly provider: (HttpProvider & { readonly leave?: (baseUrl: string) => () => void }) | null;
  /** `import.meta.env.VITE_OM_RELAY_DEPLOY_URL`: the one-click setup, offered again when adding a backup. Anything that is not an https address is ignored. */
  readonly deployUrl?: string | undefined;
  /** Restarts the connection with whatever is saved (the host asks the user to unlock again). */
  readonly reconnect: () => void;
}

/** The services the screen uses, plus the way to give "move away" its Core once an identity is open. */
export type HomeHost = HomeSettingsServices & { readonly bindCore?: (retire: RetireHome) => HomeSettingsServices };

export function createHomeSettings({ env, choice, set, provider, deployUrl, reconnect }: HomeSettingsInput): HomeHost {
  const loopback = env.allowLoopbackHttp === true;
  const secrets = createIndexedDbSecretStore(env.indexedDB);
  const homes = set ?? (choice === null ? null : { source: choice.source, homes: [{ config: choice.config, leaving: false }] } satisfies HomeSet);

  const more = (retire: RetireHome | undefined): MoreHomesServices | undefined => {
    if (homes === null) return undefined;
    const services: DeliveryServiceItem[] = homes.homes.map((h, i) => ({ baseUrl: h.config.baseUrl, role: h.leaving ? "leaving" : i === 0 ? "main" : "backup" }));
    return {
      services,
      setup: { deployUrl: httpsOnly(deployUrl), prepare: () => pendingAdmissionHash(secrets) },
      async add(input) {
        const url = checkProviderUrl(input, { allowLoopbackHttp: loopback });
        if (!url.ok) return "failed";
        const base = url.config.baseUrl;
        if (homes.homes.some((h) => h.config.baseUrl === base)) return "already";
        if (homes.homes.length >= MAX_HOMES) return "full";
        try {
          // A secret already bound to this address (an earlier attempt that stored nothing else) is kept: the pending one was used up making it.
          if ((await readAdmission(secrets, base)) === undefined && !(await adoptAdmission(secrets, base))) return "failed";
          return await addBackupHome(env, base);
        } catch { return "failed"; }
      },
      async move(baseUrl) {
        // The stored list, not the one this screen started with: a service may have been left a moment ago, before the next connection.
        const now = await resolveHomeSet(env).catch(() => null) ?? homes;
        const target = now.homes.find((h) => h.config.baseUrl === baseUrl);
        if (target === undefined || retire === undefined) return { kind: "failed" };
        if (!target.leaving && !now.homes.some((h) => !h.leaving && h.config.baseUrl !== baseUrl)) return { kind: "last" };
        const undo = provider?.leave?.(baseUrl);
        try {
          const stuck = await retire(`${baseUrl}/`);
          // Core has acted. If this cannot be recorded, saying so lets the person repeat it: retiring again is harmless.
          if (!(await markHomeLeaving(env, baseUrl))) { undo?.(); return { kind: "failed" }; }
          return { kind: "moved", stuck };
        } catch { undo?.(); return { kind: "failed" }; }
      },
    };
  };

  const build = (retire: RetireHome | undefined): HomeSettingsServices => {
    const m = more(retire);
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
      ...(m === undefined ? {} : { more: m }),
    };
  };
  return { ...build(undefined), bindCore: (retire) => build(retire) };
}
