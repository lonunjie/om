import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent, ReactNode, SyntheticEvent } from "react";
import type { AudioRecording, ConversationId, CoreIntent, FilesApi, LocationApi, LocationFix, MicrophoneApi, OperationHandle, OutcomeClass } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Icon } from "../shell/primitives/icons";
import { IconButton } from "../shell/primitives/IconButton";
import type { ConversationCopy } from "./copy";
import { firstOutcome, resolveIntent, type ConversationServices } from "./services";
import { safeFileName } from "./Attachments";
import "./conversations.css";

export interface InputPrivacy { readonly spellcheck: boolean; readonly autocomplete: "on" | "off" }
const PRIVATE_DEFAULT: InputPrivacy = { spellcheck: false, autocomplete: "off" };

/** What this device can capture. A control appears only for a family that is present; files and voice also need Core's file path. */
export interface ComposerCapture {
  readonly files?: FilesApi;
  readonly microphone?: MicrophoneApi;
  readonly location?: LocationApi;
}

/** Something chosen or recorded and waiting for Send. Held in memory only; Core seals it when the message is sent. */
export interface StagedItem {
  readonly id: string;
  readonly kind: "file" | "voice";
  readonly name: string;
  readonly mediaType: string;
  readonly size: number;
  readonly durationMs?: number;
  readonly bytes: Uint8Array;
  /** Set once Core has sealed it, so a retry of the same Send does not seal it again. */
  readonly blobId?: string;
}

export interface ComposerProps {
  readonly services: ConversationServices;
  readonly conversationId: ConversationId;
  readonly copy: ConversationCopy;
  /** Controlled draft: the parent owns it, so it survives mode transitions and remounts. */
  readonly draft: string;
  readonly onDraftChange: (next: string) => void;
  readonly inputPrivacy?: InputPrivacy;
  readonly onSent?: (handle: OperationHandle) => void;
  /** Controls placed immediately before Send, in the strong lower-trailing reach area (the Omnibar trigger; OMNIBAR_TRIGGER_AND_TRANSITION.md s2). */
  readonly trailing?: ReactNode;
  readonly capture?: ComposerCapture;
  /** Controlled tray (the parent keeps it across remounts); without it the Composer keeps its own. */
  readonly staged?: readonly StagedItem[];
  readonly onStagedChange?: (next: readonly StagedItem[]) => void;
}

/** Clear the draft only for these; Rejected and NeedsUserAction leave it intact. */
const MAX_INPUT_PX = 160;
/** The fixed overhead Core adds to every sealed file; a file this close to the limit is refused before anything is read. */
const SEAL_OVERHEAD = 16;
const MAX_STAGED = 8;
/** A keyboard with a real Enter key sends on Enter (Shift+Enter adds a line); a touch screen keeps Enter as a new line and sends with the button. */
const enterSends = (): boolean => typeof matchMedia === "function" && matchMedia("(pointer: fine)").matches;

const clearsDraft = (o: OutcomeClass): boolean => o.kind === "Committed" || o.kind === "PendingAuthorization" || o.kind === "PendingNetwork" || o.kind === "PendingReconciliation";

type Recording = { readonly status: "starting" } | { readonly status: "on"; readonly rec: AudioRecording; readonly startedAt: number };
type Place = { readonly status: "asking" } | { readonly status: "confirm"; readonly fix: LocationFix };

let stagedSeq = 0;
const degrees = (e7: number): string => (e7 / 10_000_000).toFixed(4);

/** The sound just recorded, previewable before it is sent. The object URL lives exactly as long as the item. */
function VoicePreview({ item, label }: { readonly item: StagedItem; readonly label: string }) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (typeof URL.createObjectURL !== "function") return undefined;
    const u = URL.createObjectURL(new Blob([item.bytes as Uint8Array<ArrayBuffer>], { type: item.mediaType }));
    setUrl(u);
    return () => { URL.revokeObjectURL(u); };
  }, [item]);
  return url === null ? null : <audio className="om-staged__audio" controls src={url} aria-label={label} />;
}

