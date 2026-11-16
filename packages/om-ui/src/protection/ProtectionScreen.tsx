import { useState } from "react";
import type { ChosenPrivacyLevel, PrivacyPolicyLevel } from "@om/platform-api";
import { FocusHeading } from "../devices/FocusHeading";
import { useLiveQuery, type SettingsServices } from "../devices/operation";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { ProtectionCopy } from "./copy";
import "../devices/devices.css";

export interface ProtectionScreenProps {
  readonly services: SettingsServices;
  readonly copy: ProtectionCopy;
}

const LEVELS: readonly ChosenPrivacyLevel[] = ["recommended", "hardened", "maximum"];
/** Display mirror of Core's rank (Custom ranks with Hardened). Core refuses an unconfirmed loosening whatever this says; a mismatch can only make the screen ask when it need not. */
const rank = (l: PrivacyPolicyLevel): number => (l === "recommended" ? 0 : l === "maximum" ? 2 : 1);

/**
 * Settings > Protection. Choosing a stricter level applies at once, with its cost said first. Choosing a less strict one first names what
 * changes and needs its own confirmation; nothing is ever loosened silently (ADR-0189 section 3 wording, applied to the person's own level).
 * The level is local to this device and never sent. The change goes through the Action Registry (a person's own tap only) to Core.
 */
export function ProtectionScreen({ services, copy }: ProtectionScreenProps) {
  const live = useLiveQuery(services, { kind: "GetPrivacyLevel", ctx: services.ctx }, "privacy-level");
  const [ask, setAsk] = useState<ChosenPrivacyLevel | null>(null);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState(false);
  const [done, setDone] = useState<ChosenPrivacyLevel | null>(null);

  if (live.status === "loading") return <Text role="status">{copy.loading}</Text>;
  if (live.status === "failed") return <Text role="alert">{copy.loadFailed}</Text>;
  const current = live.value.value;

  const send = async (level: ChosenPrivacyLevel, confirmed: boolean): Promise<void> => {
    setBusy(true); setProblem(false); setDone(null);
    let refused: string | null = "unavailable";
    try {
      const api = services.client.privacy;
      const r = services.registry.resolve({ actionId: "set-privacy-level", params: { level, ...(confirmed ? { confirmed: true } : {}) }, ctx: services.ctx, origin: "direct" });
      if (api !== undefined && r.status === "privacy") { const step = await api.set(services.ctx, r.intent); refused = step.ok ? null : step.refused; }
    } catch { refused = "unavailable"; }
    setBusy(false);
    if (refused === null) { setAsk(null); setDone(level); return; }
    if (refused === "loosening_needs_confirmation" && !confirmed) { setAsk(level); return; } // Core saw a loosening the screen did not predict: ask, never apply
    setAsk(null); setProblem(true);
  };
  const choose = (level: ChosenPrivacyLevel): void => {
    setDone(null); setProblem(false);
    if (rank(level) < rank(current)) { setAsk(level); return; }
    void send(level, false);
  };
  const nameOf = (l: ChosenPrivacyLevel): string => copy.level[l].name;

  return (
    <section className="om-settings om-protection" aria-labelledby="om-protection-h">
      <Text as="h1" size="xl" weight="bold" id="om-protection-h">{copy.heading}</Text>
      <Text as="p">{copy.intro}</Text>
      <Text as="p" size="sm" tone="secondary">{copy.scope}</Text>
      {current === "custom" ? <Text as="p">{copy.custom}</Text> : null}
      {ask === null ? (
        <ul className="om-settings__list" aria-label={copy.levelsLabel}>
          {LEVELS.map((l) => (
            <li key={l} className="om-settings__row" data-current={current === l ? "true" : undefined}>
              <Text as="h2" size="md" weight="bold">{nameOf(l)}</Text>
              <Text as="p">{copy.level[l].body}</Text>
              {current === l ? <Text as="p" weight="bold">{copy.inUse}</Text> : (
                <div>
                  <Button variant="secondary" disabled={busy} onClick={() => { choose(l); }}>{copy.use(nameOf(l))}</Button>
                  {l === "recommended" ? null : <Text as="p" size="sm" tone="secondary">{copy.cost[l]}</Text>}
                </div>
              )}
            </li>
          ))}
        </ul>
      ) : (
        <div role="group" aria-labelledby="om-protection-ask-h" className="om-settings__confirm">
          <FocusHeading id="om-protection-ask-h">{copy.loosen.title(nameOf(ask))}</FocusHeading>
          <Text as="p">{copy.loosen.body[ask]}</Text>
          <Button disabled={busy} onClick={() => { void send(ask, true); }}>{copy.loosen.confirm(nameOf(ask))}</Button>
          <Button variant="quiet" disabled={busy} onClick={() => { setAsk(null); }}>{copy.loosen.keep}</Button>
        </div>
      )}
      {done === null ? null : <Text role="status">{copy.applied(nameOf(done))}</Text>}
      {problem ? <Text role="alert">{copy.refused}</Text> : null}
    </section>
  );
}
