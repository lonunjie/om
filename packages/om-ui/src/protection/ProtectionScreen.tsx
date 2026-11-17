import { useState } from "react";
import { isTerminalOutcome } from "@om/platform-api";
import type { ChosenPrivacyLevel, PrivacyPolicyLevel } from "@om/platform-api";
import { FocusHeading } from "../devices/FocusHeading";
import { useLiveQuery, useOperation, type SettingsServices } from "../devices/operation";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { ProtectionCopy } from "./copy";
import "../devices/devices.css";

export interface ProtectionScreenProps {
  readonly services: SettingsServices;
  readonly copy: ProtectionCopy;
}

const LEVELS: readonly ChosenPrivacyLevel[] = ["recommended", "hardened", "maximum"];
/** Display mirror of Core's rank (Custom ranks with Hardened). It only decides when the screen asks first; Core's nonce is bound to the level the change was issued against. */
const rank = (l: PrivacyPolicyLevel): number => (l === "recommended" ? 0 : l === "maximum" ? 2 : 1);

/**
 * Settings > Protection. Choosing a stricter level applies at once (one tap), with its cost said first. Choosing a less strict one
 * first names what changes and needs its own confirmation; nothing is ever loosened silently (ADR-0189 section 3 wording, applied to
 * the person's own level). The level is local to this device and never sent. Both directions are the `ChangePrivacyPolicy` intent
 * with a Core-issued, single-use nonce (ADR-0181 section 8): the Action Registry obtains it for a person's own tap only, and Core
 * refuses a stale, reused or expired one, so a failed change leaves the level as it was.
 */
export function ProtectionScreen({ services, copy }: ProtectionScreenProps) {
  const live = useLiveQuery(services, { kind: "GetPrivacyLevel", ctx: services.ctx }, "privacy-level");
  const op = useOperation(services);
  const [ask, setAsk] = useState<ChosenPrivacyLevel | null>(null);
  const [target, setTarget] = useState<ChosenPrivacyLevel | null>(null);

  if (live.status === "loading") return <Text role="status">{copy.loading}</Text>;
  if (live.status === "failed") return <Text role="alert">{copy.loadFailed}</Text>;
  const current = live.value.value;

  const send = (level: ChosenPrivacyLevel): void => {
    setAsk(null); setTarget(level);
    void op.run("change-privacy-policy", { policy: level });
  };
  const choose = (level: ChosenPrivacyLevel): void => {
    op.reset(); setTarget(null);
    if (rank(level) < rank(current)) { setAsk(level); return; }
    send(level);
  };
  const outcome = op.state.phase === "observing" ? op.state.outcome : null;
  const done = target !== null && outcome?.kind === "Committed" ? target : null;
  const problem = op.state.phase === "failed" || (outcome !== null && outcome.kind !== "Committed" && isTerminalOutcome(outcome));
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
                  <Button variant="secondary" disabled={op.locked} onClick={() => { choose(l); }}>{copy.use(nameOf(l))}</Button>
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
          <Button disabled={op.locked} onClick={() => { send(ask); }}>{copy.loosen.confirm(nameOf(ask))}</Button>
          <Button variant="quiet" disabled={op.locked} onClick={() => { setAsk(null); }}>{copy.loosen.keep}</Button>
        </div>
      )}
      {done === null ? null : <Text role="status">{copy.applied(nameOf(done))}</Text>}
      {problem ? <Text role="alert">{copy.refused}</Text> : null}
    </section>
  );
}
