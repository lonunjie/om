import { useReducer, useState } from "react";
import type { ReactNode } from "react";
import type { CoreClient, DeviceVault, OperationId, ProfileExecutionContext, VaultWrappingInfo } from "@om/platform-api";
import type { ActionRegistry } from "../actions/registry";
import type { ConversationServices } from "../conversations/services";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { OnboardingCopy } from "./copy";
import { JoinWithLink } from "../devices/JoinWithLink";
import { choosingFlow, flowReducer, initialFlow } from "./flow";
import { FirstRunChoice } from "./FirstRunChoice";
import { HomeFacts } from "./HomeFacts";
import { AcceptInvite } from "./AcceptInvite";
import { InviteFirst } from "./InviteFirst";
import { OnboardingFrame } from "./OnboardingFrame";
import type { OnboardingPlatform } from "./platform";
import { ProtectDevice } from "./ProtectDevice";
import { ProtectionSummary } from "./ProtectionSummary";
import { StepHeading } from "./StepHeading";
import "./onboarding.css";

export interface OnboardingResult {
  readonly safetyCodePending: OperationId | null;
  /** An invitation was accepted during setup (the other person still has to approve it). */
  readonly inviteAccepted: boolean;
}

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
  /** The same camera, with the wording for a device-link code ("I already use OM"). Without it the link is typed. */
  readonly scanLinkQr?: () => Promise<string | null>;
  /** See `InviteFirst`: the form of an invite that is shown, shared and put in the QR code. */
  readonly shareLink?: (coreLink: string) => string;
  /** The token of an invitation link this person opened to get here: the last step offers to accept it instead of inviting. */
  readonly incomingInvite?: string | null;
  /** Called when the person turns the opened invitation down. */
  readonly onIncomingInviteDeclined?: () => void;
  /** Test seam: bound on waiting for a provider in the invite step. */
  readonly settleTimeoutMs?: number;
  /** The host already showed its own introduction (the landing page): start at "protect device". */
  readonly skipWelcome?: boolean;
  /** Test seam: see `ProtectDevice`. */
  readonly slowPromptMs?: number;
  /** Adding a further identity from inside the app: a way back out, and no second password choice. Omit on first run. */
  readonly addingIdentity?: { readonly backLabel: string; readonly onBack: () => void };
  /**
   * The host can link a device (Core and a delivery service for the ceremony): first run opens with "Start new" and "I already use OM"
   * (deviation D24). Needs `copy.join`. Not offered when adding a further identity or opening an invitation.
   */
  readonly allowJoin?: boolean;
  /** The join was declined or timed out, or the person gave up: the host starts first run over with a fresh, empty profile. */
  readonly onJoinRestart?: () => void;
}

