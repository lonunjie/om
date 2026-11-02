// Settings > Lock: how this device opens OM, and the ways to change it. A key kept in this browser is called out as "not locked" in
// red; turning every lock off asks twice and says what it means. Every step goes through `LockServices` (the vault), never around it.
import { useEffect, useRef, useState, type ReactNode, type SyntheticEvent } from "react";
import type { VaultErrorReason, VaultWrappingInfo } from "@om/platform-api";
import { Button, Icon, PASSPHRASE_MIN_CODE_POINTS, Text, VisuallyHidden, checkNewPassphrase, type IconName } from "@om/ui";
import { lockCopy as c, onboardingCopy } from "../copy/en";
import { IDLE_CHOICES, type IdleMinutes } from "./lockPrefs";
import type { LockResult, LockServices } from "./lockServices";

type Mode = "none" | "device" | "device-shared" | "password";
const modeOf = (w: VaultWrappingInfo): Mode =>
  w.passphraseProtected ? "password" : w.method === "device-key" ? "none" : w.reducedAtRestProperty ? "device-shared" : "device";

type View = "home" | "set" | "change" | "remove" | "device" | "stop";
type Wrapping = { readonly k: "loading" } | { readonly k: "failed" } | { readonly k: "ready"; readonly w: VaultWrappingInfo };

const problemText = (reason: VaultErrorReason): string => onboardingCopy.protect.errors[reason];

