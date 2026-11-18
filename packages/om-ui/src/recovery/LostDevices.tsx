import { useEffect, useRef, useState } from "react";
import type { ConversationId, ReconnectContactView, RecoveringView } from "@om/platform-api";
import { RECOVERY_APPROVAL_LINK } from "../actions/registry";
import { FocusHeading } from "../devices/FocusHeading";
import { OperationStatus, useLiveQuery, useOperation, type SettingsServices } from "../devices/operation";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { LostDevicesCopy, ReconnectCopy } from "./lostCopy";
import "../devices/devices.css";

export interface LostDevicesProps {
  readonly services: SettingsServices;
  readonly copy: LostDevicesCopy;
  /** Opens the camera only from the click. Without it, typing or pasting the key is the only path. */
  readonly scanKey?: () => Promise<string | null>;
  /** Same, for the approval link another device shows. */
  readonly scanLink?: () => Promise<string | null>;
  /** The account is back on this device: the host carries on into the app. */
  readonly onRecovered: () => void;
  /** The person turned back, or gave up after the request ended. */
  readonly onCancel: () => void;
}

/**
 * New device after losing every device. Core keeps the key and the attempt (restricted: this device cannot send as the account);
 * this screen asks, shows what Core reports and never decides anything itself: the wait is this device's own timer, a surviving
 * device can approve, and only Core's verification brings the account back (`onRecovered` fires when the restricted state is gone).
 */
export function LostDevices({ services, copy, scanKey, scanLink, onRecovered, onCancel }: LostDevicesProps) {
  const live = useLiveQuery(services, { kind: "GetRecoveryStatus", ctx: services.ctx }, "lost-devices");
  const seen = useRef(false);
  const told = useRef(false);
  const recovering = live.status === "ready" ? live.value.value.recovering : undefined;
  useEffect(() => {
    if (recovering !== undefined) seen.current = true;
    else if (live.status === "ready" && seen.current && !told.current) { told.current = true; onRecovered(); }
  }, [recovering, live.status, onRecovered]);

  if (live.status === "loading") return <Text role="status">{copy.outcome.working}</Text>;
  if (live.status === "failed") return <Text role="alert">{copy.outcome.failed}</Text>;
  if (recovering === undefined) return seen.current ? <Text role="status">{copy.waitDone}</Text> : <Entry services={services} copy={copy} {...(scanKey ? { scanKey } : {})} onCancel={onCancel} />;
  return <Restricted services={services} copy={copy} view={recovering} {...(scanLink ? { scanLink } : {})} onCancel={onCancel} />;
}

function Entry({ services, copy, scanKey, onCancel }: { readonly services: SettingsServices; readonly copy: LostDevicesCopy; readonly scanKey?: () => Promise<string | null>; readonly onCancel: () => void }) {
  const [key, setKey] = useState("");
  const [place, setPlace] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const op = useOperation(services);
  const begin = (text: string): void => {
    if (text.trim() === "") { setNote(copy.stage.not_found); return; }
    setNote(null);
    void op.run("begin-recovery", { recoveryKey: text, ...(place.trim() === "" ? {} : { place }) });
  };
  const scan = async (): Promise<void> => {
    if (!scanKey) return;
    const payload = await scanKey();
    if (payload === null) { setNote(copy.scanNothing); return; }
    setKey(payload);
    begin(payload);
  };
  return (
    <section className="om-settings__step" aria-labelledby="om-lost-h">
      <FocusHeading id="om-lost-h">{copy.heading}</FocusHeading>
      <Text as="p">{copy.intro}</Text>
      {scanKey ? <div className="om-settings__actions"><Button variant="primary" disabled={op.locked} onClick={() => { void scan(); }}>{copy.scan}</Button></div> : null}
      <form className="om-settings__form" onSubmit={(e) => { e.preventDefault(); begin(key); }}>
        <label className="om-settings__field">
          <span>{copy.keyLabel}</span>
          <textarea
            className="om-settings__input" rows={3} value={key} aria-describedby="om-lost-key-hint" autoComplete="off" autoCapitalize="off" autoCorrect="off" spellCheck={false}
            onChange={(e) => { setKey(e.target.value); }}
          />
        </label>
        <Text as="p" size="sm" tone="secondary" id="om-lost-key-hint">{copy.keyHint}</Text>
        <label className="om-settings__field">
          <span>{copy.placeLabel}</span>
          <input type="text" className="om-settings__input" value={place} aria-describedby="om-lost-place-hint" inputMode="url" autoComplete="off" autoCapitalize="off" spellCheck={false} onChange={(e) => { setPlace(e.target.value); }} />
        </label>
        <Text as="p" size="sm" tone="secondary" id="om-lost-place-hint">{copy.placeHint}</Text>
        <div className="om-settings__actions">
          <Button type="submit" variant={scanKey ? "secondary" : "primary"} disabled={op.locked}>{copy.find}</Button>
          <Button variant="quiet" onClick={onCancel}>{copy.cancel}</Button>
        </div>
      </form>
      {note === null ? null : <p role="alert">{note}</p>}
      <OperationStatus state={op.state} copy={copy.outcome} done="" />
    </section>
  );
}

