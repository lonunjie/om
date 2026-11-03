// Settings > Backup file: save the chats to one file, and read them back from one. Says what a backup holds and does not hold before
// asking for anything. The password is typed twice to save and once to restore; nothing about how the file is locked is shown.
import { useEffect, useRef, useState, type SyntheticEvent } from "react";
import { Button, Icon, PASSPHRASE_MIN_CODE_POINTS, Text, checkNewPassphrase } from "@om/ui";
import { backupCopy as c, onboardingCopy } from "../copy/en";
import { MAX_BACKUP_BYTES, downloadBytes, type BackupFailure, type BackupServices } from "./backupServices";

type Notice = { readonly tone: "done" | "problem"; readonly text: string } | null;

const problem = (failure: BackupFailure | "no_chats"): Notice => ({ tone: "problem", text: c.errors[failure] });

export function BackupScreen({ services, download = downloadBytes }: { readonly services: BackupServices; readonly download?: (bytes: Uint8Array, name: string) => void }) {
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  return (
    <section className="om-lock om-backup" aria-labelledby="om-backup-h">
      <Text as="h1" size="xl" weight="bold" id="om-backup-h">{c.heading}</Text>
      <Text as="p">{c.intro}</Text>
      <div className="om-lock__state om-backup__holds" data-mode="device">
        <span className="om-lock__icon" aria-hidden="true"><Icon name="shield" size={22} /></span>
        <div className="om-lock__text">
          <Text as="h2" size="md" weight="bold">{c.holds.heading}</Text>
          <Text as="p" size="sm">{c.holds.yes}</Text>
          <Text as="p" size="sm">{c.holds.no}</Text>
          <Text as="p" size="sm" weight="bold" role="note">{c.holds.care}</Text>
        </div>
      </div>
      <SaveBackup services={services} download={download} alive={mounted} />
      <RestoreBackup services={services} alive={mounted} />
    </section>
  );
}

function SaveBackup({ services, download, alive }: { readonly services: BackupServices; readonly download: (bytes: Uint8Array, name: string) => void; readonly alive: { readonly current: boolean } }) {
  const p = onboardingCopy.protect.passphrase;
  const [password, setPassword] = useState("");
  const [again, setAgain] = useState("");
  const [shown, setShown] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<Notice>(null);
  const check = checkNewPassphrase(password, again);

  const submit = (e: SyntheticEvent): void => {
    e.preventDefault();
    if (check !== null) { setShown(true); return; }
    setBusy(true);
    setNotice(null);
    const typed = password;
    setPassword(""); setAgain(""); setShown(false);
    void services.save(typed).then((r) => {
      if (!alive.current) return;
      if (r.ok) { download(r.bytes, r.name); setNotice({ tone: "done", text: c.save.done(r.name) }); } else setNotice(r.failure === "no_chats" ? { tone: "problem", text: c.save.nothing } : problem(r.failure));
    }).finally(() => { if (alive.current) setBusy(false); });
  };
  return (
    <form className="om-lock__panel" aria-labelledby="om-backup-save-h" onSubmit={submit}>
      <Text as="h2" size="lg" weight="bold" id="om-backup-save-h">{c.save.heading}</Text>
      <p role="note" className="om-onb__notice">{c.save.warning}</p>
      <div className="om-onb__field">
        <label htmlFor="om-backup-pass">{c.save.password}</label>
        <input id="om-backup-pass" className="om-onb__input" type="password" autoComplete="new-password" spellCheck={false} autoCapitalize="none" value={password} disabled={busy} onChange={(e) => { setPassword(e.target.value); }} />
      </div>
      <div className="om-onb__field">
        <label htmlFor="om-backup-again">{c.save.again}</label>
        <input id="om-backup-again" className="om-onb__input" type="password" autoComplete="new-password" spellCheck={false} autoCapitalize="none" value={again} disabled={busy} onChange={(e) => { setAgain(e.target.value); }} />
      </div>
      <Text as="p" size="sm" tone="secondary">{c.save.slow}</Text>
      {shown && check === "mismatch" ? <p role="alert" data-code="passphrase_mismatch">{p.mismatch}</p> : null}
      {shown && (check === "too_short" || check === "too_long") ? <p role="alert" data-code={`passphrase_${check}`}>{p.tooShort(PASSPHRASE_MIN_CODE_POINTS)}</p> : null}
      {busy ? <p role="status">{c.save.working}</p> : null}
      <NoticeLine notice={notice} />
      <div className="om-onb__actions"><Button type="submit" variant="primary" disabled={busy}><Icon name="check" size={20} />{c.save.action}</Button></div>
    </form>
  );
}

