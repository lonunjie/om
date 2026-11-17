import { shownName as nameShown } from "../conversations/shownName";
import { useState } from "react";
import type { ConversationSummary, GroupInviteView } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import { PatternAvatar } from "./PatternAvatar";
import { findSameNames } from "./sameName";
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
  // Said yes, not in yet: it stays visible until the add is done, so a person is never left guessing (ADR-0189: "Joining", then the state).
  const waiting = invites.filter((i) => i.dir === "incoming" && (i.state === "accepted" || i.state === "admitting"));
  if (offered.length === 0 && waiting.length === 0 && said.size === 0) return null;
  const names = new Map(chats.map((x) => [x.id as string, nameShown(x)]));
  // An invitation from someone whose name another contact also shows says so before you say yes (section 10).
  const same = findSameNames(chats.map((x) => ({ id: x.id, name: nameShown(x) })));
  const shownName = (id: string): string => { const n = names.get(id) ?? ""; const s = same.of(id); return s === null ? n : copy.sameName.of(n, s.index, s.of); };
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
          {same.of(i.relationshipId) === null ? null : <PatternAvatar seed={i.peerAccount} />}
          <Text as="p"><bdi>{c.from(shownName(i.relationshipId), i.groupName)}</bdi></Text>
          {same.of(i.relationshipId) === null ? null : <Text as="p" size="sm" tone="secondary" className="om-group-samename"><bdi>{(same.of(i.relationshipId)?.kind === "alike" ? copy.sameName.alike : copy.sameName.contacts)(same.of(i.relationshipId)?.name ?? "", same.of(i.relationshipId)?.of ?? 0)}</bdi></Text>}
          <Text as="p" size="sm" tone="secondary">{c.hint}</Text>
          <div className="om-request__actions">
            <Button variant="primary" disabled={busy !== null} onClick={() => { void answer(i, true); }}>{busy === i.id ? c.joining : c.join}</Button>
            <Button variant="quiet" disabled={busy !== null} onClick={() => { void answer(i, false); }}>{c.decline}</Button>
          </div>
          {said.has(i.id) ? <p role="status">{said.get(i.id)}</p> : null}
        </div>
      ))}
      {waiting.map((i) => (
        <div key={i.id} className="om-card om-group-invite" role="status">
          <Text as="p" weight="bold"><bdi>{c.waiting.title(i.groupName)}</bdi></Text>
          <Text as="p" size="sm" tone="secondary">{c.waiting.body}</Text>
        </div>
      ))}
      {[...said].filter(([id]) => !offered.some((i) => i.id === id) && !waiting.some((i) => i.id === id)).map(([id, text]) => <p key={id} role="status" className="om-group-invite__said">{text}</p>)}
    </section>
  );
}
