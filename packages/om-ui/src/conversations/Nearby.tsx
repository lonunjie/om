import { useEffect, useId, useState } from "react";
import { Text } from "../shell/primitives/Text";
import type { ConversationCopy } from "./copy";
import type { NearbyService, NearbyView } from "./nearby";

type Shown = { readonly phase: "loading" } | { readonly phase: "ready"; readonly view: NearbyView } | { readonly phase: "failed" };

/**
 * The switch for "Find each other on the same Wi-Fi" in the contact sheet. It renders what Core reports: the person's choice is
 * never assumed from the tap, a refusal leaves the switch where Core says it is, and a state that cannot be changed says why in
 * words (never by colour alone).
 */
export function NearbyRow({ service, conversationId, copy }: { readonly service: NearbyService; readonly conversationId: string; readonly copy: ConversationCopy["contact"]["nearby"] }) {
  const [shown, setShown] = useState<Shown>({ phase: "loading" });
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const hintId = useId();
  useEffect(() => {
    let live = true;
    service.state(conversationId).then((view) => { if (live) setShown({ phase: "ready", view }); }, () => { if (live) setShown({ phase: "failed" }); });
    return () => { live = false; };
  }, [service, conversationId]);

  if (shown.phase === "loading") return null;
  const view = shown.phase === "ready" ? shown.view : null;
  const usable = view !== null && service.platform !== "unavailable" && (view.available || view.enabled);
  const reason = service.platform === "unavailable" ? copy.platformUnavailable : view === null ? copy.failed : view.available || view.enabled ? null : copy.notNow;
  const change = (on: boolean): void => {
    setBusy(true);
    setFailed(false);
    service.set(conversationId, on).then((next) => { setShown({ phase: "ready", view: next }); }, () => { setFailed(true); }).finally(() => { setBusy(false); });
  };
  return (
    <div className="om-contact__nearby">
      <label className="om-contact__calls">
        <input type="checkbox" role="switch" checked={view?.enabled === true} disabled={busy || !usable} aria-describedby={hintId} onChange={(e) => { change(e.target.checked); }} />
        {" "}<Text as="span" weight="bold">{copy.label}</Text>
      </label>
      <Text as="p" size="sm" tone="secondary" id={hintId}>{copy.hint}</Text>
      {service.platform === "partial" && usable ? <Text as="p" size="sm" tone="secondary">{copy.partial}</Text> : null}
      {reason === null ? <Text as="p" size="sm" role="status">{view?.enabled === true ? copy.on : copy.off}</Text> : <Text as="p" size="sm" tone="secondary" role="status">{reason}</Text>}
      {failed ? <p className="om-settings__status" role="alert">{copy.failed}</p> : null}
    </div>
  );
}