export function Composer({ services, conversationId, copy, draft, onDraftChange, inputPrivacy = PRIVATE_DEFAULT, onSent, trailing, capture, staged, onStagedChange }: ComposerProps) {
  const att = copy.attachments;
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const input = useRef<HTMLTextAreaElement>(null);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  /** A function, so a check after an await is not narrowed away: the component may have unmounted meanwhile. */
  const alive = (): boolean => mounted.current;
  // The box grows with what is typed, up to a few lines, then scrolls.
  useEffect(() => {
    const el = input.current;
    if (el === null) return;
    el.style.blockSize = "auto";
    el.style.blockSize = `${String(Math.min(el.scrollHeight, MAX_INPUT_PX))}px`;
  }, [draft]);
  // After a dispatch error or NeedsUserAction, resubmitting the same text re-dispatches the same intent (same OperationId).
  const inflight = useRef<{ readonly key: string; readonly intent: CoreIntent } | null>(null);

  // ---- the tray ----
  const [ownStaged, setOwnStaged] = useState<readonly StagedItem[]>([]);
  const items = staged ?? ownStaged;
  const itemsRef = useRef(items);
  itemsRef.current = items;
  const setItems = (next: readonly StagedItem[]): void => {
    itemsRef.current = next;
    if (onStagedChange) onStagedChange(next); else setOwnStaged(next);
  };
  const hasVoice = items.some((i) => i.kind === "voice");

  const files = capture?.files;
  const microphone = capture?.microphone;
  const place = capture?.location;
  const canAttach = files !== undefined && services.client.attachments !== undefined;
  const canRecord = microphone !== undefined && services.client.attachments !== undefined;

  /** Said before anything is read or sent: the delivery service's own advertised ceiling. `null`: no limit known yet. */
  const tooBig = (size: number): boolean => {
    const limits = services.client.attachments?.limits();
    return limits !== null && limits !== undefined && size + SEAL_OVERHEAD > limits.maxBlobBytes;
  };
  const maxFile = (): number | null => {
    const limits = services.client.attachments?.limits();
    return limits === null || limits === undefined ? null : Math.max(0, limits.maxBlobBytes - SEAL_OVERHEAD);
  };

  const pickFiles = async (): Promise<void> => {
    if (files === undefined) return;
    setNotice(null);
    let picked;
    try { picked = await files.pick({ multiple: true }); } catch { return; }
    if (!alive()) return;
    const next = [...itemsRef.current];
    let refused: string | null = null;
    for (const f of picked) {
      if (next.length >= MAX_STAGED) break;
      if (tooBig(f.size)) { refused = att.tooBig(maxFile()); continue; }
      try {
        const bytes = await f.bytes();
        if (!alive()) return;
        next.push({ id: `s${String(++stagedSeq)}`, kind: "file", name: safeFileName(f.name, att.file), mediaType: f.mimeType === "" ? "application/octet-stream" : f.mimeType, size: bytes.length, bytes });
      } catch { refused = att.refusal({ error: "unavailable" }); }
    }
    setItems(next);
    if (refused !== null) setNotice(refused);
  };

  // ---- voice note ----
  const [recording, setRecording] = useState<Recording | null>(null);
  const recRef = useRef<AudioRecording | null>(null);
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    if (recording?.status !== "on") return undefined;
    const t = setInterval(() => { setElapsed(Date.now() - recording.startedAt); }, 500);
    return () => { clearInterval(t); };
  }, [recording]);
  // The microphone is never left open: leaving the conversation or the screen ends the recording and drops it.
  useEffect(() => () => { recRef.current?.cancel(); recRef.current = null; }, [conversationId]);

  const startVoice = async (): Promise<void> => {
    if (microphone === undefined || recording !== null || hasVoice) return;
    setNotice(null);
    setRecording({ status: "starting" });
    setElapsed(0);
    let r: AudioRecording | "denied" | "failed";
    try { r = await microphone.startRecording(); } catch { r = "failed"; }
    if (!alive()) { if (typeof r === "object") r.cancel(); return; }
    if (r === "denied" || r === "failed") { setRecording(null); setNotice(r === "denied" ? att.voice.denied : att.voice.failed); return; }
    recRef.current = r;
    setRecording({ status: "on", rec: r, startedAt: Date.now() });
  };
  const stopVoice = async (): Promise<void> => {
    const rec = recRef.current;
    if (rec === null) return;
    recRef.current = null;
    try {
      const out = await rec.stop();
      if (!alive()) return;
      setRecording(null);
      if (out.bytes.length === 0) { setNotice(att.voice.nothing); return; }
      if (tooBig(out.bytes.length)) { setNotice(att.tooBig(maxFile())); return; }
      setItems([...itemsRef.current, { id: `s${String(++stagedSeq)}`, kind: "voice", name: att.voice.note, mediaType: out.mimeType, size: out.bytes.length, durationMs: out.durationMs, bytes: out.bytes }]);
    } catch {
      if (mounted.current) { setRecording(null); setNotice(att.voice.failed); }
    }
  };
  const discardVoice = (): void => {
    recRef.current?.cancel();
    recRef.current = null;
    setRecording(null);
  };

  // ---- one-shot location ----
  const [spot, setSpot] = useState<Place | null>(null);
  const locInflight = useRef<{ readonly key: string; readonly intent: CoreIntent } | null>(null);
  const askLocation = async (): Promise<void> => {
    if (place === undefined || spot !== null) return;
    setNotice(null);
    setSpot({ status: "asking" });
    let fix: LocationFix | "denied" | "unavailable";
    try { fix = await place.current(); } catch { fix = "unavailable"; }
    if (!alive()) return;
    if (fix === "denied" || fix === "unavailable") { setSpot(null); setNotice(fix === "denied" ? att.location.denied : att.location.unavailable); return; }
    setSpot({ status: "confirm", fix });
  };
  const panel = useRef<HTMLDivElement>(null);
  const confirming = spot?.status === "confirm";
  useEffect(() => { if (confirming) panel.current?.focus(); }, [confirming]);

  /** Dispatches and reads the first outcome; the shared tail of every kind of send. */
  const settle = async (intent: CoreIntent): Promise<{ readonly handle: OperationHandle; readonly outcome: OutcomeClass }> => {
    const handle = await services.client.dispatch(intent);
    return { handle, outcome: await firstOutcome(services, handle.operationId) };
  };

  const sendLocation = async (fix: LocationFix): Promise<void> => {
    if (busy) return;
    setBusy(true);
    setNotice(null);
    try {
      const key = `${String(fix.latE7)},${String(fix.lonE7)},${String(fix.accuracyM)}`;
      let intent = locInflight.current?.key === key ? locInflight.current.intent : null;
      if (intent === null) {
        const params: Record<string, unknown> = { conversationId, latE7: fix.latE7, lonE7: fix.lonE7 };
        if (fix.accuracyM !== undefined) params["accuracyM"] = Math.min(1_000_000, Math.max(1, Math.round(fix.accuracyM)));
        const r = resolveIntent(services, "send-location", params);
        if (r.status !== "intent") { setNotice(copy.composer.sendFailed); return; }
        intent = r.intent;
        locInflight.current = { key, intent };
      }
      const { handle, outcome } = await settle(intent);
      if (!alive()) return;
      if (clearsDraft(outcome)) { locInflight.current = null; setSpot(null); onSent?.(handle); }
      else if (outcome.kind === "Rejected") { locInflight.current = null; setSpot(null); setNotice(copy.composer.rejected[outcome.reason]); }
      else if (outcome.kind === "NeedsUserAction") setNotice(copy.composer.needsAction[outcome.action]);
    } catch {
      if (mounted.current) setNotice(copy.composer.sendFailed);
    } finally {
      if (mounted.current) setBusy(false);
    }
  };

  // ---- send ----
  const sendable = draft.trim() !== "" || items.length > 0;
  const submit = async (): Promise<void> => {
    const text = draftRef.current;
    const tray = itemsRef.current;
    if (busy || recording !== null || (text.trim() === "" && tray.length === 0)) return;
    setBusy(true);
    setNotice(null);
    try {
      let resolved = tray;
      if (tray.length > 0) {
        const api = services.client.attachments;
        if (api === undefined) { setNotice(att.refusal({ error: "unsupported" })); return; }
        // Seal each once: a later retry reuses the blob Core already holds.
        for (const item of tray) {
          if (item.blobId !== undefined) continue;
          const r = await api.prepare(services.ctx, item.bytes, { mediaType: item.mediaType, ...(item.kind === "file" ? { filename: item.name } : {}), ...(item.durationMs === undefined ? {} : { durationMs: item.durationMs }) });
          if (!alive()) return;
          if (!r.ok) {
            // A file the service will not take can never be sent as it is, so it leaves the tray; anything else stays for another try.
            if (r.refusal.error === "too_large") setItems(itemsRef.current.filter((i) => i.id !== item.id));
            setNotice(att.refusal(r.refusal));
            return;
          }
          resolved = resolved.map((i) => (i.id === item.id ? { ...i, blobId: r.blobId } : i));
          setItems(itemsRef.current.map((i) => (i.id === item.id ? { ...i, blobId: r.blobId } : i)));
        }
      }
      const key = `${text}\u0000${resolved.map((i) => i.blobId ?? i.id).join(",")}`;
      let intent = inflight.current?.key === key ? inflight.current.intent : null;
      if (intent === null) {
        const r = resolved.length === 0
          ? resolveIntent(services, "send-message", { conversationId, text })
          : resolveIntent(services, "send-attachments", {
            conversationId, text,
            attachments: resolved.map((i) => ({ blobId: i.blobId ?? "", ...(i.kind === "voice" ? { voiceNote: {} } : {}) })),
          });
        if (r.status !== "intent") { setNotice(copy.composer.sendFailed); return; }
        intent = r.intent;
        inflight.current = { key, intent };
      }
      const { handle, outcome } = await settle(intent);
      if (!alive()) { if (clearsDraft(outcome) && draftRef.current === text) onDraftChange(""); return; }
      if (clearsDraft(outcome)) {
        inflight.current = null;
        if (draftRef.current === text) onDraftChange("");
        if (resolved.length > 0) setItems(itemsRef.current.filter((i) => !resolved.some((r) => r.id === i.id)));
        onSent?.(handle);
      } else if (outcome.kind === "Rejected") {
        inflight.current = null;
        // The service said no to a file: it cannot be sent as it is (no retry), the words stay.
        if (outcome.reason === "too_large") setItems(itemsRef.current.filter((i) => !resolved.some((r) => r.id === i.id)));
        setNotice(copy.composer.rejected[outcome.reason]);
      } else if (outcome.kind === "NeedsUserAction") {
        setNotice(copy.composer.needsAction[outcome.action]);
      }
    } catch {
      if (mounted.current) setNotice(copy.composer.sendFailed);
    } finally {
      if (mounted.current) setBusy(false);
    }
  };

  const onSubmit = (e: SyntheticEvent): void => { e.preventDefault(); void submit(); };
  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>): void => {
    if (e.key !== "Enter" || e.nativeEvent.isComposing) return;
    if (e.ctrlKey || e.metaKey || (!e.shiftKey && enterSends())) { e.preventDefault(); void submit(); }
  };

  const tools = canAttach || canRecord || place !== undefined;
  return (
    <form className="om-composer" onSubmit={onSubmit}>
      {items.length === 0 ? null : (
        <ul className="om-staged" aria-label={att.tray}>
          {items.map((item) => (
            <li key={item.id} className="om-staged__item" data-kind={item.kind}>
              <span className="om-staged__icon" aria-hidden="true"><Icon name={item.kind === "voice" ? "mic" : "file"} size={18} /></span>
              <span className="om-staged__meta">
                <bdi className="om-staged__name">{item.kind === "voice" ? att.voice.review : item.name}</bdi>
                <span className="om-staged__size">{item.durationMs === undefined ? att.size(item.size) : `${att.voice.duration(item.durationMs)} · ${att.size(item.size)}`}</span>
              </span>
              {item.kind === "voice" ? <VoicePreview item={item} label={att.voice.review} /> : null}
              <IconButton className="om-staged__remove" label={att.removeStaged(item.kind === "voice" ? att.voice.note : item.name)} icon={<Icon name="close" size={18} />} disabled={busy} onClick={() => { setItems(itemsRef.current.filter((i) => i.id !== item.id)); }} />
            </li>
          ))}
        </ul>
      )}
      {recording === null ? null : (
        <div className="om-recording" role="group" aria-label={att.voice.recording}>
          <span className="om-recording__dot" aria-hidden="true" />
          <span className="om-recording__label" role="status">{att.voice.recording}</span>
          <span className="om-recording__time" aria-hidden="true">{att.voice.elapsed(elapsed)}</span>
          <Button variant="primary" disabled={recording.status !== "on"} onClick={() => { void stopVoice(); }}>{att.voice.stop}</Button>
          <Button variant="quiet" onClick={discardVoice}>{att.voice.discard}</Button>
        </div>
      )}
      {spot === null ? null : (
        <div ref={panel} className="om-location-confirm" role="group" aria-labelledby="om-loc-h" tabIndex={-1}>
          {spot.status === "asking" ? <p role="status">{att.location.asking}</p> : (
            <>
              <p id="om-loc-h" className="om-location-confirm__heading">{att.location.heading}</p>
              <p className="om-location-confirm__point">{att.location.point(degrees(spot.fix.latE7), degrees(spot.fix.lonE7))}{spot.fix.accuracyM === undefined ? "" : ` · ${att.location.accuracy(Math.max(1, Math.round(spot.fix.accuracyM)))}`}</p>
              <p>{att.location.confirm}</p>
              <div className="om-location-confirm__actions">
                <Button variant="primary" disabled={busy} onClick={() => { void sendLocation(spot.fix); }}>{att.location.send}</Button>
                <Button variant="quiet" disabled={busy} onClick={() => { setSpot(null); locInflight.current = null; }}>{att.location.cancel}</Button>
              </div>
            </>
          )}
        </div>
      )}
      {tools ? (
        <div className="om-composer__tools">
          {canAttach ? <IconButton label={att.attach} icon={<Icon name="attach" />} disabled={busy || items.length >= MAX_STAGED} onClick={() => { void pickFiles(); }} /> : null}
          {canRecord ? <IconButton label={att.recordVoice} icon={<Icon name="mic" />} disabled={busy || recording !== null || hasVoice} onClick={() => { void startVoice(); }} /> : null}
          {place === undefined ? null : <IconButton label={att.shareLocation} icon={<Icon name="pin" />} disabled={busy || spot !== null} onClick={() => { void askLocation(); }} />}
        </div>
      ) : null}
      <textarea
        ref={input} className="om-composer__input" aria-label={copy.composer.label} placeholder={copy.composer.placeholder} rows={1}
        value={draft} spellCheck={inputPrivacy.spellcheck} autoComplete={inputPrivacy.autocomplete} autoCorrect={inputPrivacy.autocomplete}
        onChange={(e) => { onDraftChange(e.target.value); }} onKeyDown={onKeyDown}
      />
      {trailing}
      <IconButton type="submit" className="om-composer__send" data-variant="primary" label={copy.composer.send} icon={<Icon name="send" />} disabled={busy || recording !== null || !sendable} />
      {notice === null ? null : <p className="om-composer__notice" role="alert">{notice}</p>}
    </form>
  );
}