function Restricted({ services, copy, view, scanLink, onCancel }: { readonly services: SettingsServices; readonly copy: LostDevicesCopy; readonly view: RecoveringView; readonly scanLink?: () => Promise<string | null>; readonly onCancel: () => void }) {
  const op = useOperation(services);
  const attempt = view.attempt ?? null;
  const ended = attempt === "vetoed" || attempt === "stale" || attempt === "cancelled";
  if (view.stage === "checking") return <section className="om-settings__step" aria-labelledby="om-lost-h"><FocusHeading id="om-lost-h">{copy.heading}</FocusHeading><Text role="status">{copy.looking}</Text></section>;
  if (view.stage !== "found") {
    const key = view.stage;
    return (
      <section className="om-settings__step" aria-labelledby="om-lost-h">
        <FocusHeading id="om-lost-h">{copy.heading}</FocusHeading>
        <Text as="p" role="alert" data-stage={key}>{copy.stage[key]}</Text>
        <div className="om-settings__actions">
          <Button variant="secondary" disabled={op.locked} onClick={() => { void op.run("check-recovery-again", {}); }}>{copy.checkAgain}</Button>
          <Button variant="quiet" onClick={onCancel}>{copy.startOver}</Button>
        </div>
        <OperationStatus state={op.state} copy={copy.outcome} done="" />
      </section>
    );
  }
  if (attempt === null || view.retryFromVerifiedHead) {
    return (
      <section className="om-settings__step" aria-labelledby="om-lost-h">
        <FocusHeading id="om-lost-h">{copy.foundHeading}</FocusHeading>
        <Text as="p">{copy.foundBody}</Text>
        <Text as="p" size="sm" tone="secondary" data-evidence={view.evidence ?? "unpinned"}>{copy.evidence[view.evidence === "pinned" ? "pinned" : "other"]}</Text>
        <div className="om-settings__actions">
          <Button variant="primary" disabled={op.locked} onClick={() => { void op.run("request-recovery", {}); }}>{copy.ask}</Button>
          <Button variant="quiet" onClick={onCancel}>{copy.cancel}</Button>
        </div>
        <OperationStatus state={op.state} copy={copy.outcome} done="" />
      </section>
    );
  }
  if (ended) {
    return (
      <section className="om-settings__step" aria-labelledby="om-lost-h">
        <FocusHeading id="om-lost-h">{copy.heading}</FocusHeading>
        <Text as="p" role="alert" data-attempt={attempt}>{copy.ended[attempt]}</Text>
        <div className="om-settings__actions"><Button variant="primary" onClick={onCancel}>{copy.startOver}</Button></div>
      </section>
    );
  }
  return (
    <section className="om-settings__step" aria-labelledby="om-lost-h">
      <FocusHeading id="om-lost-h">{copy.waitingHeading}</FocusHeading>
      <Text as="p">{copy.waitingBody}</Text>
      <Text as="p" role="status" data-remaining={String(view.remainingMs ?? "")}>{copy.remaining(view.remainingMs ?? 0)}</Text>
      <div className="om-settings__actions">
        <Button variant="secondary" disabled={op.locked} onClick={() => { void op.run("check-recovery-again", {}); }}>{copy.checkAgain}</Button>
        <Button variant="quiet" disabled={op.locked} onClick={() => { void op.run("cancel-recovery", {}); }}>{copy.stop}</Button>
      </div>
      <OperationStatus state={op.state} copy={copy.outcome} done="" />
      <Approval services={services} copy={copy} view={view} {...(scanLink ? { scanLink } : {})} />
    </section>
  );
}

