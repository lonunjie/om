import type { GroupStates } from "./hooks";
import type { GroupsCopy } from "./copy";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import "./groups.css";

export interface GroupStatusProps {
  readonly copy: GroupsCopy;
  readonly states: GroupStates;
  /** Only this group's own states (a chat), or everything (the list). */
  readonly scope: { readonly groupId: string; readonly groupName: string } | "all";
  /** Who an accepted invitation is from (for the line "X said yes"), or null when unknown. */
  readonly inviteName?: (inviteId: string) => string | null;
  readonly groupName?: (groupId: string) => string | null;
  readonly onDismissUndeliverable: () => void;
  readonly onDismissSettingBlocked: (groupId: string) => void;
  readonly onDismissAdmitBlocked: (inviteId: string) => void;
}

/**
 * The honest states of the group carriage, each as object, what it means and what happens next (CONTENT_AND_MICROCOPY_VOICE). A state stays until
 * it is no longer true or the person has seen it; nothing here promises a retry Core does not make. A superseded Commit has no line (it is retried silently).
 */
export function GroupStatus({ copy, states, scope, inviteName, groupName, onDismissUndeliverable, onDismissSettingBlocked, onDismissAdmitBlocked }: GroupStatusProps) {
  const c = copy.states;
  const lines: { readonly key: string; readonly title: string; readonly body: string; readonly dismiss?: { readonly label: string; readonly run: () => void } }[] = [];
  // A held update names no group, so it is shown wherever the person is (a phone shows one pane at a time).
  if (states.held) lines.push({ key: "held", title: c.held.title, body: c.held.body });
  if (scope === "all") {
    for (const [inviteId, reason] of states.admitBlocked) lines.push({ key: `a-${inviteId}`, title: c.admitBlocked.title(inviteName?.(inviteId) ?? ""), body: c.reason(reason, "admit"), dismiss: { label: c.admitBlocked.dismiss, run: () => { onDismissAdmitBlocked(inviteId); } } });
  }
  if (states.undeliverable > 0) lines.push({ key: "undeliverable", title: c.undeliverable.title(states.undeliverable), body: c.undeliverable.body, dismiss: { label: c.undeliverable.dismiss, run: onDismissUndeliverable } });
  for (const [gid, reason] of states.settingBlocked) {
    if (scope !== "all" && scope.groupId !== gid) continue;
    const name = scope === "all" ? groupName?.(gid) ?? "" : scope.groupName;
    lines.push({ key: `s-${gid}`, title: c.settingBlocked.title(name), body: c.reason(reason, "setting"), dismiss: { label: c.settingBlocked.dismiss, run: () => { onDismissSettingBlocked(gid); } } });
  }
  if (lines.length === 0) return null;
  return (
    <section className="om-group-status" aria-label={c.label}>
      {lines.map((l) => (
        <div key={l.key} className="om-group-status__line" role="status">
          <Text as="p" weight="bold">{l.title}</Text>
          <Text as="p" size="sm">{l.body}</Text>
          {l.dismiss === undefined ? null : <Button variant="quiet" onClick={l.dismiss.run}>{l.dismiss.label}</Button>}
        </div>
      ))}
    </section>
  );
}
