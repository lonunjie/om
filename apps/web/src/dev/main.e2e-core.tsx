// E2E ONLY. Mounts the real Core with an in-page loopback provider so a browser test can drive two identities against
// each other without a relay. Imported dynamically from main.tsx behind a statically dead branch; the production
// bundle check refuses the loopback marker and the test-support module, so this can never ship.
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createMemoryProvider, TEST_PROVIDER_MARKER } from "@om/core-wasm/test-support";
import { RealApp } from "../core/RealApp";
import { appStrings } from "../copy/en";
import "./dev.css";

export function mountE2eCore(root: HTMLElement): void {
  if (import.meta.env.PROD && import.meta.env.VITE_OM_E2E_CORE !== "1") throw new Error("e2e loopback refused: production build");
  const provider = createMemoryProvider();
  // Lets the browser test assert what a relay would have seen (ciphertext only); absent from every production bundle.
  (globalThis as unknown as { __OM_E2E_LOOPBACK__: { deposited: () => readonly string[] } }).__OM_E2E_LOOPBACK__ = { deposited: () => provider.deposited() };
  createRoot(root).render(
    <StrictMode>
      <RealApp
        provider={provider}
        pollMs={400}
        banner={<div className="om-dev-banner" role="note">E2E LOOPBACK PROVIDER, NOT PRODUCTION ({TEST_PROVIDER_MARKER}). {appStrings.providerStatus.loopback}</div>}
      />
    </StrictMode>,
  );
}
