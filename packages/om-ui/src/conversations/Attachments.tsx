import { useEffect, useRef, useState } from "react";
import type { AttachmentRefusal, AttachmentView, ConversationTimelineItem, LocationCardView, MessageId } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Icon } from "../shell/primitives/icons";
import type { AttachmentCopy } from "./copy";
import type { ConversationServices } from "./services";
import "./conversations.css";

/** The name a sender chose is untrusted display text: keep it to one line segment, never a path. */
export const safeFileName = (name: string | undefined, fallback: string): string => {
  const base = Array.from((name ?? "").split(/[\\/]/).pop() ?? "").filter((c) => c.charCodeAt(0) > 0x1f && c.charCodeAt(0) !== 0x7f).join("").trim();
  return base === "" || base === "." || base === ".." ? fallback : base.slice(0, 120);
};

/** Hands decrypted bytes to the browser as a download. The file is never rendered or run inside OM. */
export function saveBytes(bytes: Uint8Array, name: string): boolean {
  if (typeof URL === "undefined" || typeof URL.createObjectURL !== "function") return false;
  const url = URL.createObjectURL(new Blob([bytes as Uint8Array<ArrayBuffer>], { type: "application/octet-stream" }) /* opaque on purpose: nothing a sender names can make the browser open it as a page */);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.rel = "noopener";
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => { URL.revokeObjectURL(url); }, 30_000);
  return true;
}

type Fetch = { readonly status: "idle" | "busy" | "done" } | { readonly status: "failed"; readonly refusal: AttachmentRefusal };

/** Fetching tells the sender's delivery service when and from where you looked, so it only ever happens when the person asks. */
function useOpen(services: ConversationServices, messageId: MessageId, index: number) {
  const [state, setState] = useState<Fetch>({ status: "idle" });
  const alive = useRef(true);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const api = services.client.attachments;
  const run = async () => {
    if (api === undefined || state.status === "busy") return null;
    setState({ status: "busy" });
    try {
      const r = await api.open(services.ctx, messageId, index);
      if (!alive.current) return null;
      if (!r.ok) { setState({ status: "failed", refusal: r.refusal }); return null; }
      setState({ status: "done" });
      return r;
    } catch {
      if (alive.current) setState({ status: "failed", refusal: { error: "unavailable" } });
      return null;
    }
  };
  return { state, run, available: api !== undefined };
}

function storedText(a: AttachmentView, copy: AttachmentCopy): string | null {
  if (a.state === "uploading") return copy.uploading;
  if (a.state === "stored_for_later") return a.storedUntil === undefined ? null : copy.keptUntil(copy.formatDay(a.storedUntil));
  return null;
}

function FileChip({ services, messageId, a, copy }: { readonly services: ConversationServices; readonly messageId: MessageId; readonly a: AttachmentView; readonly copy: AttachmentCopy }) {
  const { state, run, available } = useOpen(services, messageId, a.index);
  const name = safeFileName(a.filename, copy.file);
  const status = storedText(a, copy);
  const download = async () => {
    const r = await run();
    if (r?.ok) saveBytes(r.bytes, safeFileName(r.filename ?? a.filename, copy.file));
  };
  return (
    <div className="om-file" data-state={a.state}>
      <span className="om-file__icon" aria-hidden="true"><Icon name="file" /></span>
      <span className="om-file__meta">
        <bdi className="om-file__name">{name}</bdi>
        <span className="om-file__detail">{copy.size(a.size)}{status === null ? "" : ` · ${status}`}</span>
      </span>
      {a.state === "remote" && available ? (
        <Button variant="quiet" className="om-file__action" aria-label={copy.download(name)} disabled={state.status === "busy"} onClick={() => { void download(); }}>
          <Icon name="download" size={18} /> {state.status === "busy" ? copy.downloading : copy.download("").trim()}
        </Button>
      ) : null}
      <span className="om-file__note" role="status">
        {state.status === "done" ? copy.downloaded : state.status === "failed" ? copy.refusal(state.refusal) : ""}
      </span>
    </div>
  );
}

