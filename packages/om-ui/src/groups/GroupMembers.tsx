import { shownName } from "../conversations/shownName";
import { useState } from "react";
import type { ConversationSummary, GroupDetailView, GroupInviteView } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import { Avatar } from "../conversations/Avatar";
import { PatternAvatar } from "./PatternAvatar";
import { findSameNames } from "./sameName";
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
  /** Opens Room settings (a sheet of its own, so Back returns to this one). */
  readonly onOpenSettings?: () => void;
}

type Pending = { readonly kind: "remove"; readonly account: string; readonly name: string } | { readonly kind: "leave" };

/**
 * Who is in a group, and what a person can do about it: add, remove, leave. Whether the person is allowed is Core's call and its refusal
 * is said plainly; the screen does not guess at roles. Removing and leaving say what they do not do (nothing already received is erased).
 */
export function GroupMembers({ services, copy, detail, names, invites, chats, onChanged, onLeft, onOpenSettings }: GroupMembersProps) {
  const c = copy.members;
  const group = detail.summary.name;
  const [pending, setPending] = useState<Pending | null>(null);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const label = (account: string): string => (names.isYou(account) ? c.you : names.nameOf(account) ?? c.someone(names.unnamedNumber(account)));
  // Section 10: a name another person here also shows gets a pattern, "1 of 2" and one calm line. Candidates are checked against the people already here.
  const inGroup = new Set(invites.filter((i) => i.conversationId === detail.summary.conversationId && i.state !== "declined").map((i) => i.relationshipId as string));
  const addable = chats.filter((x) => x.relationship === "active" && !inGroup.has(x.id));
  // The rule counts distinct ACCOUNTS: a member who is also a contact is one person, so both entries share the account's key.
  const keyOfMember = (account: string): string => `a:${account}`;
  const keyOfContact = (x: ConversationSummary): string => (x.peerAccount === undefined ? `c:${x.id}` : keyOfMember(x.peerAccount));
  const same = findSameNames([
    ...detail.people.flatMap((p) => (p.display === null || names.isYou(p.account) ? [] : [{ id: keyOfMember(p.account), name: p.display }])),
    ...addable.map((x) => ({ id: keyOfContact(x), name: shownName(x) })),
  ]);
  const shown = (key: string, name: string): string => { const s = same.of(key); return s === null ? name : copy.sameName.of(name, s.index, s.of); };
  const rows = detail.people.map((p) => ({ p, text: shown(keyOfMember(p.account), label(p.account)), seed: same.of(keyOfMember(p.account)) === null ? null : p.account }));

  const run = async (actionId: string, params: Readonly<Record<string, unknown>>, after: () => void): Promise<void> => {
    setBusy(true);
    setProblem(null);
    const r = await runGroupAction(services, actionId, params);
    setBusy(false);
    setPending(null);
    if (r.ok) after(); else setProblem(copy.refused(r.refused));
  };

  return (
    <div className="om-group-members">
      {same.groups.map((g) => <Text key={g.name} as="p" size="sm" tone="secondary" className="om-group-samename"><bdi>{(g.kind === "alike" ? copy.sameName.alike : copy.sameName.here)(g.name, g.count)}</bdi></Text>)}
      <ul className="om-group-members__list" aria-label={c.listLabel}>
        {rows.map(({ p, text, seed }) => (
          <li key={p.account} className="om-group-members__row" data-status={p.status}>
            {seed === null ? <Avatar name={text} /> : <PatternAvatar seed={seed} />}
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
                <Button variant="secondary" disabled={busy} aria-label={c.addLabel(shown(keyOfContact(x), shownName(x)))} onClick={() => { void run("group-invite", { conversationId: detail.summary.conversationId, relationshipId: x.id }, () => { onChanged("invited"); }); }}>
                  {same.of(keyOfContact(x)) === null ? null : <PatternAvatar seed={x.id} />}
                  <bdi>{shown(keyOfContact(x), shownName(x))}</bdi>
                </Button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <Text as="p" size="sm" tone="secondary">{c.membersSeeNote}</Text>
      <Text as="p" size="sm" tone="secondary">{c.historyNote}</Text>
      <Text as="p" size="sm" tone="secondary">{c.proofNote}</Text>

      {onOpenSettings === undefined ? null : <Button variant="secondary" onClick={onOpenSettings}>{c.settings}</Button>}

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
