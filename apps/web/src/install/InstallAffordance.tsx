import { Button, Text } from "@om/ui";
import { landingCopy } from "../copy/en";
import { useInstall } from "./useInstall";

/** A quiet install control: nothing at all where installing is not possible, a hint on iOS, a button where the browser offers one. */
export function InstallAffordance() {
  const state = useInstall();
  const c = landingCopy.install;
  switch (state.kind) {
    case "promptable": return <Button onClick={state.install}>{c.installButton}</Button>;
    case "ios-manual": return <Text as="p" size="sm" tone="secondary">{c.iosHint}</Text>;
    case "installed": return <Text as="p" size="sm" tone="secondary">{c.installed}</Text>;
    case "none": return null;
  }
}
