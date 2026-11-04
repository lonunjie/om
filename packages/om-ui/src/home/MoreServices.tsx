import { useEffect, useRef, useState } from "react";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import { VisuallyHidden } from "../shell/primitives/VisuallyHidden";
import "../devices/devices.css";
import type { HomeCopy } from "./copy";
import { describeProbe } from "./describe";
import type { DeliveryRole, DeliveryServiceItem, HomeProbeResult, HomeSettingsServices, MoreHomesServices } from "./services";

export interface MoreServicesProps {
  readonly services: HomeSettingsServices;
  readonly more: MoreHomesServices;
  readonly copy: HomeCopy;
  readonly copyText?: (text: string) => Promise<boolean>;
}

type Probe = HomeProbeResult | "checking";
type Move =
  | { readonly k: "idle" }
  | { readonly k: "confirm" }
  | { readonly k: "working" }
  | { readonly k: "done"; readonly stuck: number }
  | { readonly k: "last" }
  | { readonly k: "failed" };

/**
 * Settings > Message delivery > Advanced (MULTI_RELAY_AND_DELIVERY_RESILIENCE 1: a person may use several delivery services at once).
 * Lists every service with how it is doing, adds a backup the same way the first one was set up, and moves chats off a service.
 * Nothing is fetched or stored here: the host probes, stores and tells Core. A change takes effect when the host reconnects.
 */
export function MoreServices({ services, more, copy, copyText }: MoreServicesProps) {
  const a = copy.advanced;
  const [open, setOpen] = useState(false);
  const [opened, setOpened] = useState(false);
  const [probes, setProbes] = useState<Readonly<Record<string, Probe>>>({});
  const asked = useRef(new Set<string>());
  const [moves, setMoves] = useState<Readonly<Record<string, Move>>>({});
  const [leaving, setLeaving] = useState<ReadonlySet<string>>(() => new Set());
  const [reconnect, setReconnect] = useState<"none" | "added" | "moved">("none");
  const mounted = useRef(true);
  useEffect(() => () => { mounted.current = false; }, []);

  // Services are asked how they are doing only once the person opens this part of the page.
  useEffect(() => {
    if (!open) return;
    for (const { baseUrl } of more.services) {
      if (asked.current.has(baseUrl)) continue;
      asked.current.add(baseUrl);
      setProbes((p) => ({ ...p, [baseUrl]: "checking" }));
      services.probe(baseUrl).then(
        (r) => { if (mounted.current) setProbes((p) => ({ ...p, [baseUrl]: r })); },
        () => { if (mounted.current) setProbes((p) => ({ ...p, [baseUrl]: { kind: "unreachable" } })); },
      );
    }
  }, [open, more.services, services]);

  const roleOf = (item: DeliveryServiceItem): DeliveryRole => (leaving.has(item.baseUrl) ? "leaving" : item.role);
  const staying = more.services.filter((s) => roleOf(s) !== "leaving");
  const setMove = (url: string, m: Move): void => { setMoves((x) => ({ ...x, [url]: m })); };

  function move(url: string): void {
    setMove(url, { k: "working" });
    more.move(url).then(
      (r) => {
        if (!mounted.current) return;
        if (r.kind === "moved") { setLeaving((l) => new Set(l).add(url)); setMove(url, { k: "done", stuck: r.stuck }); setReconnect("moved"); } else setMove(url, { k: r.kind });
      },
      () => { if (mounted.current) setMove(url, { k: "failed" }); },
    );
  }

  return (
    <section className="om-settings__group om-home__advanced" aria-label={a.toggle}>
      <Button variant="quiet" aria-expanded={open} aria-controls="om-more" onClick={() => { setOpen((v) => !v); setOpened(true); }}>{a.toggle}</Button>
      <div id="om-more" className="om-settings__group" hidden={!open}>
        {!opened ? null : (
          <>
            <section className="om-settings__group" aria-labelledby="om-more-list-h">
              <Text as="h2" size="lg" weight="bold" id="om-more-list-h">{a.listHeading}</Text>
              <Text as="p">{a.listIntro}</Text>
              <ul className="om-settings__rows">
                {more.services.map((item, i) => (
                  <ServiceRow
                    key={item.baseUrl} index={i} item={item} role={roleOf(item)} probe={probes[item.baseUrl]} copy={copy} move={moves[item.baseUrl] ?? { k: "idle" }}
                    canMove={roleOf(item) === "leaving" || staying.some((s) => s.baseUrl !== item.baseUrl)}
                    onAsk={() => { setMove(item.baseUrl, { k: "confirm" }); }} onCancel={() => { setMove(item.baseUrl, { k: "idle" }); }} onMove={() => { move(item.baseUrl); }}
                  />
                ))}
              </ul>
            </section>

            <AddBackup services={services} more={more} copy={copy} {...(copyText === undefined ? {} : { copyText })} onAdded={() => { setReconnect("added"); }} />

            <section className="om-settings__group" aria-labelledby="om-more-move-h">
              <Text as="h2" size="lg" weight="bold" id="om-more-move-h">{a.move.heading}</Text>
              <Text as="p">{a.move.body}</Text>
              {staying.length < 2 ? <Text as="p" tone="secondary">{a.move.needBackup}</Text> : null}
            </section>

            {reconnect === "none" ? null : (
              <div className="om-settings__step">
                <Text as="p" role="status">{reconnect === "added" ? a.add.added : a.move.done}</Text>
                <div className="om-settings__actions"><Button variant="primary" onClick={services.reconnect}>{copy.reconnect}</Button></div>
              </div>
            )}
          </>
        )}
      </div>
    </section>
  );
}

