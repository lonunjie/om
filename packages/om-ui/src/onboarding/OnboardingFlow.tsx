import { useReducer, useState } from "react";
import type { ReactNode } from "react";
import type { CoreClient, DeviceVault, OperationId, ProfileExecutionContext } from "@om/platform-api";
import type { ActionRegistry } from "../actions/registry";
import type { ConversationServices } from "../conversations/services";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { OnboardingCopy } from "./copy";
import { flowReducer, initialFlow } from "./flow";
import { HomeFacts } from "./HomeFacts";
import { InviteFirst } from "./InviteFirst";
import type { OnboardingPlatform } from "./platform";
import { ProtectDevice } from "./ProtectDevice";
import { RecoveryKeyStep } from "./RecoveryKeyStep";
import { StepHeading } from "./StepHeading";
import "./onboarding.css";

export interface OnboardingFlowProps {
  readonly client: CoreClient;
  readonly registry: ActionRegistry;
  /** Context of the not-yet-onboarded profile this flow creates the vault and identity for. */
  readonly ctx: ProfileExecutionContext;
  readonly vault: DeviceVault;
  readonly platform: OnboardingPlatform;
  readonly copy: OnboardingCopy;
  readonly onDone: () => void;
  /** Called when vault data already exists or is corrupt; the host opens its recovery flow. */
  readonly onExistingVault?: (reason: "already_exists" | "corrupt") => void;
  readonly renderQr?: (payload: string) => ReactNode;
  readonly scanQr?: () => Promise<string | null>;
  /** The host already showed its own introduction (the landing page): start at "protect device". */
  readonly skipWelcome?: boolean;
}

export function OnboardingFlow({ client, registry, ctx, vault, platform, copy, onDone, onExistingVault, renderQr, scanQr, skipWelcome = false }: OnboardingFlowProps) {
  const [flow, dispatch] = useReducer(flowReducer, skipWelcome ? flowReducer(initialFlow, { type: "start" }) : initialFlow);
  const [keyOperationId, setKeyOperationId] = useState<OperationId | null>(null);
  const services: ConversationServices = { client, registry, ctx };

  switch (flow.step) {
    case "welcome":
      return (
        <section className="om-onb__step" aria-labelledby="om-onb-welcome-h">
          <StepHeading id="om-onb-welcome-h" focus={false}>{copy.welcome.heading}</StepHeading>
          <Text as="p">{copy.welcome.body}</Text>
          <div className="om-onb__actions"><Button variant="primary" onClick={() => { dispatch({ type: "start" }); }}>{copy.welcome.start}</Button></div>
        </section>
      );
    case "protect_device":
      return (
        <ProtectDevice
          services={services} vault={vault} platform={platform} copy={copy.protect}
          {...(onExistingVault ? { onExistingVault } : {})}
          onProtected={(op) => { setKeyOperationId(op); dispatch({ type: "device_protected" }); }}
        />
      );
    case "recovery_key":
      return keyOperationId === null ? null : (
        <RecoveryKeyStep
          services={services} platform={platform} keyOperationId={keyOperationId} copy={copy.recovery}
          onConfirmed={() => { setKeyOperationId(null); dispatch({ type: "key_confirmed" }); }}
        />
      );
    case "invite_first":
      return (
        <>
          <InviteFirst
            services={services} platform={platform} copy={copy.invite}
            {...(renderQr ? { renderQr } : {})} {...(scanQr ? { scanQr } : {})}
            onAccepted={() => { dispatch({ type: "invite_accepted" }); }} onSkip={() => { dispatch({ type: "invites_skipped" }); }}
          />
          <HomeFacts client={client} ctx={ctx} copy={copy.home} />
        </>
      );
    case "ready":
      return (
        <section className="om-onb__step" aria-labelledby="om-onb-ready-h">
          <StepHeading id="om-onb-ready-h">{copy.ready.heading}</StepHeading>
          <Text as="p">{copy.ready.body}</Text>
          <HomeFacts client={client} ctx={ctx} copy={copy.home} />
          <div className="om-onb__actions"><Button variant="primary" onClick={onDone}>{copy.ready.done}</Button></div>
        </section>
      );
  }
}
