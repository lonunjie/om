import type { ConversationSummary, HeldGroupInvitations } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { GroupsCopy } from "./copy";
import "./groups.css";

export interface HeldGroupInvitesProps {
  readonly copy: GroupsCopy;
  readonly held: HeldGroupInvitations;
  readonly chats: readonly ConversationSummary[];
  /**
   * Whether the group files are ready on this device. When they are, the invitations below are ordinary pending ones (the card
   * above the chat list), so only the overflow notice is left to show here.
   */
  readonly groupsReady: boolean;
  /** Gets the groups ready (the loader sheet); the ordinary invitation card then offers Join. Never accepts anything. */
  readonly onOpen: () => void;
  readonly onAcknowledge: () => void;
}

/** How many invitations are named in the entry; the rest are counted (a calm entry, not a list). */
const SHOWN = 3;

/**
 * "You have a group invitation": the entry for an invitation Core verified and is holding because this device has not loaded the groups
 * yet. It says nothing has happened, and its one action gets the groups ready, where Join and No thanks are asked as usual. It never
 * joins. If the held store was ever full, a plain notice says so instead of dropping an invitation without a trace.
 */
export function HeldGroupInvites({ copy, held, chats, groupsReady, onOpen, onAcknowledge }: HeldGroupInvitesProps) {
  const c = copy.held;
  const names = new Map(chats.map((x) => [x.id as string, x.displayName]));
  const list = groupsReady ? [] : held.invitations;
  const { invites, replies } = held.overflow;
  if (list.length === 0 && invites === 0 && replies === 0) return null;
  const shown = list.slice(0, SHOWN);
  return (
    <section className="om-page om-stack" aria-label={c.heading}>
      {list.length > 0 ? (
        <div className="om-card om-group-invite">
          <Text as="p" weight="bold">{c.title(list.length)}</Text>
          {shown.map((i) => (
            <Text as="p" key={`${i.relationshipId}/${i.inviteId}`}>
              <bdi>{c.from(names.get(i.relationshipId as string) ?? "", i.groupName)}</bdi>
              {i.receivedMs === null ? null : <Text as="span" size="sm" tone="secondary">{` ${c.received(i.receivedMs)}`}</Text>}
            </Text>
          ))}
          {list.length > shown.length ? <Text as="p" size="sm" tone="secondary">{c.more(list.length - shown.length)}</Text> : null}
          <Text as="p" size="sm" tone="secondary">{c.body}</Text>
          <div className="om-request__actions">
            <Button variant="primary" onClick={onOpen}>{c.open(list.length)}</Button>
          </div>
        </div>
      ) : null}
      {invites > 0 || replies > 0 ? (
        <div className="om-card om-group-invite" role="status">
          {invites > 0 ? (
            <>
              <Text as="p" weight="bold">{c.overflow.title}</Text>
              <Text as="p" size="sm" tone="secondary">{c.overflow.invites}</Text>
            </>
          ) : null}
          {replies > 0 ? (
            <>
              <Text as="p" weight="bold">{c.overflow.repliesTitle}</Text>
              <Text as="p" size="sm" tone="secondary">{c.overflow.replies}</Text>
            </>
          ) : null}
          <div className="om-request__actions">
            <Button variant="quiet" onClick={onAcknowledge}>{c.overflow.dismiss}</Button>
          </div>
        </div>
      ) : null}
    </section>
  );
}