interface RowProps {
  readonly index: number;
  readonly item: DeliveryServiceItem;
  readonly role: DeliveryRole;
  readonly probe: Probe | undefined;
  readonly copy: HomeCopy;
  readonly move: Move;
  readonly canMove: boolean;
  readonly onAsk: () => void;
  readonly onCancel: () => void;
  readonly onMove: () => void;
}

function ServiceRow({ index, item, role, probe, copy, move, canMove, onAsk, onCancel, onMove }: RowProps) {
  const a = copy.advanced;
  const [shown, setShown] = useState(false);
  const id = `om-more-${String(index)}`;
  const said = probe === undefined || probe === "checking" ? undefined : describeProbe(copy, probe);
  const busy = move.k === "working";
  return (
    <li className="om-home__service">
      <Text as="h3" size="md" weight="bold" id={`${id}-role`}>{a.role[role]}</Text>
      {probe === "checking" ? <Text as="p" role="status">{copy.checkingCurrent}</Text> : null}
      {said === undefined ? null : <Text as="p" role={said.problem ? "alert" : "status"} data-problem={said.problem ? "true" : undefined}>{said.text}</Text>}
      <div className="om-home__address">
        <Button variant="quiet" aria-expanded={shown} aria-controls={`${id}-address`} aria-describedby={`${id}-role`} onClick={() => { setShown((v) => !v); }}>{shown ? copy.hideAddress : copy.showAddress}</Button>
        <Text as="p" id={`${id}-address`} hidden={!shown} className="om-settings__name">{copy.address(item.baseUrl)}</Text>
      </div>

      {move.k === "confirm" ? (
        <div className="om-settings__confirm" role="group" aria-labelledby={`${id}-confirm-h`}>
          <Text as="p" weight="bold" id={`${id}-confirm-h`}>{a.move.confirmHeading}</Text>
          <Text as="p">{a.move.confirmBody}</Text>
          {role === "main" ? <Text as="p">{a.move.confirmInvites}</Text> : null}
          <div className="om-settings__actions">
            <Button variant="danger" onClick={onMove}>{a.move.yes}</Button>
            <Button onClick={onCancel}>{a.move.no}</Button>
          </div>
        </div>
      ) : null}
      {busy ? <Text as="p" role="status">{a.move.working}</Text> : null}
      {move.k === "done" && move.stuck > 0 ? <Text as="p" role="alert" data-problem="true">{a.move.stuck(move.stuck)}</Text> : null}
      {move.k === "last" ? <Text as="p" role="alert" data-problem="true">{a.move.needBackup}</Text> : null}
      {move.k === "failed" ? <Text as="p" role="alert" data-problem="true">{a.move.failed}</Text> : null}
      {canMove && move.k !== "confirm" && !busy ? (
        <div className="om-settings__actions">
          <Button aria-describedby={`${id}-role`} onClick={role === "leaving" ? onMove : onAsk}>{role === "leaving" ? a.move.retry : a.move.action}</Button>
        </div>
      ) : null}
    </li>
  );
}

interface AddProps {
  readonly services: HomeSettingsServices;
  readonly more: MoreHomesServices;
  readonly copy: HomeCopy;
  readonly copyText?: (text: string) => Promise<boolean>;
  readonly onAdded: () => void;
}

type Value = { readonly k: "loading" } | { readonly k: "failed" } | { readonly k: "ready"; readonly hex: string };

