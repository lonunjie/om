import { useEffect, useRef, useState } from "react";
import { describeLimits } from "@om/platform-api";
import type { DeviceVault, OperationId, VaultErrorReason, VaultStage, VaultWrappingInfo } from "@om/platform-api";
import { firstOutcome, resolveIntent } from "../conversations/services";
import type { ConversationServices } from "../conversations/services";
import { Button } from "../shell/primitives/Button";
import { LiveRegion } from "../shell/primitives/LiveRegion";
import { Text } from "../shell/primitives/Text";
import type { OnboardingCopy } from "./copy";
import type { OnboardingPlatform } from "./platform";
import { StepHeading } from "./StepHeading";

export interface ProtectDeviceProps {
  readonly services: ConversationServices;
  readonly vault: DeviceVault;
  readonly platform: OnboardingPlatform;
  readonly copy: OnboardingCopy["protect"];
  /** Core created the identity; `keyOperationId` names the RecoveryKeyView to show next. */
  readonly onProtected: (keyOperationId: OperationId, wrapping: VaultWrappingInfo) => void;
  /** Existing or corrupt vault data: the only way forward is the recovery flow, never overwrite or recreate. */
  readonly onExistingVault?: (reason: "already_exists" | "corrupt") => void;
}

type Phase =
  | { readonly k: "idle" }
  | { readonly k: "working" }
  | { readonly k: "gesture"; readonly stage: VaultStage }
  | { readonly k: "error"; readonly reason: VaultErrorReason }
  | { readonly k: "existing"; readonly reason: "already_exists" | "corrupt" }
  | { readonly k: "profile_failed"; readonly wrapping: VaultWrappingInfo }
  | { readonly k: "done"; readonly wrapping: VaultWrappingInfo; readonly keyOperationId: OperationId };

export function ProtectDevice({ services, vault, platform, copy, onProtected, onExistingVault }: ProtectDeviceProps) {
  const [phase, setPhase] = useState<Phase>({ k: "idle" });
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const set = (p: Phase): void => { if (mounted.current) setPhase(p); };

  async function createProfile(wrapping: VaultWrappingInfo): Promise<void> {
    try {
      const r = resolveIntent(services, "create-profile", { label: copy.defaultProfileLabel });
      if (r.status !== "intent") { set({ k: "profile_failed", wrapping }); return; }
      const handle = await services.client.dispatch(r.intent);
      const outcome = await firstOutcome(services, handle.operationId);
      set(outcome.kind === "Committed" ? { k: "done", wrapping, keyOperationId: handle.operationId } : { k: "profile_failed", wrapping });
    } catch {
      set({ k: "profile_failed", wrapping });
    }
  }

  // Runs only from a click: every ceremony needs its own user gesture, so nothing here retries on its own.
  async function protect(): Promise<void> {
    set({ k: "working" });
    let result;
    try {
      result = await vault.createProfileVault(services.ctx.profileId, { wrapping: "auto", passkeyUser: copy.passkeyUser });
    } catch {
      set({ k: "error", reason: "crypto_failed" });
      return;
    }
    if (result.ok) { await createProfile(result.wrapping); return; }
    switch (result.reason) {
      case "needs_user_gesture": set({ k: "gesture", stage: result.stage ?? "enroll_create" }); break;
      case "already_exists":
      case "corrupt": set({ k: "existing", reason: result.reason }); break;
      default: set({ k: "error", reason: result.reason });
    }
  }
  const run = (fn: () => Promise<void>) => (): void => { void fn(); };

  const busy = phase.k === "working";
  const wrapping = phase.k === "done" || phase.k === "profile_failed" ? phase.wrapping : null;
  return (
    <section className="om-onb__step" aria-labelledby="om-onb-protect-h">
      <StepHeading id="om-onb-protect-h">{copy.heading}</StepHeading>
      <Text as="p">{copy.body}</Text>
      {wrapping ? (
        <div className="om-onb__result">
          <Text as="h3" size="md" weight="bold">{copy.resultHeading}</Text>
          <Text as="p">{copy.method[wrapping.method]}</Text>
          <Text as="p" tone="secondary">{copy.reason[wrapping.reason]}</Text>
          {wrapping.reducedAtRestProperty ? <p role="note" className="om-onb__notice" data-code="reduced_at_rest">{copy.reducedAtRest}</p> : null}
          {wrapping.method === "device-key" ? (
            <div role="note" className="om-onb__notice" data-code="device_key_limit">
              <p>{copy.deviceKeyLimit}</p>
              <p>{copy.nativeRecommended}</p>
              <Text as="h4" size="sm" weight="bold">{copy.limitsHeading}</Text>
              <ul>{describeLimits(platform.capabilities()).map((c) => <li key={c}>{copy.limits[c]}</li>)}</ul>
            </div>
          ) : null}
        </div>
      ) : null}
      {phase.k === "gesture" ? <p role="status">{copy.needsGesture[phase.stage]}</p> : null}
      {phase.k === "error" ? <p role="alert" data-reason={phase.reason}>{copy.errors[phase.reason]}</p> : null}
      {phase.k === "existing" ? <p role="alert" data-reason={phase.reason}>{copy.existing[phase.reason]}</p> : null}
      {phase.k === "profile_failed" ? <p role="alert">{copy.profileFailed}</p> : null}
      <LiveRegion>{busy ? copy.working : ""}</LiveRegion>
      <div className="om-onb__actions">
        {phase.k === "idle" || phase.k === "working" ? <Button variant="primary" disabled={busy} onClick={run(protect)}>{busy ? copy.working : copy.action}</Button> : null}
        {phase.k === "gesture" ? <Button variant="primary" onClick={run(protect)}>{copy.continueCeremony}</Button> : null}
        {phase.k === "error" ? <Button variant="primary" onClick={run(protect)}>{copy.retry}</Button> : null}
        {phase.k === "profile_failed" ? <Button variant="primary" onClick={run(() => createProfile(phase.wrapping))}>{copy.retryProfile}</Button> : null}
        {phase.k === "existing" && onExistingVault ? <Button variant="primary" onClick={() => { onExistingVault(phase.reason); }}>{copy.openRecovery}</Button> : null}
        {phase.k === "done" ? <Button variant="primary" onClick={() => { onProtected(phase.keyOperationId, phase.wrapping); }}>{copy.next}</Button> : null}
      </div>
    </section>
  );
}
