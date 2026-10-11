import { useCallback } from "react";
import type { ProfileId, ProfileSummary } from "@om/platform-api";
import { Button, VisuallyHidden } from "../shell/primitives/index";
import { useActiveProfile, useProfiles } from "./context";

export interface ProfileSwitcherLabels {
  /** Accessible name of the list, e.g. "Identities". */
  readonly group: string;
  /** Visually hidden suffix for the current entry, e.g. "current". */
  readonly current: string;
}
export interface ProfileSwitcherProps {
  /** View models: `label` is the display name. */
  readonly profiles: readonly ProfileSummary[];
  readonly activeProfileId: ProfileId | null;
  readonly onSwitch: (id: ProfileId) => void;
  readonly labels: ProfileSwitcherLabels;
}

/** Shown only when two or more profiles exist. Switching is a selection change: no logout, no rekey. */
export function ProfileSwitcher({ profiles, activeProfileId, onSwitch, labels }: ProfileSwitcherProps) {
  if (profiles.length < 2) return null;
  return (
    <ul aria-label={labels.group} className="om-profile-switcher" style={{ listStyle: "none", margin: 0, padding: 0 }}>
      {profiles.map((p) => {
        const isCurrent = p.id === activeProfileId;
        return (
          <li key={p.id}>
            <Button variant={isCurrent ? "primary" : "quiet"} aria-current={isCurrent ? "true" : undefined} onClick={() => { if (!isCurrent) onSwitch(p.id); }}>
              {p.label}
              {isCurrent ? <VisuallyHidden> ({labels.current})</VisuallyHidden> : null}
            </Button>
          </li>
        );
      })}
    </ul>
  );
}

/** Wired to the provider; drop into AppShell's `profileControl` slot. */
export function ConnectedProfileSwitcher({ labels }: { readonly labels: ProfileSwitcherLabels }) {
  const { profiles, switchTo } = useProfiles();
  const active = useActiveProfile();
  const onSwitch = useCallback((id: ProfileId) => { switchTo(id); }, [switchTo]);
  return <ProfileSwitcher profiles={profiles} activeProfileId={active?.id ?? null} onSwitch={onSwitch} labels={labels} />;
}
