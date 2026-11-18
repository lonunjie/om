import { useState } from "react";
import type { ConversationSummary, GroupInviteView, GroupRejoinReviewView, GroupRejoinView } from "@om/platform-api";
import { shownName } from "../conversations/shownName";
import type { ConversationServices } from "../conversations/services";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { GroupsCopy } from "./copy";
import { findSameNames } from "./sameName";
import { runGroupAction } from "./services";

interface Props {
  readonly services: ConversationServices;
  readonly copy: GroupsCopy;
  readonly chats: readonly ConversationSummary[];
  readonly onChanged: () => void;
}

function contactLabel(chat: ConversationSummary, chats: readonly ConversationSummary[], copy: GroupsCopy): string {
  const name = shownName(chat);
  const same = findSameNames(chats.map((c) => ({ id: c.id, name: shownName(c) }))).of(chat.id);
  return same === null ? name : copy.sameName.of(name, same.index, same.of);
}

/** A recovered profile can ask a known contact for an ordinary invitation. */
export function GroupRejoinRequests({ services, copy, chats, onChanged, rejoin, invites, onEnable }: Props & {
  readonly rejoin: GroupRejoinView;
  readonly invites: readonly GroupInviteView[];
  readonly onEnable?: () => void;
}) {
  const c = copy.rejoin;
  const [contact, setContact] = useState("");
  const [room, setRoom] = useState("");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const choices = chats.filter((chat) => rejoin.contacts.includes(chat.id));
  const accepting = new Set(invites.filter((invite) => invite.dir === "incoming" && invite.state === "accepted"
    && rejoin.pending.some((request) => request.relationshipId === invite.relationshipId && request.roomHint === invite.groupName))
    .map((invite) => invite.groupName));
  const pending = [...new Set(rejoin.pending.map((r) => r.roomHint))].filter((name) => !accepting.has(name));
  if (choices.length === 0 && pending.length === 0) return null;

  const canSend = !busy && room.trim() !== "" && choices.some((chat) => chat.id === contact);
  const submit = async (e: { preventDefault(): void }): Promise<void> => {
    e.preventDefault();
    if (!canSend) return;
    setBusy(true);
    setFailed(false);
    const r = await runGroupAction(services, "group-ask-to-rejoin", { relationshipId: contact, roomHint: room.trim() });
    setBusy(false);
    if (r.ok) {
      setRoom("");
      onChanged();
    } else {
      setFailed(true);
    }
  };

  return (
    <section className="om-page om-stack" aria-label={c.ask}>
      {pending.map((name) => <div key={name} className="om-card" role="status"><Text as="p"><bdi>{c.pending(name)}</bdi></Text></div>)}
      {choices.length === 0 ? null : onEnable ? <Button variant="quiet" onClick={onEnable}>{c.ask}</Button> : (
        <details className="om-group-form__protection">
          <summary>{c.ask}</summary>
          <form className="om-group-form" onSubmit={(e) => { void submit(e); }} aria-busy={busy}>
            <label className="om-group-form__field">
              <span>{c.contact}</span>
              <select className="om-group-form__input" value={contact} onChange={(e) => { setContact(e.target.value); }}>
                <option value="">{c.choose}</option>
                {choices.map((chat) => <option key={chat.id} value={chat.id}>{contactLabel(chat, chats, copy)}</option>)}
              </select>
            </label>
            <label className="om-group-form__field">
              <span>{c.room}</span>
              <input className="om-group-form__input" value={room} maxLength={256} autoComplete="off" onChange={(e) => { setRoom(e.target.value); }} />
            </label>
            <Button type="submit" disabled={!canSend}>{c.send}</Button>
            {failed ? <Text as="p" role="status">{c.sendFailed}</Text> : null}
          </form>
        </details>
      )}
    </section>
  );
}

export function GroupRejoinReview({ services, copy, chats, onChanged, review }: Props & {
  readonly review: GroupRejoinReviewView;
}) {
  const c = copy.rejoin;
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const chat = chats.find((x) => x.id === review.relationshipId);
  if (chat === undefined) return null;
  const resolve = async (agree: boolean): Promise<void> => {
    setBusy(true);
    setFailed(false);
    const r = await runGroupAction(services, "group-resolve-rejoin", { requestId: review.id, agree });
    setBusy(false);
    setFailed(!r.ok);
    onChanged();
  };
  const working = busy || review.working;
  return (
    <div className="om-card om-group-invite">
      <Text as="p"><bdi>{c.review(contactLabel(chat, chats, copy))}</bdi></Text>
      {working ? <Text as="p" size="sm" role="status">{c.working}</Text> : (
        <div className="om-request__actions">
          {review.canInvite ? <Button onClick={() => { void resolve(true); }}>{c.invite}</Button> : (
            <Text as="p" size="sm" tone="secondary">{c.policyPending}</Text>
          )}
          <Button variant="quiet" onClick={() => { void resolve(false); }}>{c.decline}</Button>
        </div>
      )}
      {failed ? <Text as="p" size="sm" role="status">{c.inviteFailed}</Text> : null}
    </div>
  );
}
