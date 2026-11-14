import { useState } from "react";
import type { ConversationSummary, GroupInviteView } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { ConversationServices } from "../conversations/services";
import type { GroupsCopy } from "./copy";
import { runGroupAction } from "./services";
import "./groups.css";

export interface GroupInvitesProps {
  readonly services: ConversationServices;
  readonly copy: GroupsCopy;
  readonly invites: readonly GroupInviteView[];
  readonly chats: readonly ConversationSummary[];
  /** After an answer went through; `joined` is true when the person said yes. */
  readonly onAnswered: (joined: boolean) => void;
}

/**
 * Invitations to a group, shown as cards above the chat list like message requests. Joining is a choice: accepting tells the group the
 * person is there, so the card says so first, and an invitation nobody answers changes nothing.
 */
export function GroupInvites({ services, copy, invites, chats, onAnswered }: GroupInvitesProps) {
  const c = copy.invites;
  const [busy, setBusy] = useState<string | null>(null);
  const [said, setSaid] = useState<ReadonlyMap<string, string>>(() => new Map());
  const offered = invites.filter((i) => i.dir === "incoming" && i.state === "offered");
  if (offered.length === 0 && said.size === 0) return null;
  const names = new Map(chats.map((x) => [x.id as string, x.displayName]));
  const answer = async (i: GroupInviteView, join: boolean): Promise<void> => {
    setBusy(i.id);
    const r = await runGroupAction(services, join ? "group-accept" : "group-decline", { inviteId: i.id });
    setBusy(null);
    setSaid((m) => new Map(m).set(i.id, r.ok ? (join ? c.joined : c.declined) : copy.refused(r.refused)));
    if (r.ok) onAnswered(join);
  };
  return (
    <section className="om-page om-stack" aria-label={c.heading}>
      {offered.map((i) => (
        <div key={i.id} className="om-card om-group-invite">
          <Text as="p"><bdi>{c.from(names.get(i.relationshipId) ?? "", i.groupName)}</bdi></Text>
          <Text as="p" size="sm" tone="secondary">{c.hint}</Text>
          <div className="om-request__actions">
            <Button variant="primary" disabled={busy !== null} onClick={() => { void answer(i, true); }}>{busy === i.id ? c.joining : c.join}</Button>
            <Button variant="quiet" disabled={busy !== null} onClick={() => { void answer(i, false); }}>{c.decline}</Button>
          </div>
          {said.has(i.id) ? <p role="status">{said.get(i.id)}</p> : null}
        </div>
      ))}
      {[...said].filter(([id]) => !offered.some((i) => i.id === id)).map(([id, text]) => <p key={id} role="status" className="om-group-invite__said">{text}</p>)}
    </section>
  );
}
