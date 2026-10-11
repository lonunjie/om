import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "@om/ui";
import { registerServiceWorker } from "./sw/register";

void registerServiceWorker();

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
