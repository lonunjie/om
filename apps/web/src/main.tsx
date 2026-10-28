import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { RealApp } from "./core/RealApp";
import { registerServiceWorker } from "./sw/register";
import "./landing/landing.css";

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");

// Production: the real Core over the device vault. No provider adapter exists yet, so none is configured and the app
// says so (RealApp shows "not connected to a provider"); it never fakes one. The landing page opens the first run.
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
      <RealApp provider={null} />
    </StrictMode>,
  );
}
