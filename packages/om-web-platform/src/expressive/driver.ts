/// <reference types="vite/client" />

import type {
  ExpressiveDecodeFallback, ExpressiveDecodeResult, ExpressiveFrameResult, ExpressivePlayback, ValidatedExpressiveAsset,
} from "@om/platform-api";
import workerAsset from "./decoder.worker?worker&url";
import { checkBudget, DECODE_TIME_MS } from "./budget";
import type { DecodeReply, DecodeRequest } from "./decoder.worker";

type DecoderWorker = Pick<Worker, "onmessage" | "onerror" | "onmessageerror" | "postMessage" | "terminate">;
type WorkerPolicy = { createScriptURL(url: string): unknown };
let policy: WorkerPolicy | undefined;

function startWorker(): Worker {
  const url = new URL(workerAsset, import.meta.url);
  if (url.origin !== location.origin) throw new Error("Invalid expressive worker origin");
  const types = (globalThis as { trustedTypes?: {
    createPolicy(name: string, rules: { createScriptURL(url: string): string }): WorkerPolicy;
  } }).trustedTypes;
  policy ??= types?.createPolicy("om-expressive-worker", {
    createScriptURL(value) {
      if (value !== url.href) throw new Error("Invalid expressive worker URL");
      return value;
    },
  });
  return new Worker((policy?.createScriptURL(url.href) ?? url.href) as string);
}

const fallback = (reason: ExpressiveDecodeFallback["reason"]): ExpressiveDecodeFallback => ({ state: "fallback", reason });

export async function decodeExpressiveAsset(
  asset: ValidatedExpressiveAsset, reducedMotion: boolean, createWorker: () => DecoderWorker = startWorker,
): Promise<ExpressiveDecodeResult> {
  let worker: DecoderWorker;
  let bytes: Uint8Array<ArrayBuffer>;
  try {
    const failure = checkBudget(asset);
    if (failure !== undefined) return fallback(failure);
    bytes = Uint8Array.from(asset.bytes);
    worker = createWorker();
  } catch {
    return fallback("unavailable");
  }

  let remainingMs = DECODE_TIME_MS;
  let closed = false;
  let failure: ExpressiveDecodeFallback | undefined;
  let finishPending: ((result: DecodeReply) => void) | undefined;
  const close = (): void => {
    if (closed) return;
    closed = true;
    finishPending?.(fallback("decode_failed"));
    worker.onmessage = null;
    worker.onerror = null;
    worker.onmessageerror = null;
    worker.terminate();
  };
  const workerFailed = (): void => {
    failure = fallback("decode_failed");
    close();
  };
  worker.onerror = (event) => { event.preventDefault(); workerFailed(); };
  worker.onmessageerror = workerFailed;
  worker.onmessage = (event: MessageEvent<DecodeReply>) => { finishPending?.(event.data); };

  function request(message: DecodeRequest, transfer: Transferable[] = []): Promise<DecodeReply> {
    if (closed) return Promise.resolve(fallback("decode_failed"));
    if (remainingMs <= 0) {
      close();
      return Promise.resolve(fallback("decode_timeout"));
    }
    return new Promise((resolve) => {
      const started = performance.now();
      const timer = setTimeout(() => {
        finishPending?.(fallback("decode_timeout"));
      }, remainingMs);
      finishPending = (reply) => {
        clearTimeout(timer);
        finishPending = undefined;
        remainingMs -= performance.now() - started;
        const result = remainingMs <= 0 ? fallback("decode_timeout") : reply;
        if (result.state === "fallback") {
          failure = result;
          close();
        }
        resolve(result);
      };
      try {
        worker.postMessage(message, transfer);
      } catch {
        finishPending(fallback("decode_failed"));
      }
    });
  }

  const first = await request({ type: "open", bytes, facts: asset.facts }, [bytes.buffer]);
  if (first.state === "fallback") return first;
  const { frameCount } = first.facts;
  const result = { state: "decoded", frame: first.frame, facts: first.facts } as const;
  if (frameCount === null || frameCount <= 1) {
    close();
    return result;
  }

  let playing = !reducedMotion;
  let disposed = false;
  let revision = 0;
  let index = 1;
  let pending: Promise<ExpressiveFrameResult> | undefined;
  const playback: ExpressivePlayback = {
    play: () => { playing = true; },
    pause: () => { playing = false; revision++; },
    nextFrame() {
      if (disposed) return Promise.resolve({ state: "closed" });
      if (failure !== undefined) return Promise.resolve(failure);
      if (!playing) return Promise.resolve({ state: "paused" });
      if (pending !== undefined) return pending;
      const requested = revision;
      pending = request({ type: "frame", index }).then((reply) => {
        pending = undefined;
        if (disposed) return { state: "closed" };
        if (reply.state === "fallback") return reply;
        if (requested !== revision) return { state: "paused" };
        index = (index + 1) % frameCount;
        return { state: "frame", frame: reply.frame };
      });
      return pending;
    },
    close() { disposed = true; close(); },
  };
  return { ...result, playback };
}
