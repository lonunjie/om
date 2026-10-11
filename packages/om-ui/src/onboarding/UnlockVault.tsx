import { useEffect, useRef, useState } from "react";
import type { DeviceVault, ProfileId, VaultErrorReason, VaultStage } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { LiveRegion } from "../shell/primitives/LiveRegion";
import { Text } from "../shell/primitives/Text";
import type { UnlockCopy } from "./copy";
import { passphraseBytes } from "./passphrase";
import { StepHeading } from "./StepHeading";

export interface UnlockVaultProps {
  readonly vault: DeviceVault;
  readonly profileId: ProfileId;
  readonly copy: UnlockCopy;
  readonly onUnlocked: () => void;
  /** Missing, damaged or evicted vault data: the only way forward is the recovery flow. */
  readonly onNeedsRecovery?: () => void;
}

type Phase =
  | { readonly k: "loading" }
  | { readonly k: "idle" }
  | { readonly k: "working" }
  | { readonly k: "gesture"; readonly stage: VaultStage }
  | { readonly k: "error"; readonly reason: VaultErrorReason };

const NEEDS_RECOVERY: readonly VaultErrorReason[] = ["not_found", "corrupt"];

/**
 * Unlock screen. Runs a vault ceremony only from a click (each prompt needs its own user gesture) and asks for the
 * passphrase only when the vault says one is set. The passphrase leaves component state as soon as it is sent.
 */
export function UnlockVault({ vault, profileId, copy, onUnlocked, onNeedsRecovery }: UnlockVaultProps) {
  const [phase, setPhase] = useState<Phase>({ k: "loading" });
  const [needsPassphrase, setNeedsPassphrase] = useState(false);
  const [passphrase, setPassphrase] = useState("");
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const set = (p: Phase): void => { if (mounted.current) setPhase(p); };

  useEffect(() => {
    let live = true;
    vault.wrapping(profileId).then(
      (r) => {
        if (!live) return;
        if (r.ok) { setNeedsPassphrase(r.wrapping.passphraseProtected); setPhase({ k: "idle" }); }
        else setPhase({ k: "error", reason: r.reason });
      },
      () => { if (live) setPhase({ k: "error", reason: "storage_failed" }); },
    );
    return () => { live = false; };
  }, [vault, profileId]);

  async function unlock(): Promise<void> {
    if (needsPassphrase && passphrase === "") { set({ k: "error", reason: "passphrase_required" }); return; }
    set({ k: "working" });
    const secret = needsPassphrase ? passphraseBytes(passphrase) : null;
    setPassphrase("");
    let result;
    try {
      result = await vault.unlock(profileId, secret === null ? undefined : { passphrase: { kind: "passphrase", secret } });
    } catch {
      set({ k: "error", reason: "crypto_failed" });
      return;
    }
    if (result.ok) { if (mounted.current) onUnlocked(); return; }
    if (result.reason === "needs_user_gesture") set({ k: "gesture", stage: result.stage ?? "unlock" });
    else set({ k: "error", reason: result.reason });
  }

  const busy = phase.k === "working";
  const ready = phase.k === "idle" || phase.k === "error" || phase.k === "gesture";
  return (
    <section className="om-onb__step" aria-labelledby="om-unlock-h">
      <StepHeading id="om-unlock-h">{copy.heading}</StepHeading>
      <Text as="p">{copy.body}</Text>
      {needsPassphrase && phase.k !== "loading" ? (
        <div className="om-onb__field">
          <label htmlFor="om-unlock-pass">{copy.passphraseLabel}</label>
          <input
            id="om-unlock-pass" className="om-onb__input" type="password" autoComplete="current-password" spellCheck={false} autoCapitalize="none"
            value={passphrase} disabled={busy} onChange={(e) => { setPassphrase(e.target.value); }}
          />
        </div>
      ) : null}
      {phase.k === "gesture" ? <p role="status">{copy.needsGesture[phase.stage]}</p> : null}
      {phase.k === "error" ? <p role="alert" data-reason={phase.reason}>{copy.errors[phase.reason]}</p> : null}
      {busy ? <p role="status">{needsPassphrase ? copy.workingSlow : copy.working}</p> : null}
      <LiveRegion>{busy ? copy.working : ""}</LiveRegion>
      <div className="om-onb__actions">
        {ready || busy ? (
          <Button variant="primary" disabled={busy} onClick={() => { void unlock(); }}>
            {phase.k === "gesture" ? copy.continueCeremony : copy.unlock}
          </Button>
        ) : null}
        {phase.k === "error" && NEEDS_RECOVERY.includes(phase.reason) && onNeedsRecovery ? <Button onClick={onNeedsRecovery}>{copy.openRecovery}</Button> : null}
      </div>
    </section>
  );
}
