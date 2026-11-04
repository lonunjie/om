import { useEffect, useRef, useState } from "react";
import { parseDeviceLink } from "@om/platform-api";
import type { OperationId } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { JoinCopy } from "./copy";
import { FocusHeading } from "./FocusHeading";
import { OperationStatus, useLiveQuery, useOperation, type SettingsServices } from "./operation";
import "./devices.css";

export interface JoinWithLinkProps {
  readonly services: SettingsServices;
  readonly copy: JoinCopy;
  /** Opens the camera only from the click. Without it, typing or pasting the link is the only path. */
  readonly scanQr?: () => Promise<string | null>;
  /** The other device approved and this one is linked: the host carries on into the app. */
  readonly onLinked: () => void;
  /** The person turned back before asking, or gave up after a refusal or a timeout. */
  readonly onCancel: () => void;
}

/**
 * New device (deviation D24): type or scan the link the other device shows, then show the same check code and wait. This
 * device never approves itself; only the other device's decision (reported by Core) moves it on.
 */
export function JoinWithLink({ services, copy, scanQr, onLinked, onCancel }: JoinWithLinkProps) {
  const [pasted, setPasted] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const op = useOperation(services);
  const asked: OperationId | null = op.state.phase === "observing" && op.state.outcome?.kind !== "Rejected" ? op.state.operationId : null;

  const ask = (text: string): void => {
    const parsed = parseDeviceLink(text);
    if (!parsed.ok) { setNote(copy.parseError[parsed.reason]); return; }
    setNote(null);
    void op.run("request-device-link", { link: text });
  };
  const scan = async (): Promise<void> => {
    if (!scanQr) return;
    const payload = await scanQr();
    if (payload === null) { setNote(copy.scanNothing); return; }
    ask(payload);
  };

  if (asked !== null) return <Waiting services={services} copy={copy} operationId={asked} onLinked={onLinked} onCancel={onCancel} />;
  return (
    <section className="om-settings__step" aria-labelledby="om-join-h">
      <FocusHeading id="om-join-h">{copy.heading}</FocusHeading>
      <Text as="p">{copy.body}</Text>
      {scanQr ? <div className="om-settings__actions"><Button variant="primary" disabled={op.locked} onClick={() => { void scan(); }}>{copy.scan}</Button></div> : null}
      <form className="om-settings__form" onSubmit={(e) => { e.preventDefault(); ask(pasted); }}>
        <label className="om-settings__field">
          <span>{copy.pasteLabel}</span>
          <input
            type="text" className="om-settings__input" value={pasted} aria-describedby="om-join-hint" inputMode="url"
            autoComplete="off" autoCapitalize="off" autoCorrect="off" spellCheck={false}
            onChange={(e) => { setPasted(e.target.value); }}
          />
        </label>
        <Text as="p" size="sm" tone="secondary" id="om-join-hint">{copy.pasteHint}</Text>
        <div className="om-settings__actions">
          <Button type="submit" variant={scanQr ? "secondary" : "primary"} disabled={op.locked}>{copy.open}</Button>
          <Button variant="quiet" onClick={onCancel}>{copy.cancel}</Button>
        </div>
      </form>
      {note === null ? null : <p role="alert">{note}</p>}
      <OperationStatus state={op.state} copy={copy.outcome} done="" />
    </section>
  );
}

function Waiting({ services, copy, operationId, onLinked, onCancel }: { readonly services: SettingsServices; readonly copy: JoinCopy; readonly operationId: OperationId; readonly onLinked: () => void; readonly onCancel: () => void }) {
  const live = useLiveQuery(services, { kind: "GetLinkRequest", ctx: services.ctx, operationId }, `join-${operationId}`);
  const request = live.status === "ready" ? live.value.value : null;
  const linked = request?.state === "approved";
  const told = useRef(false);
  useEffect(() => {
    if (linked && !told.current) { told.current = true; onLinked(); }
  }, [linked, onLinked]);
  if (live.status === "failed") return <Text role="alert">{copy.outcome.failed}</Text>;
  if (request === null) return <Text role="status">{copy.outcome.working}</Text>;
  const over = request.state === "rejected" || request.state === "expired";
  return (
    <section className="om-settings__step" aria-labelledby="om-join-h">
      <FocusHeading id="om-join-h">{copy.heading}</FocusHeading>
      <div className="om-devices__code" role="group" aria-labelledby="om-join-code-l">
        <Text as="p" size="sm" weight="bold" id="om-join-code-l">{copy.codeLabel}</Text>
        <output className="om-devices__digits" translate="no">{request.verificationCode}</output>
        <Text as="p" size="sm" tone="secondary">{copy.codeHint}</Text>
      </div>
      <p className="om-devices__state" role={over ? "alert" : "status"} data-state={request.state}>{linked ? copy.joined : copy.state[request.state]}</p>
      {over ? <div className="om-settings__actions"><Button variant="primary" onClick={onCancel}>{copy.tryAgain}</Button></div> : null}
    </section>
  );
}
