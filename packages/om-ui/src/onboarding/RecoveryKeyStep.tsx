import { useEffect, useMemo, useRef, useState } from "react";
import type { OperationId } from "@om/platform-api";
import { firstOutcome, resolveIntent } from "../conversations/services";
import type { ConversationServices } from "../conversations/services";
import { Button } from "../shell/primitives/Button";
import { LiveRegion } from "../shell/primitives/LiveRegion";
import { Text } from "../shell/primitives/Text";
import { VisuallyHidden } from "../shell/primitives/VisuallyHidden";
import type { OnboardingCopy } from "./copy";
import type { OnboardingPlatform } from "./platform";
import { groupMatches, pickVerifyIndices } from "./recovery";
import { StepHeading } from "./StepHeading";

export interface RecoveryKeyStepProps {
  readonly services: ConversationServices;
  readonly platform: OnboardingPlatform;
  readonly keyOperationId: OperationId;
  readonly copy: OnboardingCopy["recovery"];
  readonly onConfirmed: () => void;
}

type Load = { readonly k: "loading" } | { readonly k: "unavailable" } | { readonly k: "ready"; readonly groups: readonly string[] };
type Phase = "show" | "verify" | "confirming";
type CopyState = "idle" | "copied" | "failed";

/**
 * The key lives only in this component's React state: never in storage, logs, the announcement or an attribute.
 * It is dropped (state cleared) once Core accepts ConfirmRecoveryKeyStored, and Core will not return it again.
 */
export function RecoveryKeyStep({ services, platform, keyOperationId, copy, onConfirmed }: RecoveryKeyStepProps) {
  const [load, setLoad] = useState<Load>({ k: "loading" });
  const [phase, setPhase] = useState<Phase>("show");
  const [typed, setTyped] = useState<readonly string[]>([]);
  const [copyState, setCopyState] = useState<CopyState>("idle");
  const [error, setError] = useState(false);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);

  useEffect(() => {
    let alive = true;
    services.client.query({ kind: "GetRecoveryKey", ctx: services.ctx, keyOperationId }).then(
      (v) => { if (alive) setLoad(v ? { k: "ready", groups: v.groups } : { k: "unavailable" }); },
      () => { if (alive) setLoad({ k: "unavailable" }); },
    );
    return () => { alive = false; };
  }, [services.client, services.ctx, keyOperationId]);

  const groups = load.k === "ready" ? load.groups : null;
  const indices = useMemo(() => (groups ? pickVerifyIndices(groups.length) : []), [groups]);

  // Copy goes through the Action Registry's sensitive-clipboard request, and only when this platform honors `sensitive`.
  const clipboard = platform.clipboard();
  const report = platform.capabilities().families.clipboard;
  const canCopy = clipboard.state === "available" && report.detail !== "sensitive_unsupported";
  const doCopy = async (): Promise<void> => {
    if (!groups || clipboard.state === "unavailable") return;
    const r = services.registry.resolve({ actionId: "copy-text", params: { sensitive: true }, ctx: services.ctx, origin: "direct" });
    if (r.status !== "platform" || r.request.capability !== "Clipboard" || !r.request.sensitive) { setCopyState("failed"); return; }
    try {
      const res = await clipboard.api.write(groups.join("-"), { sensitive: true });
      if (mounted.current) setCopyState(res === "written" ? "copied" : "failed");
    } catch {
      if (mounted.current) setCopyState("failed");
    }
  };

  const verified = groups !== null && indices.length > 0 && indices.every((gi, i) => groupMatches(typed[i] ?? "", groups[gi] ?? ""));

  async function confirm(): Promise<void> {
    setPhase("confirming");
    setError(false);
    try {
      const r = resolveIntent(services, "confirm-recovery-key-stored", { keyOperationId });
      if (r.status !== "intent") throw new Error("blocked");
      const handle = await services.client.dispatch(r.intent);
      const outcome = await firstOutcome(services, handle.operationId);
      if (!mounted.current) return;
      if (outcome.kind !== "Committed") throw new Error("not committed");
      setLoad({ k: "unavailable" });
      setTyped([]);
      onConfirmed();
    } catch {
      if (mounted.current) { setPhase("verify"); setError(true); }
    }
  }

  const total = groups?.length ?? 0;
  return (
    <section className="om-onb__step" aria-labelledby="om-onb-recovery-h">
      <StepHeading key={phase === "show" ? "show" : "verify"} id="om-onb-recovery-h">{phase === "show" ? copy.heading : copy.verifyHeading}</StepHeading>
      <LiveRegion>{phase === "show" && groups ? copy.shownAnnouncement : ""}</LiveRegion>
      {load.k === "loading" ? <Text as="p">{copy.loading}</Text> : null}
      {load.k === "unavailable" && phase !== "confirming" ? <p role="alert">{copy.unavailable}</p> : null}
      {groups && phase === "show" ? (
        <>
          <Text as="p">{copy.body}</Text>
          <ol className="om-onb__key" aria-label={copy.keyLabel} data-sensitive="true" translate="no">
            {groups.map((g, i) => <li key={String(i)}><VisuallyHidden>{copy.group(i + 1, total)} </VisuallyHidden>{g}</li>)}
          </ol>
          {canCopy ? (
            <Button onClick={() => { void doCopy(); }}>{copy.copy}</Button>
          ) : (
            <p role="note" data-code="clipboard_sensitive_unavailable">{copy.copyUnavailable}</p>
          )}
          {copyState === "copied" ? <p role="status">{copy.copied}</p> : null}
          {copyState === "failed" ? <p role="alert">{copy.copyFailed}</p> : null}
          <div className="om-onb__actions">
            <Button variant="primary" onClick={() => { setTyped([]); setPhase("verify"); }}>{copy.toVerify}</Button>
          </div>
        </>
      ) : null}
      {groups && phase !== "show" ? (
        <>
          <Text as="p">{copy.verifyBody}</Text>
          {indices.map((gi, i) => (
            <label key={gi} className="om-onb__field">
              <span>{copy.verifyField(gi + 1)}</span>
              <input
                type="text" className="om-onb__input" value={typed[i] ?? ""} disabled={phase === "confirming"}
                autoComplete="off" autoCapitalize="off" autoCorrect="off" spellCheck={false}
                onChange={(e) => { const v = e.target.value; setTyped((t) => { const n = [...t]; n[i] = v; return n; }); }}
              />
            </label>
          ))}
          {error ? <p role="alert">{copy.confirmFailed}</p> : null}
          <div className="om-onb__actions">
            <Button variant="primary" disabled={!verified || phase === "confirming"} onClick={() => { void confirm(); }}>{phase === "confirming" ? copy.confirming : copy.saved}</Button>
            <Button variant="quiet" disabled={phase === "confirming"} onClick={() => { setPhase("show"); }}>{copy.showAgain}</Button>
          </div>
        </>
      ) : null}
    </section>
  );
}
