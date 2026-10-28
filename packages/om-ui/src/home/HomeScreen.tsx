import { useEffect, useRef, useState } from "react";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import "../devices/devices.css";
import type { HomeCopy } from "./copy";
import type { HomeProbeResult, HomeSettingsServices } from "./services";

export interface HomeScreenProps {
  readonly services: HomeSettingsServices;
  readonly copy: HomeCopy;
  /** How often the in-memory list of lost mailboxes is re-read. */
  readonly refreshMs?: number;
}

type Checked = { readonly k: "idle" } | { readonly k: "checking" } | { readonly k: "done"; readonly result: HomeProbeResult };
type Saved = "none" | "chosen" | "cleared" | "failed";

/** An unhealthy Home cannot take messages now, so it is never offered for saving; a degraded or unknown one is, with its state shown. */
const savable = (r: HomeProbeResult): r is Extract<HomeProbeResult, { kind: "ok" }> => r.kind === "ok" && r.health !== "unhealthy";

/** Wording for everything a probe can say about an address (the same lines serve the current Home and a candidate). */
function describe(copy: HomeCopy, r: HomeProbeResult): { readonly text: string; readonly problem: boolean } {
  const c = copy.result;
  switch (r.kind) {
    case "invalid_url": return { text: copy.invalid[r.reason], problem: true };
    case "ok": return { text: `${c.ok(r.providerId)} ${c.health[r.health]} ${copy.selfReported}`, problem: r.health !== "healthy" };
    case "unreachable": return { text: c.unreachable, problem: true };
    case "failing": return { text: c.failing(r.status), problem: true };
    case "not_a_home": return { text: c.notAHome, problem: true };
    case "wrong_version": return { text: c.wrongVersion(r.offered.join(", ")), problem: true };
    case "no_mailbox": return { text: c.noMailbox, problem: true };
  }
}

/**
 * Settings > Your Home. The user types an address, the host asks that address who it is, and only then can it be chosen.
 * A choice takes effect on reconnect, and there is deliberately no way to arrive here pre-filled from a link.
 */
