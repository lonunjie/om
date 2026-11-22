// E2E ONLY. Mounts the real Core with an in-page loopback provider so a browser test can drive two identities against
// each other without a relay. Imported dynamically from main.tsx behind a statically dead branch; the production
// bundle check refuses the loopback marker and the test-support module, so this can never ship.
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createDirectFaults, createMemoryProvider, TEST_PROVIDER_MARKER } from "@om/core-wasm/test-support";
import type { DirectTransport, WasmCoreClient } from "@om/core-wasm";
import type { DirectFaults } from "@om/core-wasm/test-support";
import { RealApp } from "../core/RealApp";
import { appStrings } from "../copy/en";
import "./dev.css";

export function mountE2eCore(root: HTMLElement): void {
  if (import.meta.env.PROD && import.meta.env.VITE_OM_E2E_CORE !== "1") throw new Error("e2e loopback refused: production build");
  const provider = createMemoryProvider();
  // Lets the browser test assert what a relay would have seen (ciphertext only); absent from every production bundle.
  // `direct` is the e2e stand-in for the byte channel between two devices (D120: no product transport exists): an in-page loopback, the
  // running Core, and the ciphertext the receiver kept, so a spec can drive a direct transfer the way two devices would.
  const faults = createDirectFaults();
  const transport: DirectTransport = {
    connect: (ctx, target) => faults.transportFor(ctx.profileId).connect(ctx, target),
    listen: (ctx, onChannel) => faults.transportFor(ctx.profileId).listen(ctx, onChannel),
  };
  const kept: { profile: string; messageId: string; length: number }[] = [];
  const direct: { rt?: WasmCoreClient; faults: DirectFaults; kept: typeof kept; profiles: string[]; setBlobLimit: (maxBlobBytes: number) => void } = {
    faults, kept, profiles: [],
    setBlobLimit: (maxBlobBytes) => { provider.setBlobLimits({ maxBlobBytes, maxSpoolBytes: 32 * 1024 * 1024, maxBlobs: 64 }); },
  };
  (globalThis as unknown as { __OM_E2E_LOOPBACK__: unknown }).__OM_E2E_LOOPBACK__ = { deposited: () => provider.deposited(), direct };
  const onCore = (rt: WasmCoreClient): void => {
    direct.rt = rt;
    const open = rt.openProfile.bind(rt);
    rt.openProfile = async (id) => { if (!direct.profiles.includes(id)) direct.profiles.push(id); return open(id); };
  };
  createRoot(root).render(
    <StrictMode>
      <RealApp
        provider={provider}
        pollMs={400}
        directDriver={{ transport, cache: { put: (profile, messageId, _i, ciphertext) => { kept.push({ profile, messageId, length: ciphertext.length }); return Promise.resolve(); } }, retryMs: 0 }}
        onCore={onCore}
        banner={<div className="om-dev-banner" role="note">E2E LOOPBACK PROVIDER, NOT PRODUCTION ({TEST_PROVIDER_MARKER}). {appStrings.providerStatus.loopback}</div>}
      />
    </StrictMode>,
  );
}