export function OnboardingFlow({ client, registry, ctx, vault, platform, copy, onDone, onExistingVault, renderQr, scanQr, scanLinkQr, shareLink, incomingInvite = null, onIncomingInviteDeclined, settleTimeoutMs, skipWelcome = false, slowPromptMs, addingIdentity, allowJoin = false, onJoinRestart }: OnboardingFlowProps) {
  const joinCopy = allowJoin && addingIdentity === undefined && incomingInvite === null ? copy.join : undefined;
  const [flow, dispatch] = useReducer(flowReducer, joinCopy !== undefined ? choosingFlow : skipWelcome ? flowReducer(initialFlow, { type: "start" }) : initialFlow);
  const [protectedAs, setProtectedAs] = useState<{ readonly keyOperationId: OperationId; readonly wrapping: VaultWrappingInfo } | null>(null);
  const [declined, setDeclined] = useState(false);
  const finish = (inviteAccepted = false): void => { onDone({ safetyCodePending: protectedAs?.keyOperationId ?? null, inviteAccepted }); };
  const services: ConversationServices = { client, registry, ctx };

  const total = skipWelcome ? 2 : 3;
  const frame = (n: number, children: ReactNode): ReactNode => (
    <OnboardingFrame copy={copy.frame} stepKey={flow.step} progress={{ n, total }} {...(addingIdentity !== undefined && flow.step === "protect_device" ? { back: { label: addingIdentity.backLabel, onBack: addingIdentity.onBack } } : {})}>{children}</OnboardingFrame>
  );

  switch (flow.step) {
    case "welcome":
      return frame(1, (
        <section className="om-onb__step" aria-labelledby="om-onb-welcome-h">
          <StepHeading id="om-onb-welcome-h" focus={false}>{copy.welcome.heading}</StepHeading>
          <Text as="p">{copy.welcome.body}</Text>
          <div className="om-onb__actions"><Button variant="primary" onClick={() => { dispatch({ type: "start" }); }}>{copy.welcome.start}</Button></div>
        </section>
      ));
    case "choose":
      return joinCopy === undefined ? null : (
        <OnboardingFrame copy={copy.frame} stepKey="choose">
          <FirstRunChoice copy={joinCopy} onStartNew={() => { dispatch({ type: "choose_new" }); }} onUseExisting={() => { dispatch({ type: "choose_join" }); }} />
        </OnboardingFrame>
      );
    case "join_protect": {
      if (joinCopy === undefined) return null;
      // No name is asked: the account being joined already has one. The same device-lock ceremony, but no identity is created here.
      const protect = Object.fromEntries(Object.entries(copy.protect).filter(([key]) => key !== "name")) as Omit<typeof copy.protect, "name">;
      const joining = { ...protect, heading: joinCopy.protectHeading, body: joinCopy.protectBody, action: joinCopy.protectAction };
      return (
        <OnboardingFrame copy={copy.frame} stepKey={flow.step} progress={{ n: 1, total: 2 }}>
          <ProtectDevice
            services={services} vault={vault} copy={joining} allowPassphrase
            {...(onExistingVault ? { onExistingVault } : {})} {...(slowPromptMs !== undefined ? { slowPromptMs } : {})}
            onProtected={() => undefined} onVaultReady={() => { dispatch({ type: "vault_ready" }); }}
          />
          <div className="om-onb__actions"><Button variant="quiet" onClick={() => { dispatch({ type: "back_to_choice" }); }}>{joinCopy.back}</Button></div>
        </OnboardingFrame>
      );
    }
    case "join_link":
      return joinCopy === undefined ? null : (
        <OnboardingFrame copy={copy.frame} stepKey={flow.step} progress={{ n: 2, total: 2 }}>
          <JoinWithLink
            services={services} copy={joinCopy.link} {...(scanLinkQr ? { scanQr: scanLinkQr } : {})}
            onLinked={() => { onDone({ safetyCodePending: null, inviteAccepted: false }); }}
            onCancel={() => { onJoinRestart?.(); }}
          />
        </OnboardingFrame>
      );
    case "protect_device":
      return frame(total - 1, (
        <ProtectDevice
          services={services} vault={vault} copy={copy.protect} allowPassphrase={addingIdentity === undefined}
          {...(onExistingVault ? { onExistingVault } : {})} {...(slowPromptMs !== undefined ? { slowPromptMs } : {})}
          onProtected={(op, wrapping) => { setProtectedAs({ keyOperationId: op, wrapping }); dispatch({ type: "device_protected" }); }}
        />
      ));
    case "invite_first":
      return frame(total, (
        <>
          {incomingInvite !== null && !declined ? (
            <AcceptInvite
              services={services} copy={copy.incoming} token={incomingInvite} {...(settleTimeoutMs !== undefined ? { settleTimeoutMs } : {})}
              onAccepted={() => { finish(true); }} onDecline={() => { setDeclined(true); onIncomingInviteDeclined?.(); }}
            />
          ) : (
            <InviteFirst
              services={services} platform={platform} copy={copy.invite}
              {...(renderQr ? { renderQr } : {})} {...(scanQr ? { scanQr } : {})} {...(shareLink ? { shareLink } : {})} {...(settleTimeoutMs !== undefined ? { settleTimeoutMs } : {})}
              onAccepted={() => { finish(true); }} onSkip={() => { finish(); }}
            />
          )}
          <aside className="om-onb__aside">
            {protectedAs ? <ProtectionSummary wrapping={protectedAs.wrapping} platform={platform} copy={copy.protect} /> : null}
            <HomeFacts client={client} ctx={ctx} copy={copy.home} />
          </aside>
        </>
      ));
  }
}
