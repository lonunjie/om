import { useReducer, useState } from "react";
import type { ReactNode } from "react";
import type { CoreClient, DeviceVault, OperationId, ProfileExecutionContext, VaultWrappingInfo } from "@om/platform-api";
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
import { ProtectionSummary } from "./ProtectionSummary";
import { StepHeading } from "./StepHeading";
import "./onboarding.css";

export interface OnboardingResult { readonly safetyCodePending: OperationId | null }

export interface OnboardingFlowProps {
  readonly client: CoreClient;
  readonly registry: ActionRegistry;
  /** Context of the not-yet-onboarded profile this flow creates the vault and identity for. */
  readonly ctx: ProfileExecutionContext;
  readonly vault: DeviceVault;
  readonly platform: OnboardingPlatform;
  readonly copy: OnboardingCopy;
  /**
   * The flow is over. `safetyCodePending` names the display-once safety code Core still holds (ADR-0111: recovery is on by
   * default and the code is saved on the user's own time); the host offers it later with `RecoveryKeyStep`.
   */
  readonly onDone: (result: OnboardingResult) => void;
  /** Called when vault data already exists or is corrupt; the host opens its recovery flow. */
  readonly onExistingVault?: (reason: "already_exists" | "corrupt") => void;
  readonly renderQr?: (payload: string) => ReactNode;
  readonly scanQr?: () => Promise<string | null>;
  /** Test seam: bound on waiting for a provider in the invite step. */
  readonly settleTimeoutMs?: number;
  /** The host already showed its own introduction (the landing page): start at "protect device". */
  readonly skipWelcome?: boolean;
}

export function OnboardingFlow({ client, registry, ctx, vault, platform, copy, onDone, onExistingVault, renderQr, scanQr, settleTimeoutMs, skipWelcome = false }: OnboardingFlowProps) {
  const [flow, dispatch] = useReducer(flowReducer, skipWelcome ? flowReducer(initialFlow, { type: "start" }) : initialFlow);
  const [protectedAs, setProtectedAs] = useState<{ readonly keyOperationId: OperationId; readonly wrapping: VaultWrappingInfo } | null>(null);
  const finish = (): void => { onDone({ safetyCodePending: protectedAs?.keyOperationId ?? null }); };
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
          services={services} vault={vault} copy={copy.protect}
          {...(onExistingVault ? { onExistingVault } : {})}
          onProtected={(op, wrapping) => { setProtectedAs({ keyOperationId: op, wrapping }); dispatch({ type: "device_protected" }); }}
        />
      );
    case "invite_first":
      return (
        <>
          <InviteFirst
            services={services} platform={platform} copy={copy.invite}
            {...(renderQr ? { renderQr } : {})} {...(scanQr ? { scanQr } : {})} {...(settleTimeoutMs !== undefined ? { settleTimeoutMs } : {})}
            onAccepted={finish} onSkip={finish}
          />
          {protectedAs ? <ProtectionSummary wrapping={protectedAs.wrapping} platform={platform} copy={copy.protect} /> : null}
          <HomeFacts client={client} ctx={ctx} copy={copy.home} />
        </>
      );
  }
}
