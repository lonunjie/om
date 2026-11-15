import { useEffect, useRef, useState } from "react";
import type { GroupDetailView, GroupEvent, GroupProtectionLevel, GroupRoomSetting } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { ConversationServices } from "../conversations/services";
import type { GroupsCopy } from "./copy";
import { PROTECTION_LEVELS, afterSetting, asLevel, loosenedBy, type LoosenKey } from "./protection";
import { runGroupAction } from "./services";
import "./groups.css";

export interface RoomSettingsProps {
  readonly services: ConversationServices;
  readonly copy: GroupsCopy;
  readonly detail: GroupDetailView;
  /** A change went through (the caller re-reads); `me` is this person's own account as the step reported it. The notice that everyone sees comes from Core, not from here. */
  readonly onChanged: (me: string | null) => void;
}

interface Ask { readonly setting: GroupRoomSetting; readonly keys: readonly LoosenKey[]; readonly level: GroupProtectionLevel | null }

/**
 * Room settings (ADR-0189 section 3, "Tightening, loosening, and what is never silent"). Tightening is one tap and applies at once, with
 * its cost said under the button first. Anything that makes the group less strict first names what changes and needs its own confirmation;
 * Core refuses an unconfirmed loosening whatever this screen thinks, and refuses what the person may not change.
 */
