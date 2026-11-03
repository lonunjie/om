// First-run delivery-service setup over the real configuration (ADR-0186): no built-in service exists, so a new install asks
// the person to set up their own. This adapts @om/provider-http to the page (`RelaySetup`): the owner's admission secret is made
// on this device before the address is known (the setup asks for its public hash), the typed address is checked like every
// other source (providerConfig.ts) and asked who it is, and only then are address and secret stored together. The setup link is
// the only place this app sends a person to a vendor, and only because they chose it on a page that names it.
import { adoptAdmission, checkProviderUrl, createIndexedDbSecretStore, pendingAdmissionHash, probeHome } from "@om/provider-http";
import type { RelaySetupServices } from "@om/ui";
import { saveRelayUrl, type ProviderEnv } from "./providerConfig";

export interface RelaySetupInput {
  readonly env: ProviderEnv;
  /** `import.meta.env.VITE_OM_RELAY_DEPLOY_URL`: the one-click setup. Anything that is not an https address is ignored. */
  readonly deployUrl?: string | undefined;
  /** Restarts the connection with whatever is saved. */
  readonly reconnect: () => void;
}

/** True once the person has set up a delivery service on this page load: first run then continues past the landing page they already saw. */
let connectedThisLoad = false;
export const relayJustConnected = (): boolean => connectedThisLoad;

const httpsOnly = (url: string | undefined): string | undefined => {
  if (url === undefined || url === "") return undefined;
  try { const u = new URL(url); return u.protocol === "https:" && u.username === "" && u.password === "" ? u.href : undefined; } catch { return undefined; }
};

export function createRelaySetup({ env, deployUrl, reconnect }: RelaySetupInput): RelaySetupServices {
  const loopback = env.allowLoopbackHttp === true;
  const secrets = createIndexedDbSecretStore(env.indexedDB);
  const link = httpsOnly(deployUrl);
  return {
    deployUrl: link,
    prepare: () => pendingAdmissionHash(secrets),
    async probe(input) {
      const url = checkProviderUrl(input, { allowLoopbackHttp: loopback });
      if (!url.ok) return { kind: "invalid_url", reason: url.reason };
      return probeHome(url.config.baseUrl, env.fetch === undefined ? {} : { fetch: env.fetch });
    },
    async connect(baseUrl) {
      try {
        const url = checkProviderUrl(baseUrl, { allowLoopbackHttp: loopback });
        if (!url.ok) return false;
        if (!(await adoptAdmission(secrets, url.config.baseUrl))) return false;
        if (!(await saveRelayUrl(env, url.config.baseUrl))) return false;
        connectedThisLoad = true;
        return true;
      } catch { return false; }
    },
    done: reconnect,
  };
}
