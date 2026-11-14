import { useState } from "react";
import type { ConversationSummary, GroupDetailView, GroupInviteView } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import { Avatar } from "../conversations/Avatar";
import type { ConversationServices } from "../conversations/services";
import type { GroupsCopy } from "./copy";
import type { PeopleNames } from "./hooks";
import { runGroupAction } from "./services";
import "./groups.css";

export interface GroupMembersProps {
  readonly services: ConversationServices;
  readonly copy: GroupsCopy;
  readonly detail: GroupDetailView;
  readonly names: PeopleNames;
  readonly invites: readonly GroupInviteView[];
  readonly chats: readonly ConversationSummary[];
  /** A step went through (the caller refreshes and may say it aloud). */
  readonly onChanged: (what: "invited" | "removed") => void;
  readonly onLeft: () => void;
}

type Pending = { readonly kind: "remove"; readonly account: string; readonly name: string } | { readonly kind: "leave" };

/**
 * Who is in a group, and what a person can do about it: add, remove, leave. Whether the person is allowed is Core's call and its refusal
 * is said plainly; the screen does not guess at roles. Removing and leaving say what they do not do (nothing already received is erased).
 */
export function GroupMembers({ services, copy, detail, names, invites, chats, onChanged, onLeft }: GroupMembersProps) {
  const c = copy.members;
  const group = detail.summary.name;
  const [pending, setPending] = useState<Pending | null>(null);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  let unnamed = 0;
  const label = (account: string): string => (names.isYou(account) ? c.you : names.nameOf(account) ?? c.someone(++unnamed));
  const rows = detail.people.map((p) => ({ p, text: label(p.account) }));

  const run = async (actionId: string, params: Readonly<Record<string, unknown>>, after: () => void): Promise<void> => {
    setBusy(true);
    setProblem(null);
    const r = await runGroupAction(services, actionId, params);
    setBusy(false);
    setPending(null);
    if (r.ok) after(); else setProblem(copy.refused(r.refused));
  };
  const inGroup = new Set(invites.filter((i) => i.conversationId === detail.summary.conversationId && i.state !== "declined").map((i) => i.relationshipId as string));
  const addable = chats.filter((x) => x.relationship === "active" && !inGroup.has(x.id));

  return (
    <div className="om-group-members">
      <ul className="om-group-members__list" aria-label={c.listLabel}>
        {rows.map(({ p, text }) => (
          <li key={p.account} className="om-group-members__row" data-status={p.status}>
            <Avatar name={text} />
            <span className="om-group-members__who">
              <bdi className="om-group-members__name">{text}</bdi>
              <span className="om-group-members__sub">
                {p.status === "invited" ? c.invited : p.roles.map((r) => c.roles[r] ?? r).join(", ")}
              </span>
            </span>
            {p.status === "member" && !names.isYou(p.account) ? (
              <Button variant="quiet" disabled={busy} aria-label={c.remove(text)} onClick={() => { setPending({ kind: "remove", account: p.account, name: text }); }}>{c.removeShort}</Button>
            ) : null}
          </li>
        ))}
      </ul>

      {pending?.kind === "remove" ? (
        <div role="group" aria-label={c.removeAsk(pending.name, group)} className="om-request__confirm">
          <Text as="p">{c.removeAsk(pending.name, group)}</Text>
          <Text as="p" size="sm" tone="secondary">{c.removeNote}</Text>
          <Button variant="primary" onClick={() => { setPending(null); }}>{c.removeKeep}</Button>
          <Button variant="danger" disabled={busy} onClick={() => { void run("group-remove-member", { conversationId: detail.summary.conversationId, account: pending.account }, () => { onChanged("removed"); }); }}>{c.removeConfirm}</Button>
        </div>
      ) : null}

      <section className="om-group-members__add" aria-label={c.add}>
        <Text as="h3" size="md" weight="bold">{c.add}</Text>
        {addable.length === 0 ? <Text as="p" size="sm" tone="secondary">{c.addNobody}</Text> : (
          <ul className="om-group-members__candidates">
            {addable.map((x) => (
              <li key={x.id}>
                <Button variant="secondary" disabled={busy} aria-label={c.addLabel(x.displayName)} onClick={() => { void run("group-invite", { conversationId: detail.summary.conversationId, relationshipId: x.id }, () => { onChanged("invited"); }); }}>
                  <bdi>{x.displayName}</bdi>
                </Button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <Text as="p" size="sm" tone="secondary">{c.historyNote}</Text>
      <Text as="p" size="sm" tone="secondary">{c.proofNote}</Text>

      {pending?.kind === "leave" ? (
        <div role="group" aria-label={c.leaveAsk(group)} className="om-request__confirm">
          <Text as="p">{c.leaveAsk(group)}</Text>
          <Text as="p" size="sm" tone="secondary">{c.leaveNote}</Text>
          <Button variant="primary" onClick={() => { setPending(null); }}>{c.leaveStay}</Button>
          <Button variant="danger" disabled={busy} onClick={() => { void run("group-leave", { conversationId: detail.summary.conversationId }, onLeft); }}>{c.leaveConfirm}</Button>
        </div>
      ) : <Button variant="danger" disabled={busy} onClick={() => { setPending({ kind: "leave" }); }}>{c.leave}</Button>}
      {problem === null ? null : <p role="alert" className="om-group-form__problem">{problem}</p>}
    </div>
  );
}
