import { useState } from "react";
import { AppShell, Button, LiveRegion, Text, ThemeProvider, type ShellDestination, type ShellLabels } from "./shell/index";

// Placeholder content slots only: UW-6/7/8 fill list, detail, composer, omnibar and profile. No Core calls here.
const DESTINATIONS: readonly ShellDestination[] = [{ id: "chats", label: "Chats" }];
const LABELS: ShellLabels = { navigation: "Main", list: "Chats", detail: "Conversation", skipToContent: "Skip to content" };

export const App = () => {
  const [active, setActive] = useState("chats");
  const [open, setOpen] = useState(false);
  return (
    <ThemeProvider>
      <AppShell
        destinations={DESTINATIONS}
        activeDestination={active}
        onDestinationChange={setActive}
        conversationOpen={open}
        labels={LABELS}
        list={<Button onClick={() => { setOpen(true); }}>Open conversation</Button>}
        detail={
          <>
            <Button variant="quiet" onClick={() => { setOpen(false); }}>Back</Button>
            <Text>No conversation selected.</Text>
          </>
        }
        composerZone={<Text tone="muted">Composer</Text>}
        omnibarTrigger={<Button>Find or do</Button>}
        profileControl={<Button variant="quiet">Profile</Button>}
      />
      <LiveRegion>{open ? "Conversation opened" : ""}</LiveRegion>
    </ThemeProvider>
  );
};