/** The first-run steps again, for a second service: the same setup value and one-tap setup, then the typed address is checked and stored. */
function AddBackup({ services, more, copy, copyText, onAdded }: AddProps) {
  const a = copy.advanced.add;
  const r = a.relay;
  const [open, setOpen] = useState(false);
  const [value, setValue] = useState<Value>({ k: "loading" });
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [said, setSaid] = useState<{ readonly text: string; readonly problem: boolean } | undefined>(undefined);
  const [copied, setCopied] = useState<"none" | "done" | "failed">("none");
  const ticket = useRef(0);
  const mounted = useRef(true);
  useEffect(() => () => { mounted.current = false; }, []);

  useEffect(() => {
    if (!open) return;
    let live = true;
    setValue({ k: "loading" });
    more.setup.prepare().then((hex) => { if (live) setValue({ k: "ready", hex }); }, () => { if (live) setValue({ k: "failed" }); });
    return () => { live = false; };
  }, [open, more.setup]);

  function check(): void {
    const mine = ++ticket.current;
    setSaid(undefined);
    setBusy(true);
    const finish = (s: { readonly text: string; readonly problem: boolean } | undefined): void => { if (ticket.current === mine && mounted.current) { setBusy(false); setSaid(s); } };
    services.probe(text).then(
      (p) => {
        if (p.kind !== "ok" || p.health === "unhealthy") { finish(describeProbe(copy, p)); return; }
        // A service that does not ask for the setup value was deployed without it and is open to anyone who learns the address.
        if (p.admission !== true) { finish({ text: r.setup.notLocked, problem: true }); return; }
        more.add(p.baseUrl).then(
          (res) => {
            if (res === "added") { if (ticket.current === mine && mounted.current) { setBusy(false); setOpen(false); setText(""); onAdded(); } return; }
            finish({ text: res === "already" ? a.already : res === "full" ? a.full : r.setup.saveFailed, problem: true });
          },
          () => { finish({ text: r.setup.saveFailed, problem: true }); },
        );
      },
      () => { finish({ text: copy.result.unreachable, problem: true }); },
    );
  }

  return (
    <section className="om-settings__group" aria-labelledby="om-more-add-b">
      <Button variant="quiet" id="om-more-add-b" aria-expanded={open} aria-controls="om-more-add" onClick={() => { setOpen((v) => !v); }}>{a.toggle}</Button>
      <div id="om-more-add" className="om-settings__group" hidden={!open}>
        {!open ? null : (
          <>
            <Text as="p">{a.intro}</Text>
            <ol className="om-settings__list">{r.cloudflare.steps.map((step) => <li key={step}>{step}</li>)}</ol>
            <Text as="p" size="sm" tone="secondary">{r.cloudflare.protect}</Text>
            <div className="om-settings__field">
              <span id="om-more-value-l">{r.setup.valueLabel}</span>
              {value.k === "ready" ? <code className="om-devices__link" translate="no" aria-labelledby="om-more-value-l">{value.hex}</code> : null}
              {value.k === "loading" ? <Text as="p" role="status">{r.setup.preparing}</Text> : null}
              {value.k === "failed" ? <Text as="p" role="alert">{r.setup.prepareFailed}</Text> : null}
              <Text as="p" size="sm" tone="secondary">{r.setup.valueNote}</Text>
            </div>
            {value.k === "ready" ? (
              <div className="om-settings__actions">
                {copyText === undefined ? null : (
                  <Button onClick={() => { void copyText(value.hex).then((ok) => { if (mounted.current) setCopied(ok ? "done" : "failed"); }, () => { if (mounted.current) setCopied("failed"); }); }}>{r.setup.copy}</Button>
                )}
                {more.setup.deployUrl === undefined ? null : (
                  <a className="om-button" data-variant="primary" href={more.setup.deployUrl} target="_blank" rel="noopener noreferrer">
                    {r.cloudflare.open}<VisuallyHidden> {r.cloudflare.opensNewTab}</VisuallyHidden>
                  </a>
                )}
              </div>
            ) : null}
            {copied === "done" ? <Text as="p" role="status">{r.setup.copied}</Text> : null}
            {copied === "failed" ? <Text as="p" role="alert">{r.setup.copyFailed}</Text> : null}
            {more.setup.deployUrl === undefined ? <Text as="p" role="note">{r.cloudflare.notReady}</Text> : null}
            <form className="om-settings__form" onSubmit={(e) => { e.preventDefault(); check(); }}>
              <label className="om-settings__field">
                <span>{r.setup.addressLabel}</span>
                <input
                  type="text" className="om-settings__input" value={text} aria-describedby="om-more-add-hint" inputMode="url"
                  autoComplete="off" autoCapitalize="off" autoCorrect="off" spellCheck={false}
                  onChange={(e) => { ticket.current++; setText(e.target.value); setSaid(undefined); setBusy(false); }}
                />
              </label>
              <Text as="p" size="sm" tone="secondary" id="om-more-add-hint">{r.setup.addressHint} {r.cloudflare.addressExample}</Text>
              {busy ? <Text as="p" role="status">{r.setup.checking}</Text> : null}
              {said === undefined ? null : <Text as="p" role={said.problem ? "alert" : "status"} data-problem={said.problem ? "true" : undefined}>{said.text}</Text>}
              <div className="om-settings__actions">
                <Button type="submit" variant="primary" disabled={busy || text.trim() === ""}>{a.check}</Button>
                <Button onClick={() => { ticket.current++; setBusy(false); setSaid(undefined); setOpen(false); }}>{a.cancel}</Button>
              </div>
            </form>
          </>
        )}
      </div>
    </section>
  );
}