/** The recorded sound opens only when the person plays it; the browser's own audio controls give play, pause, time and seeking. */
function VoiceNote({ services, messageId, a, waveform, copy }: { readonly services: ConversationServices; readonly messageId: MessageId; readonly a: AttachmentView; readonly waveform: readonly number[] | undefined; readonly copy: AttachmentCopy }) {
  const { state, run, available } = useOpen(services, messageId, a.index);
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => () => { if (url !== null) URL.revokeObjectURL(url); }, [url]);
  const status = storedText(a, copy);
  const play = async () => {
    const r = await run();
    if (r?.ok && typeof URL.createObjectURL === "function") setUrl(URL.createObjectURL(new Blob([r.bytes as Uint8Array<ArrayBuffer>], { type: r.mediaType.startsWith("audio/") ? r.mediaType : "audio/webm" })));
  };
  return (
    <div className="om-voice" data-state={a.state}>
      <span className="om-voice__head">
        <Icon name="mic" size={18} />
        <span className="om-voice__label">{copy.voice.note}</span>
        {a.durationMs === undefined ? null : <span className="om-voice__time">{copy.voice.duration(a.durationMs)}</span>}
        {status === null ? null : <span className="om-voice__detail">{status}</span>}
      </span>
      {waveform === undefined ? null : (
        <span className="om-voice__wave" aria-hidden="true">{waveform.slice(0, 64).map((v, i) => <span key={i} style={{ blockSize: `${String(Math.max(8, Math.round((v / 255) * 100)))}%` }} />)}</span>
      )}
      {url !== null ? <audio className="om-voice__audio" controls autoPlay src={url} aria-label={copy.voice.note} /> : a.state === "remote" && available ? (
        <Button variant="secondary" disabled={state.status === "busy"} onClick={() => { void play(); }}>
          <Icon name="play" size={16} /> {state.status === "busy" ? copy.voice.loading : copy.voice.play}
        </Button>
      ) : null}
      <span className="om-file__note" role="status">{state.status === "failed" ? copy.refusal(state.refusal) : ""}</span>
    </div>
  );
}

const degrees = (e7: number): string => (e7 / 10_000_000).toFixed(4);

/** A one-shot point: the numbers, a label and how sure the phone was. No map is fetched, so opening a chat tells nobody where you looked. */
export function LocationCard({ card, copy }: { readonly card: LocationCardView; readonly copy: AttachmentCopy }) {
  return (
    <div className="om-loc" role="group" aria-label={copy.location.card}>
      <span className="om-loc__icon" aria-hidden="true"><Icon name="pin" /></span>
      <span className="om-loc__meta">
        <bdi className="om-loc__label">{card.label ?? copy.location.card}</bdi>
        <span className="om-loc__point">{copy.location.point(degrees(card.latE7), degrees(card.lonE7))}</span>
        {card.accuracyM === undefined ? null : <span className="om-loc__accuracy">{copy.location.accuracy(card.accuracyM)}</span>}
      </span>
    </div>
  );
}

/** Everything a message carries besides its text. The text is always shown too (a card never replaces it). */
export function MessageAttachments({ services, item, copy }: { readonly services: ConversationServices; readonly item: ConversationTimelineItem; readonly copy: AttachmentCopy }) {
  const files = item.attachments ?? [];
  const voice = item.presentation?.voiceNote;
  const card = item.presentation?.locationCard;
  if (files.length === 0 && card === undefined) return null;
  return (
    <div className="om-attachments">
      {files.map((a) => (voice?.attachmentIndex === a.index
        ? <VoiceNote key={a.index} services={services} messageId={item.messageId} a={a} waveform={voice.waveform} copy={copy} />
        : <FileChip key={a.index} services={services} messageId={item.messageId} a={a} copy={copy} />))}
      {card === undefined ? null : <LocationCard card={card} copy={copy} />}
    </div>
  );
}
