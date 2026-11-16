// What a person sees the first time they use groups on a device: the groups module is fetched on demand (ADR-0187), so there is a short, calm wait,
// and if it cannot be loaded, an honest state with a way to try again. It decides nothing about groups: it only gets the module ready.
import { useCallback, useEffect, useRef, useState } from "react";
import { Button, Sheet, Text } from "@om/ui";
import type { ModuleFailure } from "../update/modules";

/**
 * Fetches and verifies the groups module and, on success, restarts the app onto it (so `enable` normally does not return).
 * `newGroup`: the person asked for the New group sheet, so the restarted app opens it; a person who came for a held invitation
 * is taken back to the invitation, not to a sheet they did not ask for.
 */
export interface GroupsSetup {
  readonly enable: (opts: { readonly newGroup: boolean }) => Promise<{ readonly ok: true } | { readonly ok: false; readonly reason: ModuleFailure }>;
}
export interface GroupsGateCopy {
  readonly title: string; readonly loading: string; readonly loadingNote: string; readonly failedHeading: string;
  readonly failed: { readonly unavailable: string; readonly refused: string };
  readonly retry: string; readonly close: string;
}

/** Network and storage trouble is worth a retry as is; a module that does not match this release is refused, and a reload fetches the current release. */
export const isRefusal = (reason: ModuleFailure): boolean => reason !== "unavailable" && reason !== "manifest_unavailable";

type State = { readonly k: "loading" } | { readonly k: "failed"; readonly reason: ModuleFailure };

export function GroupsGate({ setup, copy, newGroup, onClose }: { readonly setup: GroupsSetup; readonly copy: GroupsGateCopy; readonly newGroup: boolean; readonly onClose: () => void }) {
  const [state, setState] = useState<State>({ k: "loading" });
  const live = useRef(true);
  const run = useCallback((): void => {
    setState({ k: "loading" });
    setup.enable({ newGroup }).then((r) => { if (live.current && !r.ok) setState({ k: "failed", reason: r.reason }); }, () => { if (live.current) setState({ k: "failed", reason: "unavailable" }); });
  }, [setup, newGroup]);
  useEffect(() => {
    live.current = true;
    run();
    return () => { live.current = false; };
  }, [run]);
  return (
    <Sheet title={copy.title} closeLabel={copy.close} onClose={onClose}>
      {state.k === "loading" ? (
        <div role="status" aria-live="polite">
          <Text as="p">{copy.loading}</Text>
          <Text as="p" size="sm" tone="secondary">{copy.loadingNote}</Text>
        </div>
      ) : (
        <div role="alert">
          <Text as="h2" weight="bold">{copy.failedHeading}</Text>
          <Text as="p">{isRefusal(state.reason) ? copy.failed.refused : copy.failed.unavailable}</Text>
          <Button onClick={run}>{copy.retry}</Button>
        </div>
      )}
    </Sheet>
  );
}
