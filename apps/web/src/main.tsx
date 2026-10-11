import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "@om/ui";
import { appStrings, shellLabels } from "./copy/en";
import { registerServiceWorker } from "./sw/register";

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");

// Statically dead in a plain production build (DEV is false, the flag is unset), so the harness is never bundled.
if (import.meta.env.DEV || import.meta.env.VITE_OM_DEV_HARNESS === "1") {
  void import("./dev/main.dev").then((m) => { m.mountDev(root); });
} else {
  void registerServiceWorker();
  createRoot(root).render(
    <StrictMode>
      <App copy={{ labels: shellLabels, ...appStrings }} />
    </StrictMode>,
  );
}
