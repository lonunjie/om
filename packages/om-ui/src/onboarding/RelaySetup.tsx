import { useEffect, useRef, useState, type ReactNode } from "react";
import type { ProfileExecutionContext } from "@om/platform-api";
import type { ActionRegistry } from "../actions/registry";
import type { HomeProbeResult } from "../home/services";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import { VisuallyHidden } from "../shell/primitives/VisuallyHidden";
import type { OnboardingCopy, RelaySetupCopy } from "./copy";
import { OnboardingFrame } from "./OnboardingFrame";
import type { OnboardingPlatform } from "./platform";
import { StepHeading } from "./StepHeading";

/** What the page needs from its host: it never fetches, stores or generates anything itself. */
export interface RelaySetupServices {
  /** Where the one-click setup lives; `undefined` while there is none (the choice then says it is not ready yet). */
  readonly deployUrl: string | undefined;
  /** The setup value for this person's delivery service (public: a hash of a secret that stays on this device). Same value every time until a service is connected. */
  readonly prepare: () => Promise<string>;
  /** Validates the typed address and asks it who it is. Never stores anything. */
  readonly probe: (input: string) => Promise<HomeProbeResult>;
  /** Stores the address and the matching admission secret on this device. `false`: nothing was stored. */
  readonly connect: (baseUrl: string) => Promise<boolean>;
  /** The delivery service is set; the host reconnects and first run continues. */
  readonly done: () => void;
}

export interface RelaySetupProps {
  readonly services: RelaySetupServices;
  readonly copy: RelaySetupCopy;
  readonly frame: OnboardingCopy["frame"];
  readonly platform: OnboardingPlatform;
  readonly registry: ActionRegistry;
  readonly ctx: ProfileExecutionContext;
}

type Value = { readonly k: "loading" } | { readonly k: "failed" } | { readonly k: "ready"; readonly hex: string };
type Said = { readonly text: string; readonly problem: boolean } | undefined;

function describe(copy: RelaySetupCopy, r: HomeProbeResult): string {
  const c = copy.result;
  switch (r.kind) {
    case "invalid_url": return copy.invalid[r.reason];
    case "ok": return c.health[r.health];
    case "unreachable": return c.unreachable;
    case "failing": return c.failing(r.status);
    case "not_a_home": return c.notAHome;
    case "wrong_version": return c.wrongVersion(r.offered.join(", "));
    case "no_mailbox": return c.noMailbox;
  }
}

/**
 * First run, before anything is created (ADR-0186): there is no built-in delivery service, so the person sets up their own.
 * Page one says why OM is different and lists the ways; only "Set up my own, free" is built. Choosing it shows the setup value
 * the setup asks for and a link that opens in a new tab; before that, a page asks the person to make sure they have both accounts
 * (each its own step with an "I have it" confirmation, so nobody reaches the vendor's page without them); the person comes back, types the address, and the host probes,
 * stores it and carries on. Nothing is stored before the address has answered as a delivery service.
 */
