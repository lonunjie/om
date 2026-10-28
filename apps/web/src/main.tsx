import { StrictMode, useState } from "react";
import { createRoot } from "react-dom/client";
import { App } from "@om/ui";
import { appStrings, shellLabels } from "./copy/en";
import { InstallAffordance } from "./install/InstallAffordance";
import { Landing } from "./landing/Landing";
import { registerServiceWorker } from "./sw/register";
import "./landing/landing.css";

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");

// This build has no Core, so no device can have an identity yet: show the landing page first, then the shell.
function Production() {
  const [started, setStarted] = useState(false);
  if (!started) return <Landing onGetStarted={() => { setStarted(true); }} installSlot={<InstallAffordance />} />;
  return <App copy={{ labels: shellLabels, ...appStrings }} />;
}

// Statically dead in a plain production build (DEV is false, the flag is unset), so the harness is never bundled.
if (import.meta.env.DEV || import.meta.env.VITE_OM_DEV_HARNESS === "1") {
  void import("./dev/main.dev").then((m) => { m.mountDev(root); });
} else {
  void registerServiceWorker();
  createRoot(root).render(
    <StrictMode>
      <Production />
    </StrictMode>,
  );
}