function RestoreBackup({ services, alive }: { readonly services: BackupServices; readonly alive: { readonly current: boolean } }) {
  const [file, setFile] = useState<File | null>(null);
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<Notice>(null);
  const input = useRef<HTMLInputElement>(null);

  const read = async (f: File): Promise<Uint8Array | null> => {
    if (f.size > MAX_BACKUP_BYTES) { setNotice(problem("too_large")); return null; }
    try { return new Uint8Array(await f.arrayBuffer()); } catch { setNotice(problem("failed")); return null; }
  };
  const pick = (f: File | null): void => {
    setFile(f);
    setNotice(null);
    if (f === null) return;
    // Say at once that this is not a backup (or is from a newer OM); no password is needed for that.
    void read(f).then((bytes) => {
      if (!alive.current || bytes === null) return;
      const why = services.check(bytes);
      if (why !== null) setNotice(problem(why));
    });
  };
  const submit = (e: SyntheticEvent): void => {
    e.preventDefault();
    if (file === null || busy) return;
    if (password === "") { setNotice(problem("empty_password")); return; }
    setBusy(true);
    setNotice(null);
    void (async () => {
      const bytes = await read(file);
      if (bytes === null) return;
      const r = await services.restore(bytes, password);
      if (!alive.current) return;
      if (!r.ok) { setNotice(problem(r.failure)); return; }
      const s = r.summary;
      const parts = [s.chatsAdded + s.chatsUpdated === 0 ? c.restore.nothingNew : c.restore.done(s.chatsAdded + s.chatsUpdated, s.messagesAdded)];
      if (s.chatsSkipped > 0) parts.push(c.restore.already(s.chatsSkipped));
      setNotice({ tone: "done", text: parts.join(" ") });
      setPassword("");
      setFile(null);
      if (input.current) input.current.value = "";
    })().finally(() => { if (alive.current) setBusy(false); });
  };
  return (
    <form className="om-lock__panel" aria-labelledby="om-backup-restore-h" onSubmit={submit}>
      <Text as="h2" size="lg" weight="bold" id="om-backup-restore-h">{c.restore.heading}</Text>
      <Text as="p" size="sm">{c.restore.hint}</Text>
      <div className="om-onb__field">
        <label htmlFor="om-backup-file">{c.restore.file}</label>
        <input id="om-backup-file" ref={input} className="om-onb__input" type="file" accept=".ombackup,application/octet-stream" disabled={busy} onChange={(e) => { pick(e.target.files?.[0] ?? null); }} />
      </div>
      <div className="om-onb__field">
        <label htmlFor="om-backup-restore-pass">{c.restore.password}</label>
        <input id="om-backup-restore-pass" className="om-onb__input" type="password" autoComplete="off" spellCheck={false} autoCapitalize="none" value={password} disabled={busy} onChange={(e) => { setPassword(e.target.value); }} />
      </div>
      {busy ? <p role="status">{c.restore.working}</p> : null}
      <NoticeLine notice={notice} />
      <div className="om-onb__actions"><Button type="submit" variant="primary" disabled={busy || file === null}>{c.restore.action}</Button></div>
    </form>
  );
}

function NoticeLine({ notice }: { readonly notice: Notice }) {
  if (notice === null) return null;
  return notice.tone === "problem" ? <p role="alert">{notice.text}</p> : <p role="status" className="om-backup__done">{notice.text}</p>;
}
