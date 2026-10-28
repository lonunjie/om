import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { ConfiguredApp } from "./core/ConfiguredApp";
import { registerServiceWorker } from "./sw/register";
import "./landing/landing.css";

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");

// Production: the real Core over the device vault, connected to the Home named by configuration (a stored choice, else the
// build default VITE_OM_RELAY_URL). With none configured the app says so ("Not connected to a provider") and queues
// locally; it never fakes a provider. The landing page opens the first run.
// Statically dead in a plain production build (DEV is false, the flags are unset), so the harness and the e2e
// loopback are never bundled.
if (import.meta.env.DEV || import.meta.env.VITE_OM_DEV_HARNESS === "1") {
  void import("./dev/main.dev").then((m) => { m.mountDev(root); });
} else if (import.meta.env.VITE_OM_E2E_CORE === "1") {
  void import("./dev/main.e2e-core").then((m) => { m.mountE2eCore(root); });
} else {
  void registerServiceWorker();
  createRoot(root).render(
    <StrictMode>
      <ConfiguredApp banner={import.meta.env.VITE_OM_ALLOW_LOOPBACK_RELAY === "1" ? <div className="om-dev-banner" role="note">E2E LOCAL RELAY, NOT PRODUCTION (OM-E2E-LOOPBACK-RELAY-NOT-FOR-PRODUCTION).</div> : undefined} pollMs={import.meta.env.VITE_OM_ALLOW_LOOPBACK_RELAY === "1" ? 400 : undefined} />
    </StrictMode>,
  );
}
