import { shownName } from "../conversations/shownName";
import { useState } from "react";
import type { ConversationSummary, GroupProtectionLevel } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import { Avatar } from "../conversations/Avatar";
import { PatternAvatar } from "./PatternAvatar";
import { findSameNames } from "./sameName";
import type { ConversationServices } from "../conversations/services";
import type { GroupsCopy } from "./copy";
import { PROTECTION_LEVELS } from "./protection";
import { runGroupAction } from "./services";
import "./groups.css";

export interface NewGroupProps {
  readonly services: ConversationServices;
  readonly copy: GroupsCopy;
  /** The person's chats. Only active ones can be invited (a chat still waiting for the other person cannot). */
  readonly chats: readonly ConversationSummary[];
  /** The group was made: its ConversationID is not known to the caller, so the list simply refreshes. */
  readonly onCreated: (name: string) => void;
}

/** New group: choose people you already chat with, optionally name it, Create. Nobody is added until they say yes. */
export function NewGroup({ services, copy, chats, onCreated }: NewGroupProps) {
  const c = copy.newGroup;
  const eligible = chats.filter((x) => x.relationship === "active");
  const [name, setName] = useState("");
  const [picked, setPicked] = useState<ReadonlySet<string>>(() => new Set());
  const [level, setLevel] = useState<GroupProtectionLevel>(1);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const same = findSameNames(eligible.map((x) => ({ id: x.id, name: shownName(x) })));
  if (eligible.length === 0) return <Text as="p" data-sheet-initial="">{c.nobody}</Text>;
  const toggle = (id: string): void => { setPicked((p) => { const n = new Set(p); if (n.has(id)) n.delete(id); else n.add(id); return n; }); };
  const submit = async (e: { preventDefault: () => void }): Promise<void> => {
    e.preventDefault();
    if (busy || picked.size === 0) return;
    setBusy(true);
    setProblem(null);
    const chosen = eligible.filter((x) => picked.has(x.id)).map((x) => shownName(x));
    const r = await runGroupAction(services, "group-create", { name: name.trim() === "" ? c.unnamed(chosen) : name.trim(), relationshipIds: [...picked], ...(level === 1 ? {} : { protection: level }) });
    setBusy(false);
    if (r.ok) onCreated(name.trim()); else setProblem(copy.refused(r.refused));
  };
  return (
    <form className="om-group-form" onSubmit={(e) => { void submit(e); }}>
      <label className="om-group-form__field">
        <span>{c.nameLabel}</span>
        <input type="text" className="om-group-form__input" value={name} maxLength={64} placeholder={c.namePlaceholder} autoComplete="off" data-sheet-initial="" onChange={(e) => { setName(e.target.value); }} />
      </label>
      <fieldset className="om-group-form__people">
        <legend>{c.pick}</legend>
        {same.groups.map((g) => <Text key={g.name} as="p" size="sm" tone="secondary" className="om-group-samename"><bdi>{(g.kind === "alike" ? copy.sameName.alike : copy.sameName.contacts)(g.name, g.count)}</bdi></Text>)}
        {eligible.map((x) => (
          <label key={x.id} className="om-group-form__person">
            <input type="checkbox" checked={picked.has(x.id)} onChange={() => { toggle(x.id); }} />
            {same.of(x.id) === null ? <Avatar name={shownName(x)} /> : <PatternAvatar seed={x.id} />}
            <bdi>{same.of(x.id) === null ? shownName(x) : copy.sameName.of(shownName(x), same.of(x.id)?.index ?? 1, same.of(x.id)?.of ?? 1)}</bdi>
          </label>
        ))}
      </fieldset>
      <details className="om-group-form__protection">
        <summary>{copy.protection.summary(copy.protection.levels[level].label)}</summary>
        <fieldset className="om-group-form__levels">
          <legend>{copy.protection.legend}</legend>
          {PROTECTION_LEVELS.map((l) => (
            <label key={l} className="om-group-form__level">
              <input type="radio" name="om-group-protection" value={l} checked={level === l} onChange={() => { setLevel(l); }} />
              <span><strong>{copy.protection.levels[l].label}</strong><span className="om-group-form__level-line">{copy.protection.levels[l].line}</span></span>
            </label>
          ))}
          <Text as="p" size="sm" tone="secondary">{copy.protection.later}</Text>
        </fieldset>
      </details>
      <Text as="p" size="sm" tone="secondary">{c.consent}</Text>
      <Button type="submit" variant="primary" disabled={busy || picked.size === 0}>{busy ? c.creating : picked.size === 0 ? c.create : `${c.create} · ${c.selected(picked.size)}`}</Button>
      {problem === null ? null : <p role="alert" className="om-group-form__problem">{problem}</p>}
    </form>
  );
}
