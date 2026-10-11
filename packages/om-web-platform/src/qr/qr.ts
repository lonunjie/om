// QR encode/decode (@paulmillr/qr: zero-dependency, auditable). Pure: no DOM types, no camera, no network.
// A scanned code is hostile input: it is size-bounded here and still has to pass the caller's own link parser.
import encodeQR from "@paulmillr/qr";
import decodeQR from "@paulmillr/qr/decode.js";

/** Invite links are short; anything larger is refused before it reaches a parser. */
export const QR_MAX_PAYLOAD_CHARS = 2048;

export interface QrFrame {
  readonly width: number;
  readonly height: number;
  /** RGBA, four bytes per pixel. */
  readonly data: Uint8Array | Uint8ClampedArray;
}

/** Dark-module matrix without a quiet zone (the renderer adds it), or null when the text cannot be encoded. */
export function qrModules(text: string): readonly (readonly boolean[])[] | null {
  if (text === "" || text.length > QR_MAX_PAYLOAD_CHARS) return null;
  try {
    return encodeQR(text, "raw", { ecc: "medium", border: 0 });
  } catch {
    return null;
  }
}

/** One frame in, text out. Null for "no readable code here" (the normal case for most camera frames). */
export function decodeQrFrame(frame: QrFrame): string | null {
  if (frame.width <= 0 || frame.height <= 0 || frame.data.length < frame.width * frame.height * 4) return null;
  try {
    const text = decodeQR({ width: frame.width, height: frame.height, data: frame.data });
    return text === "" || text.length > QR_MAX_PAYLOAD_CHARS ? null : text;
  } catch {
    return null;
  }
}

export interface ScanLoopOptions {
  /** Next camera frame, or null when none is ready yet. */
  readonly nextFrame: () => QrFrame | null;
  readonly signal: AbortSignal;
  readonly intervalMs?: number;
  readonly timeoutMs?: number;
  /** Injected so tests do not need real timers. */
  readonly sleep?: (ms: number, signal: AbortSignal) => Promise<void>;
}

const defaultSleep = (ms: number, signal: AbortSignal): Promise<void> =>
  new Promise((resolve) => {
    const id = setTimeout(resolve, ms);
    signal.addEventListener("abort", () => { clearTimeout(id); resolve(); }, { once: true });
  });

/** Polls frames until a code decodes, the signal aborts, or the time limit passes. Resolves null on abort or timeout. */
export async function scanLoop(o: ScanLoopOptions): Promise<string | null> {
  const interval = o.intervalMs ?? 150;
  const deadline = Date.now() + (o.timeoutMs ?? 60_000);
  const sleep = o.sleep ?? defaultSleep;
  while (!o.signal.aborted && Date.now() < deadline) {
    const frame = o.nextFrame();
    if (frame !== null) {
      const text = decodeQrFrame(frame);
      if (text !== null) return text;
    }
    await sleep(interval, o.signal);
  }
  return null;
}