export function RoomSettings({ services, copy, detail, onChanged }: RoomSettingsProps) {
  const c = copy.settings;
  const p = detail.protection;
  const id = detail.summary.conversationId;
  const [ask, setAsk] = useState<Ask | null>(null);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const current = asLevel(p.profile);
  const confirmRef = useRef<HTMLHeadingElement>(null);
  // A confirmation takes focus so the person hears what changes before anything else.
  useEffect(() => { if (ask !== null) confirmRef.current?.focus(); }, [ask]);

  const send = async (setting: GroupRoomSetting, confirmed: boolean): Promise<void> => {
    setBusy(true);
    setProblem(null);
    setDone(null);
    const r = await runGroupAction(services, "group-room-setting", { conversationId: id, setting, ...(confirmed ? { confirmed: true } : {}) });
    setBusy(false);
    if (r.ok) { setAsk(null); setDone(c.applied); onChanged((r.step.events.find((e): e is Extract<GroupEvent, { kind: "policy_changed" }> => e.kind === "policy_changed")?.byAccount) ?? null); return; }
    if (r.refused === "loosening_needs_confirmation" && !confirmed) {
      // Core saw a loosening the screen did not predict: still ask, never apply.
      const next = afterSetting(p, setting);
      setAsk({ setting, keys: next === null ? [] : loosenedBy(p, next), level: "protection" in setting ? setting.protection : null });
      return;
    }
    setAsk(null);
    setProblem(copy.refused(r.refused));
  };
  /** One tap: a tightening goes straight through; a loosening (by the screen's own reading) asks first. */
  const choose = (setting: GroupRoomSetting): void => {
    const next = afterSetting(p, setting);
    const keys = next === null ? [] : loosenedBy(p, next);
    setDone(null);
    setProblem(null);
    if (keys.length > 0) { setAsk({ setting, keys, level: "protection" in setting ? setting.protection : null }); return; }
    void send(setting, false);
  };

  const row = (title: string, value: string, action: { readonly label: string; readonly cost?: string; readonly setting: GroupRoomSetting }[]) => (
    <li className="om-group-settings__row">
      <Text as="h4" size="md" weight="bold">{title}</Text>
      <Text as="p">{value}</Text>
      {action.map((a) => (
        <div key={a.label} className="om-group-settings__action">
          <Button variant="secondary" disabled={busy} onClick={() => { choose(a.setting); }}>{a.label}</Button>
          {a.cost === undefined ? null : <Text as="p" size="sm" tone="secondary">{a.cost}</Text>}
        </div>
      ))}
    </li>
  );
  const add = c.rows.add; const join = c.rows.join; const dev = c.rows.device;
  const joinOpen = p.joinRule !== 1 && p.joinRule !== 2;

  return (
    <div className="om-group-settings">
      <section aria-labelledby="om-group-level-h" className="om-group-settings__level">
        <Text as="h3" size="md" weight="bold" id="om-group-level-h">{c.level}</Text>
        <Text as="p">{p.custom || current === null ? copy.protection.custom : copy.protection.levels[current].label}</Text>
        <ul className="om-group-settings__levels">
          {PROTECTION_LEVELS.map((l) => {
            const inForce = !p.custom && l === current;
            return (
              <li key={l} className="om-group-settings__levelrow" data-current={inForce ? "true" : undefined}>
                <span><strong>{copy.protection.levels[l].label}</strong>{inForce ? <span className="om-group-settings__tag"> · {c.current}</span> : null}</span>
                <Text as="p" size="sm" tone="secondary">{copy.protection.levels[l].line}</Text>
                {inForce ? null : <Button variant="secondary" disabled={busy} onClick={() => { choose({ protection: l }); }}>{c.useLevel(copy.protection.levels[l].label)}</Button>}
              </li>
            );
          })}
        </ul>
      </section>

      <ul className="om-group-settings__rows">
        {row(add.title, p.membersMayInvite ? add.anyone : add.admins, p.membersMayInvite
          ? [{ label: add.tighten, cost: add.tightenCost, setting: { members_may_invite: false } }]
          : [{ label: add.loosen, setting: { members_may_invite: true } }])}
        {row(join.title, p.joinRule === 1 ? join.auto : p.joinRule === 2 ? join.approval : join.open, p.joinRule === 1
          ? [{ label: join.tighten, cost: join.tightenCost, setting: { join_rule: 2 } }]
          : joinOpen ? [{ label: join.tighten, cost: join.tightenCost, setting: { join_rule: 2 } }, { label: join.loosen, setting: { join_rule: 1 } }]
            : [{ label: join.loosen, setting: { join_rule: 1 } }])}
        {row(dev.title, p.newDevicePolicy === 2 ? dev.approval : dev.auto, p.newDevicePolicy === 2
          ? [{ label: dev.loosen, setting: { new_device_policy: 1 } }]
          : [{ label: dev.tighten, cost: dev.tightenCost, setting: { new_device_policy: 2 } }])}
        <li className="om-group-settings__row"><Text as="p">{c.history}</Text></li>
        <li className="om-group-settings__row"><Text as="p">{p.disappearingTimer === null ? c.timerOff : c.timerOn(p.disappearingTimer)}</Text></li>
      </ul>

      {ask === null ? null : (
        <div role="group" aria-labelledby="om-group-confirm-h" className="om-request__confirm om-group-settings__confirm">
          <h3 id="om-group-confirm-h" ref={confirmRef} tabIndex={-1} className="om-group-settings__confirm-h">{c.confirm.heading}</h3>
          <Text as="p">{c.confirm.lead}</Text>
          <ul className="om-group-settings__lines">
            {ask.keys.length === 0 ? <li>{c.confirm.generic}</li> : ask.keys.map((k) => <li key={k}>{k === "level" ? c.confirm.level(ask.level === null ? copy.protection.custom : copy.protection.levels[ask.level].label) : c.confirm.lines[k]}</li>)}
          </ul>
          <Text as="p" size="sm" tone="secondary">{c.confirm.announce}</Text>
          <Button variant="primary" onClick={() => { setAsk(null); }}>{c.confirm.keep}</Button>
          <Button variant="danger" disabled={busy} onClick={() => { void send(ask.setting, true); }}>{c.confirm.confirm}</Button>
        </div>
      )}
      <p className="om-visually-hidden" role="status">{busy ? c.saving : done ?? ""}</p>
      {done === null ? null : <Text as="p" size="sm" tone="secondary" aria-hidden="true">{done}</Text>}
      {problem === null ? null : <p role="alert" className="om-group-form__problem">{problem}</p>}
    </div>
  );
}