export function RelaySetup({ services, copy, frame, platform, registry, ctx }: RelaySetupProps) {
  const [phase, setPhase] = useState<"choose" | "accounts" | "setup">("choose");
  const [have, setHave] = useState<{ readonly cloudflare: boolean; readonly git: boolean }>({ cloudflare: false, git: false });
  const [value, setValue] = useState<Value>({ k: "loading" });
  const [text, setText] = useState("");
  const [busy, setBusy] = useState<"idle" | "checking">("idle");
  const [said, setSaid] = useState<Said>(undefined);
  const [copied, setCopied] = useState<"none" | "done" | "failed">("none");
  const ticket = useRef(0);
  const mounted = useRef(true);
  useEffect(() => () => { mounted.current = false; }, []);

  useEffect(() => {
    if (phase !== "setup") return;
    let live = true;
    setValue({ k: "loading" });
    services.prepare().then((hex) => { if (live) setValue({ k: "ready", hex }); }, () => { if (live) setValue({ k: "failed" }); });
    return () => { live = false; };
  }, [phase, services]);

  async function copyValue(hex: string): Promise<void> {
    const clipboard = platform.clipboard();
    const r = registry.resolve({ actionId: "copy-text", params: { sensitive: false }, ctx, origin: "direct" });
    if (clipboard.state === "unavailable" || r.status !== "platform" || r.request.capability !== "Clipboard") { setCopied("failed"); return; }
    try { setCopied((await clipboard.api.write(hex, { sensitive: false })) === "written" ? "done" : "failed"); } catch { setCopied("failed"); }
  }

  function check(): void {
    const mine = ++ticket.current;
    setSaid(undefined);
    setBusy("checking");
    const finish = (s: Said): void => { if (ticket.current === mine && mounted.current) { setBusy("idle"); setSaid(s); } };
    services.probe(text).then(
      (r) => {
        if (r.kind !== "ok" || r.health === "unhealthy") { finish({ text: describe(copy, r), problem: true }); return; }
        // A delivery service that does not ask for the admission secret was deployed without the setup value: it is open to anyone who learns the address.
        if (r.admission !== true) { finish({ text: copy.setup.notLocked, problem: true }); return; }
        services.connect(r.baseUrl).then(
          (ok) => { if (ok) { if (ticket.current === mine) services.done(); } else finish({ text: copy.setup.saveFailed, problem: true }); },
          () => { finish({ text: copy.setup.saveFailed, problem: true }); },
        );
      },
      () => { finish({ text: copy.result.unreachable, problem: true }); },
    );
  }

  const s = copy.setup;
  if (phase === "choose") {
    return (
      <OnboardingFrame copy={frame} stepKey="relay-choose">
        <section className="om-onb__step" aria-labelledby="om-onb-relay-h">
          <StepHeading id="om-onb-relay-h">{copy.heading}</StepHeading>
          {copy.why.map((line, i) => <Text as="p" key={i}>{line}</Text>)}
          <Text as="p">{copy.pick}</Text>
          <section className="om-onb__easy" aria-labelledby="om-onb-relay-easy-h">
            <Text as="h3" size="md" weight="bold" id="om-onb-relay-easy-h">{copy.easyHeading}</Text>
            <div className="om-onb__actions">
              <Button variant="primary" onClick={() => { setPhase("accounts"); }}>{copy.ownFree}</Button>
            </div>
            <Text as="p" size="sm" tone="secondary">{copy.cloudflare.uses}</Text>
          </section>
          <section className="om-onb__later" aria-labelledby="om-onb-relay-advanced-h">
            <Text as="h3" size="md" weight="bold" id="om-onb-relay-advanced-h">{copy.advancedHeading}</Text>
            <Text as="p" size="sm" tone="secondary">{copy.advancedIntro}</Text>
            <ul>{copy.later.items.map((item) => <li key={item}>{item}</li>)}</ul>
          </section>
        </section>
      </OnboardingFrame>
    );
  }
  if (phase === "accounts") {
    const a = copy.cloudflare.accounts;
    const step = (key: "cloudflare" | "git", body: readonly string[], heading: string): ReactNode => (
      <section className="om-onb__easy" aria-labelledby={`om-onb-relay-${key}-h`}>
        <Text as="h3" size="md" weight="bold" id={`om-onb-relay-${key}-h`}>{heading}</Text>
        {body.map((line) => <Text as="p" key={line}>{line}</Text>)}
        <div className="om-onb__actions">
          <Button aria-pressed={have[key]} onClick={() => { setHave((h) => ({ ...h, [key]: !h[key] })); }}>{a.confirm}</Button>
        </div>
        {have[key] ? <Text as="p" role="status" className="om-onb__copied">{a.confirmed}</Text> : null}
      </section>
    );
    const ready = have.cloudflare && have.git;
    return (
      <OnboardingFrame copy={frame} stepKey="relay-accounts">
        <section className="om-onb__step" aria-labelledby="om-onb-relay-accounts-h">
          <StepHeading id="om-onb-relay-accounts-h">{a.heading}</StepHeading>
          <Text as="p">{a.intro}</Text>
          {step("cloudflare", [a.cloudflare.body], a.cloudflare.heading)}
          {step("git", [a.git.body, a.git.shareAdvice], a.git.heading)}
          <Text as="p" size="sm" tone="secondary">{a.unverified}</Text>
          {ready ? null : <Text as="p" size="sm" tone="secondary" id="om-onb-relay-accounts-wait">{a.nextWaiting}</Text>}
          <div className="om-onb__actions">
            <Button variant="primary" disabled={!ready} aria-describedby={ready ? undefined : "om-onb-relay-accounts-wait"} onClick={() => { setPhase("setup"); }}>{a.next}</Button>
            <Button variant="quiet" onClick={() => { setPhase("choose"); }}>{a.back}</Button>
          </div>
        </section>
      </OnboardingFrame>
    );
  }
  return (
    <OnboardingFrame copy={frame} stepKey="relay-setup">
      <section className="om-onb__step" aria-labelledby="om-onb-relay-setup-h">
        <StepHeading id="om-onb-relay-setup-h">{s.heading}</StepHeading>
        <ol className="om-onb__relay-steps">{copy.cloudflare.steps.map((step) => <li key={step}>{step}</li>)}</ol>
        <Text as="p" size="sm" tone="secondary">{copy.cloudflare.protect}</Text>
        <div className="om-onb__field">
          <span id="om-onb-relay-value-l">{s.valueLabel}</span>
          {value.k === "ready" ? <code className="om-onb__link" translate="no" aria-labelledby="om-onb-relay-value-l">{value.hex}</code> : null}
          {value.k === "loading" ? <Text as="p" role="status">{s.preparing}</Text> : null}
          {value.k === "failed" ? <Text as="p" role="alert">{s.prepareFailed}</Text> : null}
          <Text as="p" size="sm" tone="secondary">{s.valueNote}</Text>
        </div>
        {value.k === "ready" ? (
          <div className="om-onb__actions">
            <Button onClick={() => { void copyValue(value.hex); }}>{s.copy}</Button>
            {services.deployUrl === undefined ? null : (
              <a className="om-button" data-variant="primary" href={services.deployUrl} target="_blank" rel="noopener noreferrer">
                {copy.cloudflare.open}<VisuallyHidden> {copy.cloudflare.opensNewTab}</VisuallyHidden>
              </a>
            )}
          </div>
        ) : null}
        {services.deployUrl === undefined || value.k !== "ready" ? null : <Text as="p" size="sm" tone="secondary">{copy.cloudflare.openAgain}</Text>}
        {copied === "done" ? <Text as="p" role="status" className="om-onb__copied">{s.copied}</Text> : null}
        {copied === "failed" ? <Text as="p" role="alert">{s.copyFailed}</Text> : null}
        {services.deployUrl === undefined ? <Text as="p" role="note">{copy.cloudflare.notReady}</Text> : null}
        <form className="om-onb__invite" onSubmit={(e) => { e.preventDefault(); check(); }}>
          <label className="om-onb__field">
            <span>{s.addressLabel}</span>
            <input
              type="text" className="om-onb__input om-onb__input--text" value={text} aria-describedby="om-onb-relay-address-hint" inputMode="url"
              autoComplete="off" autoCapitalize="off" autoCorrect="off" spellCheck={false}
              onChange={(e) => { ticket.current++; setText(e.target.value); setSaid(undefined); setBusy("idle"); }}
            />
          </label>
          <Text as="p" size="sm" tone="secondary" id="om-onb-relay-address-hint">{s.addressHint} {copy.cloudflare.addressExample}</Text>
          {busy === "checking" ? <Text as="p" role="status">{s.checking}</Text> : null}
          {said === undefined ? null : <Text as="p" role={said.problem ? "alert" : "status"} data-problem={said.problem ? "true" : undefined}>{said.text}</Text>}
          <div className="om-onb__actions">
            <Button type="submit" variant="primary" disabled={busy === "checking" || text.trim() === ""}>{s.check}</Button>
            <Button variant="quiet" onClick={() => { ticket.current++; setBusy("idle"); setSaid(undefined); setPhase("accounts"); }}>{s.back}</Button>
          </div>
        </form>
      </section>
    </OnboardingFrame>
  );
}
