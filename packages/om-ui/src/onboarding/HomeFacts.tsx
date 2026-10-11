import { useState } from "react";
import type { CoreClient, HomeSummary, ProfileExecutionContext } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { OnboardingCopy } from "./copy";

export interface HomeFactsProps {
  readonly client: CoreClient;
  readonly ctx: ProfileExecutionContext;
  readonly copy: OnboardingCopy["home"];
}

type Phase = { readonly status: "closed" } | { readonly status: "loading" } | { readonly status: "failed" } | { readonly status: "open"; readonly home: HomeSummary };

/** On-demand Home facts. Only machine codes from Core are shown, each mapped through copy; no provider is named here. */
export function HomeFacts({ client, ctx, copy }: HomeFactsProps) {
  const [phase, setPhase] = useState<Phase>({ status: "closed" });
  const toggle = (): void => {
    if (phase.status !== "closed") { setPhase({ status: "closed" }); return; }
    setPhase({ status: "loading" });
    client.query({ kind: "GetHomeSummary", ctx }).then((home) => { setPhase({ status: "open", home }); }, () => { setPhase({ status: "failed" }); });
  };
  return (
    <div className="om-onb__home">
      <Button variant="quiet" aria-expanded={phase.status !== "closed"} aria-controls="om-onb-home-facts" onClick={toggle}>{copy.toggle}</Button>
      <section id="om-onb-home-facts" hidden={phase.status === "closed"} aria-label={copy.heading}>
        {phase.status === "loading" ? <Text as="p">{copy.loading}</Text> : null}
        {phase.status === "failed" ? <Text as="p" role="alert">{copy.loadFailed}</Text> : null}
        {phase.status === "open" ? (
          <>
            <Text as="h3" size="md" weight="bold">{copy.tier(phase.home.tier)}</Text>
            <ul>
              {phase.home.observers.map((o) => <li key={o}>{copy.observers[o]}</li>)}
              <li>{phase.home.migrationAvailable ? copy.migrationAvailable : copy.migrationUnavailable}</li>
              {phase.home.clientOriginChoice.map((c) => <li key={c}>{copy.origin[c]}</li>)}
            </ul>
          </>
        ) : null}
      </section>
    </div>
  );
}