export function LockScreen({ services }: { readonly services: LockServices }) {
  const [state, setState] = useState<Wrapping>({ k: "loading" });
  const [view, setView] = useState<View>("home");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [idle, setIdle] = useState<IdleMinutes>(() => services.idleMinutes());
  const abort = useRef<AbortController | null>(null);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);

  const refresh = (): void => {
    services.wrapping().then((w) => { if (mounted.current) setState(w === null ? { k: "failed" } : { k: "ready", w }); }, () => { if (mounted.current) setState({ k: "failed" }); });
  };
  useEffect(() => { refresh(); }, [services]);
  // A saved tick shows for a moment and goes by itself.
  useEffect(() => {
    if (!saved) return;
    const t = setTimeout(() => { setSaved(false); }, 2600);
    return () => { clearTimeout(t); };
  }, [saved]);

  /** Runs one vault step. On success the page goes home with a tick; a refusal stays on the form and says why in plain words. */
  async function run(step: () => Promise<LockResult>, onOk?: (w: VaultWrappingInfo) => string | null): Promise<void> {
    setBusy(true);
    setProblem(null);
    try {
      const r = await step();
      if (!mounted.current) return;
      if (!r.ok) { setProblem(problemText(r.reason)); return; }
      const note = onOk?.(r.wrapping) ?? null;
      if (note !== null) { setProblem(note); refresh(); return; }
      setState({ k: "ready", w: r.wrapping });
      setView("home");
      setSaved(true);
    } catch {
      if (mounted.current) setProblem(problemText("crypto_failed"));
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  const leave = (): void => { abort.current?.abort(); setView("home"); setProblem(null); };

  if (state.k === "loading") return <LockPage><Text tone="secondary" role="status">{c.loading}</Text></LockPage>;
  if (state.k === "failed") return <LockPage><Text role="alert">{c.failed}</Text></LockPage>;
  const { w } = state;
  const mode = modeOf(w);
  const locked = mode !== "none";

  return (
    <LockPage>
      <div className="om-lock__state" data-mode={mode}>
        <span className="om-lock__icon" aria-hidden="true"><Icon name="lock" size={22} /></span>
        <div className="om-lock__text">
          <Text as="h2" size="md" weight="bold">{c.modes[mode].title}</Text>
          <Text as="p" size="sm">{c.modes[mode].body}</Text>
          {mode === "none" ? <Text as="p" size="sm" weight="bold" role="note">{c.warning}</Text> : null}
        </div>
      </div>
      {saved ? <p className="om-lock__done" role="status"><span className="om-lock__done-mark" aria-hidden="true"><Icon name="check" size={20} /></span><VisuallyHidden>{c.done}</VisuallyHidden></p> : null}

      {view === "home" ? (
        <section className="om-lock__section" aria-labelledby="om-lock-change-h">
          <Text as="h2" size="lg" weight="bold" id="om-lock-change-h">{c.changeHeading}</Text>
          <ul className="om-menu__list">
            <ActionRow icon="lock" {...(w.passphraseProtected ? c.actions.changePassword : c.actions.addPassword)} onClick={() => { setView(w.passphraseProtected ? "change" : "set"); }} />
            {w.passphraseProtected && w.method === "webauthn-prf" ? <ActionRow icon="lock" {...c.actions.removePassword} onClick={() => { setView("remove"); }} /> : null}
            {w.method === "device-key" ? <ActionRow icon="shield" {...c.actions.devicePrompt} onClick={() => { setView("device"); }} /> : null}
            {locked ? <ActionRow icon="close" danger {...c.actions.stop} onClick={() => { setView("stop"); }} /> : null}
          </ul>
        </section>
      ) : null}

      {view === "set" || view === "change" ? (
        <PasswordForm
          key={view} needsCurrent={view === "change"} busy={busy} problem={problem} onCancel={leave}
          onSubmit={(next, current) => { void run(() => services.setPassword(next, current)); }}
        />
      ) : null}
      {view === "remove" ? (
        <PasswordForm key="remove" removing needsCurrent busy={busy} problem={problem} onCancel={leave} onSubmit={(_next, current) => { void run(() => services.removePassword(current ?? "")); }} />
      ) : null}

      {view === "device" ? (
        <section className="om-lock__panel" aria-labelledby="om-lock-device-h">
          <Text as="h2" size="lg" weight="bold" id="om-lock-device-h">{c.actions.devicePrompt.title}</Text>
          <Text as="p">{c.device.body}</Text>
          {busy ? <p className="om-lock__waiting" role="status"><span className="om-onb__spinner" aria-hidden="true" />{c.device.waiting}</p> : null}
          {problem !== null ? <p role="alert">{problem}</p> : null}
          <div className="om-onb__actions">
            <Button
              variant="primary" disabled={busy}
              onClick={() => {
                abort.current = new AbortController();
                void run(() => services.useDevicePrompt(abort.current?.signal), (next) => (next.method === "webauthn-prf" ? null : c.device.unavailable));
              }}
            >{c.device.start}</Button>
            <Button onClick={leave}>{c.device.cancel}</Button>
          </div>
        </section>
      ) : null}

      {view === "stop" ? (
        <StopForm needsPassword={w.passphraseProtected} busy={busy} problem={problem} onCancel={leave} onConfirm={(current) => { void run(() => services.stopLocking(current)); }} />
      ) : null}

      <section className="om-lock__section" aria-labelledby="om-lock-idle-h">
        <Text as="h2" size="lg" weight="bold" id="om-lock-idle-h">{c.idle.heading}</Text>
        {locked ? (
          <>
            <Text as="p" size="sm" tone="secondary">{c.idle.hint}</Text>
            <fieldset className="om-lock__choices">
              <legend className="om-visually-hidden">{c.idle.heading}</legend>
              {([null, ...IDLE_CHOICES] as const).map((m) => (
                <label key={String(m)} className="om-lock__choice">
                  <input type="radio" name="om-lock-idle" checked={idle === m} onChange={() => { services.setIdleMinutes(m); setIdle(m); }} />
                  <span>{m === null ? c.idle.never : c.idle.after(m)}</span>
                </label>
              ))}
            </fieldset>
            <div className="om-onb__actions"><Button onClick={() => { void services.lockNow(); }}><Icon name="lock" size={20} />{c.idle.now}</Button></div>
          </>
        ) : <Text as="p" size="sm" tone="secondary">{c.idle.needsLock}</Text>}
      </section>
    </LockPage>
  );
}

function LockPage({ children }: { readonly children: ReactNode }) {
  return (
    <section className="om-lock" aria-labelledby="om-lock-h">
      <Text as="h1" size="xl" weight="bold" id="om-lock-h">{c.heading}</Text>
      {children}
    </section>
  );
}

function ActionRow({ icon, title, hint, danger = false, onClick }: { readonly icon: IconName; readonly title: string; readonly hint: string; readonly danger?: boolean; readonly onClick: () => void }) {
  return (
    <li>
      <button type="button" className="om-menu__row om-lock__action" data-danger={danger} onClick={onClick}>
        <span className="om-menu__icon" aria-hidden="true"><Icon name={icon} size={22} /></span>
        <span className="om-menu__text"><span className="om-menu__title">{title}</span><span className="om-menu__hint">{hint}</span></span>
        <Icon name="chevron" size={20} />
      </button>
    </li>
  );
}

interface PasswordFormProps {
  readonly needsCurrent: boolean;
  readonly removing?: boolean;
  readonly busy: boolean;
  readonly problem: string | null;
  readonly onCancel: () => void;
  readonly onSubmit: (next: string, current: string | undefined) => void;
}

/** Add, change or remove a password. The typed text leaves state as soon as it is sent; a refusal keeps the form and says why. */
function PasswordForm({ needsCurrent, removing = false, busy, problem, onCancel, onSubmit }: PasswordFormProps) {
  const p = onboardingCopy.protect.passphrase;
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [again, setAgain] = useState("");
  const [shown, setShown] = useState(false);
  const check = removing ? null : checkNewPassphrase(next, again);
  const title = removing ? c.actions.removePassword.title : needsCurrent ? c.actions.changePassword.title : c.actions.addPassword.title;
  const submit = (e: SyntheticEvent): void => {
    e.preventDefault();
    if (check !== null) { setShown(true); return; }
    if (needsCurrent && current === "") return;
    onSubmit(next, needsCurrent ? current : undefined);
    setCurrent(""); setNext(""); setAgain("");
  };
  return (
    <form className="om-lock__panel" aria-labelledby="om-lock-pw-h" onSubmit={submit}>
      <Text as="h2" size="lg" weight="bold" id="om-lock-pw-h">{title}</Text>
      {removing ? null : <p role="note" className="om-onb__notice" data-code="passphrase_unrecoverable">{p.warning}</p>}
      {needsCurrent ? (
        <div className="om-onb__field">
          <label htmlFor="om-lock-current">{c.password.current}</label>
          <input id="om-lock-current" className="om-onb__input" type="password" autoComplete="current-password" spellCheck={false} autoCapitalize="none" autoFocus value={current} disabled={busy} onChange={(e) => { setCurrent(e.target.value); }} />
        </div>
      ) : null}
      {removing ? null : (
        <>
          <div className="om-onb__field">
            <label htmlFor="om-lock-next">{c.password.next}</label>
            <input id="om-lock-next" className="om-onb__input" type="password" autoComplete="new-password" spellCheck={false} autoCapitalize="none" autoFocus={!needsCurrent} value={next} disabled={busy} onChange={(e) => { setNext(e.target.value); }} />
          </div>
          <div className="om-onb__field">
            <label htmlFor="om-lock-again">{c.password.again}</label>
            <input id="om-lock-again" className="om-onb__input" type="password" autoComplete="new-password" spellCheck={false} autoCapitalize="none" value={again} disabled={busy} onChange={(e) => { setAgain(e.target.value); }} />
          </div>
          <Text as="p" size="sm" tone="secondary">{p.slowNote}</Text>
          {shown && check === "mismatch" ? <p role="alert" data-code="passphrase_mismatch">{p.mismatch}</p> : null}
          {shown && (check === "too_short" || check === "too_long") ? <p role="alert" data-code={`passphrase_${check}`}>{p.tooShort(PASSPHRASE_MIN_CODE_POINTS)}</p> : null}
        </>
      )}
      {busy ? <p role="status">{c.password.working}</p> : null}
      {problem !== null ? <p role="alert">{problem}</p> : null}
      <div className="om-onb__actions">
        <Button type="submit" variant="primary" disabled={busy}>{removing ? c.password.remove : c.password.save}</Button>
        <Button disabled={busy} onClick={onCancel}>{c.password.cancel}</Button>
      </div>
    </form>
  );
}

/** Turning every lock off: said plainly, asked once more, red. A password (if any) is needed to prove it is the owner. */
function StopForm({ needsPassword, busy, problem, onCancel, onConfirm }: {
  readonly needsPassword: boolean; readonly busy: boolean; readonly problem: string | null; readonly onCancel: () => void; readonly onConfirm: (current: string | undefined) => void;
}) {
  const [current, setCurrent] = useState("");
  return (
    <form
      className="om-lock__panel om-lock__panel--danger" aria-labelledby="om-lock-stop-h"
      onSubmit={(e) => { e.preventDefault(); if (needsPassword && current === "") return; onConfirm(needsPassword ? current : undefined); setCurrent(""); }}
    >
      <Text as="h2" size="lg" weight="bold" id="om-lock-stop-h">{c.stop.heading}</Text>
      <p role="note" className="om-lock__danger-note">{c.stop.warning}</p>
      {needsPassword ? (
        <div className="om-onb__field">
          <label htmlFor="om-lock-stop-pass">{c.password.current}</label>
          <input id="om-lock-stop-pass" className="om-onb__input" type="password" autoComplete="current-password" spellCheck={false} autoCapitalize="none" autoFocus value={current} disabled={busy} onChange={(e) => { setCurrent(e.target.value); }} />
        </div>
      ) : null}
      {busy ? <p role="status">{c.password.working}</p> : null}
      {problem !== null ? <p role="alert">{problem}</p> : null}
      <div className="om-onb__actions">
        <Button type="submit" variant="primary" className="om-button--danger" disabled={busy}>{c.stop.confirm}</Button>
        <Button disabled={busy} onClick={onCancel}>{c.stop.cancel}</Button>
      </div>
    </form>
  );
}
