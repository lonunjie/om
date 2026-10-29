import { useEffect, useRef, useState } from "react";
import type { DeviceVault, OperationId, VaultErrorReason, VaultStage, VaultWrappingInfo } from "@om/platform-api";
import { firstOutcome, resolveIntent } from "../conversations/services";
import type { ConversationServices } from "../conversations/services";
import { Button } from "../shell/primitives/Button";
import { LiveRegion } from "../shell/primitives/LiveRegion";
import { Text } from "../shell/primitives/Text";
import type { OnboardingCopy } from "./copy";
import { checkNewPassphrase, passphraseBytes, PASSPHRASE_MIN_CODE_POINTS } from "./passphrase";
import { StepHeading } from "./StepHeading";

export interface ProtectDeviceProps {
  readonly services: ConversationServices;
  readonly vault: DeviceVault;
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
  | { readonly k: "profile_failed"; readonly wrapping: VaultWrappingInfo };

export function ProtectDevice({ services, vault, copy, onProtected, onExistingVault }: ProtectDeviceProps) {
  const [phase, setPhase] = useState<Phase>({ k: "idle" });
  const [usePassphrase, setUsePassphrase] = useState(false);
  const [pass, setPass] = useState("");
  const [passConfirm, setPassConfirm] = useState("");
  const [passShown, setPassShown] = useState(false); // validation message appears only after a first attempt
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const set = (p: Phase): void => { if (mounted.current) setPhase(p); };

  async function createProfile(wrapping: VaultWrappingInfo): Promise<void> {
    try {
      const r = resolveIntent(services, "create-profile", { label: copy.defaultProfileLabel });
      if (r.status !== "intent") { set({ k: "profile_failed", wrapping }); return; }
      const handle = await services.client.dispatch(r.intent);
      const outcome = await firstOutcome(services, handle.operationId);
      if (outcome.kind !== "Committed") { set({ k: "profile_failed", wrapping }); return; }
      // Straight on: nothing to read or tap between "protected" and the ready state (the summary follows there).
      if (mounted.current) onProtected(handle.operationId, wrapping);
    } catch {
      set({ k: "profile_failed", wrapping });
    }
  }

  // Runs only from a click: every ceremony needs its own user gesture, so nothing here retries on its own.
  async function protect(): Promise<void> {
    if (usePassphrase && checkNewPassphrase(pass, passConfirm) !== null) { setPassShown(true); return; }
    set({ k: "working" });
    // A fresh buffer per attempt: the vault zeroes it. The typed text leaves component state once it is encoded.
    const secret = usePassphrase ? passphraseBytes(pass) : null;
    let result;
    try {
      result = await vault.createProfileVault(services.ctx.profileId, {
        wrapping: "auto", passkeyUser: copy.passkeyUser,
        ...(secret ? { additionalFactor: { kind: "passphrase" as const, secret } } : {}),
      });
    } catch {
      set({ k: "error", reason: "crypto_failed" });
      return;
    }
    if (result.ok) { setPass(""); setPassConfirm(""); await createProfile(result.wrapping); return; }
    switch (result.reason) {
      case "needs_user_gesture": set({ k: "gesture", stage: result.stage ?? "enroll_create" }); break;
      case "already_exists":
      case "corrupt": set({ k: "existing", reason: result.reason }); break;
      default: set({ k: "error", reason: result.reason });
    }
  }
  const run = (fn: () => Promise<void>) => (): void => { void fn(); };

  const busy = phase.k === "working";
  const choosing = phase.k === "idle" || phase.k === "gesture" || phase.k === "error";
  const passProblem = usePassphrase && passShown ? checkNewPassphrase(pass, passConfirm) : null;
  return (
    <section className="om-onb__step" aria-labelledby="om-onb-protect-h">
      <StepHeading id="om-onb-protect-h">{copy.heading}</StepHeading>
      <Text as="p">{copy.body}</Text>
      {choosing || busy ? (
        <details className="om-onb__more">
          <summary>{copy.moreOptions}</summary>
          <label className="om-onb__check">
            <input type="checkbox" checked={usePassphrase} disabled={busy} onChange={(e) => { setUsePassphrase(e.target.checked); setPassShown(false); }} />
            {" "}{copy.passphrase.toggle}
          </label>
          {usePassphrase ? (
            <>
              <Text as="p" tone="secondary">{copy.passphrase.hint}</Text>
              <p role="note" className="om-onb__notice" data-code="passphrase_unrecoverable">{copy.passphrase.warning}</p>
              <div className="om-onb__field">
                <label htmlFor="om-pass-new">{copy.passphrase.label}</label>
                <input id="om-pass-new" className="om-onb__input" type="password" autoComplete="new-password" spellCheck={false} autoCapitalize="none"
                  value={pass} disabled={busy} onChange={(e) => { setPass(e.target.value); }} />
              </div>
              <div className="om-onb__field">
                <label htmlFor="om-pass-confirm">{copy.passphrase.confirmLabel}</label>
                <input id="om-pass-confirm" className="om-onb__input" type="password" autoComplete="new-password" spellCheck={false} autoCapitalize="none"
                  value={passConfirm} disabled={busy} onChange={(e) => { setPassConfirm(e.target.value); }} />
              </div>
              <Text as="p" tone="secondary">{copy.passphrase.slowNote}</Text>
              {passProblem === "mismatch" ? <p role="alert" data-code="passphrase_mismatch">{copy.passphrase.mismatch}</p> : null}
              {passProblem === "too_short" || passProblem === "too_long" ? <p role="alert" data-code={`passphrase_${passProblem}`}>{copy.passphrase.tooShort(PASSPHRASE_MIN_CODE_POINTS)}</p> : null}
            </>
          ) : null}
        </details>
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
      </div>
    </section>
  );
}
