import { useState } from "react";
import { AppShell, Button, LiveRegion, Text, ThemeProvider, type ShellDestination, type ShellLabels } from "./shell/index";

/** Host-supplied strings for the unconnected shell. When omitted the bare placeholder strings below are used. */
export interface AppCopy {
  readonly labels: ShellLabels;
  readonly chats: string;
  readonly back: string;
  readonly selectConversation: string;
  readonly omnibar: string;
  readonly profile: string;
  readonly notConnected: { readonly heading: string; readonly body: string };
}

const FALLBACK: AppCopy = {
  labels: { navigation: "Main", list: "Chats", detail: "Conversation", skipToContent: "Skip to content" },
  chats: "Chats", back: "Back", selectConversation: "No conversation selected.", omnibar: "Find or do", profile: "Profile",
  notConnected: { heading: "Open conversation", body: "" },
};

// Production shell without a Core: a clear "not connected" state. The connected shell is composed by the host (apps/web).
export const App = ({ copy = FALLBACK }: { readonly copy?: AppCopy }) => {
  const [active, setActive] = useState("chats");
  const [open, setOpen] = useState(false);
  const destinations: readonly ShellDestination[] = [{ id: "chats", label: copy.chats }];
  return (
    <ThemeProvider>
      <AppShell
        destinations={destinations}
        activeDestination={active}
        onDestinationChange={setActive}
        conversationOpen={open}
        labels={copy.labels}
        list={
          <>
            <Text as="h2">{copy.notConnected.heading}</Text>
            {copy.notConnected.body === "" ? <Button onClick={() => { setOpen(true); }}>{copy.notConnected.heading}</Button> : <Text as="p">{copy.notConnected.body}</Text>}
          </>
        }
        detail={
          <>
            <Button variant="quiet" onClick={() => { setOpen(false); }}>{copy.back}</Button>
            <Text>{copy.selectConversation}</Text>
          </>
        }
        omnibarTrigger={<Button disabled>{copy.omnibar}</Button>}
        profileControl={<Button variant="quiet" disabled>{copy.profile}</Button>}
      />
      <LiveRegion>{open ? copy.selectConversation : ""}</LiveRegion>
    </ThemeProvider>
  );
};
