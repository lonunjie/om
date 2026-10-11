// DEV / HARNESS-BUILD ONLY. Imported dynamically from main.tsx behind a statically dead branch in production.
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { asConversationId, asPersonId, asProfileId } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { browserVaultEnv, createWebDeviceVault, createWebPlatform } from "@om/web-platform";
import { Text } from "@om/ui";
import { appStrings } from "../copy/en";
import { DevApp } from "./DevApp";

const PENDING = asProfileId("pending-me");
const WORK = asProfileId("work");

export function mountDev(root: HTMLElement): void {
  // Refuse a production bundle that was not built as the harness build.
  if (import.meta.env.PROD && import.meta.env.VITE_OM_DEV_HARNESS !== "1") throw new Error("dev harness refused: production build");
  const vaultEnv = browserVaultEnv("OM");
  const r = createRoot(root);
  if (vaultEnv === undefined) { // fail closed: no durable boundary, nothing is started
    r.render(<main className="om-dev-fail"><Text as="h1">{appStrings.vaultUnavailable.heading}</Text><Text as="p">{appStrings.vaultUnavailable.body}</Text></main>);
    return;
  }
  // The harness bundle is built in production mode, so its own PROD guard is satisfied explicitly after the check above.
  const harness = createHarness({ env: { PROD: false } });
  harness.seedProfile({ profileId: PENDING, label: "Me" });
  harness.seedProfile({
    profileId: WORK,
    label: "Work",
    conversations: [
      {
        id: asConversationId("c-ana"), personId: asPersonId("p-ana"), displayName: "Ana Reyes",
        messages: [
          { text: "Did the budget review move to Thursday?", direction: "incoming" },
          { text: "Yes, Thursday at 10.", direction: "outgoing" },
          { text: "Great, I will bring the draft.", direction: "incoming" },
        ],
      },
      {
        id: asConversationId("c-team"), personId: asPersonId("p-team"), displayName: "Design team",
        messages: [{ text: "Welcome to the team space.", direction: "incoming" }],
      },
    ],
    introductions: [{ id: "req-1", from: asPersonId("p-sam"), displayName: "Sam Okafor", personId: asPersonId("p-sam") }],
  });
  r.render(
    <StrictMode>
      <DevApp harness={harness} vault={createWebDeviceVault(vaultEnv)} platform={createWebPlatform()} pendingProfileId={PENDING} onboarded={[WORK]} />
    </StrictMode>,
  );
}