/** A device that still works can approve this request now: its link is typed or scanned here, the codes are compared on both. */
function Approval({ services, copy, view, scanLink }: { readonly services: SettingsServices; readonly copy: LostDevicesCopy; readonly view: RecoveringView; readonly scanLink?: () => Promise<string | null> }) {
  const c = copy.approval;
  const [link, setLink] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const op = useOperation(services);
  const join = (text: string): void => {
    if (!RECOVERY_APPROVAL_LINK.test(text.trim())) { setNote(c.parseError); return; }
    setNote(null);
    void op.run("join-recovery-approval", { link: text.trim() });
  };
  const scan = async (): Promise<void> => {
    if (!scanLink) return;
    const payload = await scanLink();
    if (payload === null) { setNote(copy.scanNothing); return; }
    setLink(payload);
    join(payload);
  };
  const a = view.approval ?? null;
  const open = a === null || a.state === "failed" || a.state === "expired";
  return (
    <section className="om-settings__group" aria-labelledby="om-lost-approve-h">
      <Text as="h2" size="lg" weight="bold" id="om-lost-approve-h">{c.heading}</Text>
      <Text as="p">{c.body}</Text>
      {a === null ? null : (
        <div className="om-devices__code" role="group" aria-labelledby="om-lost-code-l">
          {a.verificationCode ? (
            <>
              <Text as="p" size="sm" weight="bold" id="om-lost-code-l">{c.codeLabel}</Text>
              <output className="om-devices__digits" translate="no">{a.verificationCode}</output>
              <Text as="p" size="sm" tone="secondary">{c.codeHint}</Text>
            </>
          ) : null}
          <p className="om-devices__state" role={a.state === "failed" || a.state === "expired" ? "alert" : "status"} data-state={a.state}>{c.state[a.state]}</p>
        </div>
      )}
      {open ? (
        <form className="om-settings__form" onSubmit={(e) => { e.preventDefault(); join(link); }}>
          {scanLink ? <div className="om-settings__actions"><Button variant="secondary" disabled={op.locked} onClick={() => { void scan(); }}>{copy.scan}</Button></div> : null}
          <label className="om-settings__field">
            <span>{c.linkLabel}</span>
            <input type="text" className="om-settings__input" value={link} aria-describedby="om-lost-link-hint" inputMode="url" autoComplete="off" autoCapitalize="off" autoCorrect="off" spellCheck={false} onChange={(e) => { setLink(e.target.value); }} />
          </label>
          <Text as="p" size="sm" tone="secondary" id="om-lost-link-hint">{c.linkHint}</Text>
          <div className="om-settings__actions"><Button type="submit" disabled={op.locked}>{c.join}</Button></div>
        </form>
      ) : null}
      {note === null ? null : <p role="alert">{note}</p>}
      <OperationStatus state={op.state} copy={copy.outcome} done="" />
    </section>
  );
}

/** The owner decides whom to re-introduce. Readiness comes from Core's established session. */
export function ReconnectContacts({ services, copy, contacts, inviteLink, copyText, onOpen }: {
  readonly services: SettingsServices;
  readonly copy: ReconnectCopy;
  readonly contacts: readonly ReconnectContactView[];
  readonly inviteLink: (link: string) => string;
  readonly copyText: (text: string) => Promise<boolean>;
  readonly onOpen: (id: ConversationId) => void;
}) {
  if (contacts.length === 0) return null;
  return (
    <section className="om-settings__step" aria-labelledby="om-reconnect-h">
      <Text as="h2" id="om-reconnect-h" weight="bold">{copy.heading}</Text>
      {contacts.map((contact) => (
        <ReconnectContact key={contact.accountId} contact={contact} services={services} copy={copy} inviteLink={inviteLink} copyText={copyText} onOpen={onOpen} />
      ))}
    </section>
  );
}

function ReconnectContact({ contact, services, copy, inviteLink, copyText, onOpen }: {
  readonly contact: ReconnectContactView;
  readonly services: SettingsServices;
  readonly copy: ReconnectCopy;
  readonly inviteLink: (link: string) => string;
  readonly copyText: (text: string) => Promise<boolean>;
  readonly onOpen: (id: ConversationId) => void;
}) {
  const op = useOperation(services);
  const { label, conversationId } = contact;
  return (
    <div className="om-settings__step">
      <Text as="p" weight="bold"><bdi>{label}</bdi></Text>
      {conversationId !== null ? (
        <>
          <Text as="p" tone="secondary" role="status">{copy.ready}</Text>
          <Button variant="quiet" onClick={() => { onOpen(conversationId); }}>{copy.open(label)}</Button>
        </>
      ) : contact.inviteOperationId !== null ? (
        <>
          {contact.invite !== null ? (
            <ReconnectLink key={contact.invite.operationId} link={inviteLink(contact.invite.link)} label={label} copy={copy} copyText={copyText} />
          ) : null}
          <Text as="p" tone="secondary" role="status">{copy.waiting}</Text>
        </>
      ) : (
        <>
          <Button variant="secondary" disabled={op.locked} onClick={() => { void op.run("reconnect-contact", { accountId: contact.accountId }); }}>{copy.reconnect(label)}</Button>
          <OperationStatus state={op.state} copy={copy.outcome} done="" />
        </>
      )}
    </div>
  );
}


function ReconnectLink({ link, label, copy, copyText }: {
  readonly link: string;
  readonly label: string;
  readonly copy: ReconnectCopy;
  readonly copyText: (text: string) => Promise<boolean>;
}) {
  const [copied, setCopied] = useState<boolean | null>(null);
  return (
    <>
      <Text as="p" tone="secondary">{copy.share}</Text>
      <input className="om-settings__input" type="text" aria-label={copy.link(label)} readOnly value={link} onFocus={(event) => { event.currentTarget.select(); }} />
      <Button variant="quiet" onClick={() => { void copyText(link).then(setCopied, () => { setCopied(false); }); }}>{copy.copyLink}</Button>
      <Text as="p" tone="secondary" role="status">{copied === null ? "" : copied ? copy.copied : copy.copyFailed}</Text>
    </>
  );
}
