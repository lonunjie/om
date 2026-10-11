// Camera QR scanning for the invite flow. The camera opens only after the user's click (InviteFirst asks for access
// first) and every track is stopped when the dialog closes, however it closes.
import { useCallback, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { scanLoop } from "@om/web-platform";
import { Button, Text } from "@om/ui";

export interface ScannerCopy {
  readonly heading: string;
  readonly hint: string;
  readonly cancel: string;
  readonly cameraFailed: string;
  readonly videoLabel: string;
}

/** Longest frame side handed to the decoder; larger frames are scaled down for speed. */
const MAX_SIDE = 720;

interface Pending { readonly resolve: (text: string | null) => void }

export function useQrScanner(copy: ScannerCopy): { readonly scanQr: () => Promise<string | null>; readonly dialog: ReactNode } {
  const [open, setOpen] = useState(false);
  const pending = useRef<Pending | null>(null);
  const scanQr = useCallback((): Promise<string | null> => new Promise((resolve) => {
    pending.current?.resolve(null); // a second request supersedes the first
    pending.current = { resolve };
    setOpen(true);
  }), []);
  const finish = useCallback((text: string | null) => {
    const p = pending.current;
    pending.current = null;
    p?.resolve(text);
    setOpen(false);
  }, []);
  const dialog = open ? <ScanDialog copy={copy} onDone={finish} /> : null;
  return { scanQr, dialog };
}

function ScanDialog({ copy, onDone }: { readonly copy: ScannerCopy; readonly onDone: (text: string | null) => void }) {
  const video = useRef<HTMLVideoElement>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    const ac = new AbortController();
    let stream: MediaStream | null = null;
    let closed = false;
    const canvas = document.createElement("canvas");
    const ctx = canvas.getContext("2d", { willReadFrequently: true });
    const stopped = (): boolean => ac.signal.aborted; // a function call: TS would narrow the property across awaits
    const finishOnce = (text: string | null): void => { if (!closed) { closed = true; onDone(text); } };

    void (async () => {
      try {
        stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: "environment" }, audio: false });
        if (stopped()) return;
        const v = video.current;
        if (!v || !ctx) throw new Error("no video");
        v.srcObject = stream;
        await v.play();
        const text = await scanLoop({
          signal: ac.signal,
          nextFrame: () => {
            if (v.videoWidth === 0 || v.videoHeight === 0) return null;
            const scale = Math.min(1, MAX_SIDE / Math.max(v.videoWidth, v.videoHeight));
            canvas.width = Math.round(v.videoWidth * scale);
            canvas.height = Math.round(v.videoHeight * scale);
            ctx.drawImage(v, 0, 0, canvas.width, canvas.height);
            const img = ctx.getImageData(0, 0, canvas.width, canvas.height);
            return { width: img.width, height: img.height, data: img.data };
          },
        });
        if (!stopped()) finishOnce(text);
      } catch {
        if (!stopped()) setFailed(true);
      }
    })();

    return () => {
      ac.abort();
      for (const t of stream?.getTracks() ?? []) t.stop();
      // Closing without a result (cancel, unmount) resolves the pending promise so the caller never hangs.
      finishOnce(null);
    };
  }, [onDone]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => { if (e.key === "Escape") onDone(null); };
    document.addEventListener("keydown", onKey);
    return () => { document.removeEventListener("keydown", onKey); };
  }, [onDone]);

  return (
    <div className="om-scan" role="dialog" aria-modal="true" aria-labelledby="om-scan-h">
      <Text as="h2" id="om-scan-h">{copy.heading}</Text>
      {failed ? <Text role="alert">{copy.cameraFailed}</Text> : <Text as="p">{copy.hint}</Text>}
      <video ref={video} className="om-scan__video" aria-label={copy.videoLabel} playsInline muted />
      <Button onClick={() => { onDone(null); }} autoFocus>{copy.cancel}</Button>
    </div>
  );
}