export function HomeScreen({ services, copy, refreshMs = 4000 }: HomeScreenProps) {
  const { current } = services;
  const [text, setText] = useState("");
  const [checked, setChecked] = useState<Checked>({ k: "idle" });
  const [saved, setSaved] = useState<Saved>("none");
  const [now, setNow] = useState<Checked>(current.baseUrl === undefined ? { k: "idle" } : { k: "checking" });
  const [lost, setLost] = useState<readonly string[]>(() => services.lostMailboxes());
  const ticket = useRef(0);

  // Ask the Home in use how it is doing, so a Home that has become unreachable or degraded is visible here.
  useEffect(() => {
    if (current.baseUrl === undefined) return;
    let live = true;
    services.probe(current.baseUrl).then((r) => { if (live) setNow({ k: "done", result: r }); }, () => { if (live) setNow({ k: "done", result: { kind: "unreachable" } }); });
    return () => { live = false; };
  }, [services, current.baseUrl]);

  useEffect(() => {
    const read = (): void => { setLost((prev) => { const next = services.lostMailboxes(); return prev.length === next.length && prev.every((p, i) => p === next[i]) ? prev : next; }); };
    read();
    const t = setInterval(read, refreshMs);
    return () => { clearInterval(t); };
  }, [services, refreshMs]);

  const check = (): void => {
    const mine = ++ticket.current;
    setSaved("none");
    setChecked({ k: "checking" });
    services.probe(text).then(
      (result) => { if (ticket.current === mine) setChecked({ k: "done", result }); },
      () => { if (ticket.current === mine) setChecked({ k: "done", result: { kind: "unreachable" } }); },
    );
  };
  const edit = (value: string): void => { ticket.current++; setText(value); setChecked({ k: "idle" }); setSaved("none"); };
  const choose = (baseUrl: string): void => {
    void services.save(baseUrl).then((ok) => { setSaved(ok ? "chosen" : "failed"); }, () => { setSaved("failed"); });
  };
  const forget = (): void => {
    void services.clear().then((ok) => { setSaved(ok ? "cleared" : "failed"); }, () => { setSaved("failed"); });
  };

  const result = checked.k === "done" ? checked.result : undefined;
  const same = result?.kind === "ok" && result.baseUrl === current.baseUrl;
  const nowText = now.k === "done" ? describe(copy, now.result) : undefined;
  const said = result === undefined ? undefined : describe(copy, result);

  return (
    <section className="om-settings om-home" aria-labelledby="om-home-h">
      <Text as="h1" size="xl" weight="bold" id="om-home-h">{copy.heading}</Text>

      <section className="om-settings__group" aria-labelledby="om-home-now-h">
        <Text as="h2" size="lg" weight="bold" id="om-home-now-h">{copy.currentHeading}</Text>
        {current.baseUrl === undefined ? <Text as="p">{copy.none}</Text> : (
          <>
            <Text as="p" className="om-settings__name">{copy.address(current.baseUrl)}</Text>
            <Text as="p" size="sm" tone="secondary">{current.source === "chosen" ? copy.source.chosen : copy.source.default}</Text>
            {now.k === "checking" ? <Text as="p" role="status">{copy.checkingCurrent}</Text> : null}
            {nowText === undefined ? null : <Text as="p" role={nowText.problem ? "alert" : "status"} data-problem={nowText.problem ? "true" : undefined}>{nowText.text}</Text>}
          </>
        )}
        {current.source === "chosen" ? (
          <div className="om-settings__actions">
            <Button onClick={forget}>{current.hasDefault ? copy.useDefault : copy.disconnect}</Button>
          </div>
        ) : null}
      </section>

      {lost.length === 0 ? null : (
        <section className="om-settings__group" aria-labelledby="om-home-lost-h" data-problem="true">
          <Text as="h2" size="lg" weight="bold" id="om-home-lost-h">{copy.lost.heading}</Text>
          <Text as="p">{copy.lost.body(lost.length)}</Text>
          <ul className="om-settings__list">
            {lost.map((m) => <li key={m}>{copy.lost.item(m.slice(0, 8))}</li>)}
          </ul>
        </section>
      )}

      <section className="om-settings__group" aria-labelledby="om-home-change-h">
        <Text as="h2" size="lg" weight="bold" id="om-home-change-h">{copy.changeHeading}</Text>
        <Text as="p">{copy.changeBody}</Text>
        <form className="om-settings__form" onSubmit={(e) => { e.preventDefault(); check(); }}>
          <label className="om-settings__field">
            <span>{copy.urlLabel}</span>
            <input
              type="text" className="om-settings__input" value={text} aria-describedby="om-home-url-hint" inputMode="url"
              autoComplete="off" autoCapitalize="off" autoCorrect="off" spellCheck={false}
              onChange={(e) => { edit(e.target.value); }}
            />
          </label>
          <Text as="p" size="sm" tone="secondary" id="om-home-url-hint">{copy.urlHint}</Text>
          <div className="om-settings__actions">
            <Button type="submit" variant="primary" disabled={checked.k === "checking" || text.trim() === ""}>{copy.check}</Button>
          </div>
        </form>

        {checked.k === "checking" ? <Text as="p" role="status">{copy.checking}</Text> : null}
        {result === undefined || said === undefined ? null : (
          <div className="om-settings__step">
            <Text as="p" role={said.problem ? "alert" : "status"} data-problem={said.problem ? "true" : undefined}>{said.text}</Text>
            {same ? <Text as="p">{copy.alreadyUsing}</Text> : null}
            {savable(result) && !same && saved === "none" ? (
              <>
                {current.baseUrl === undefined ? null : <Text as="p" className="om-devices__warning">{copy.changeWarning}</Text>}
                <div className="om-settings__actions">
                  <Button variant="primary" onClick={() => { choose(result.baseUrl); }}>{result.health === "healthy" ? copy.useThis : copy.useAnyway}</Button>
                </div>
              </>
            ) : null}
          </div>
        )}
      </section>

      {saved === "failed" ? <Text as="p" role="alert">{copy.saveFailed}</Text> : null}
      {saved === "chosen" || saved === "cleared" ? (
        <div className="om-settings__step">
          <Text as="p" role="status">{saved === "chosen" ? copy.saved : copy.cleared}</Text>
          <div className="om-settings__actions"><Button variant="primary" onClick={services.reconnect}>{copy.reconnect}</Button></div>
        </div>
      ) : null}
    </section>
  );
}
