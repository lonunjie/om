/// <reference lib="webworker" />

import type { ExpressiveDecodeFacts, ExpressiveDecodeFallback, ExpressiveFrame, ValidatedExpressiveAsset } from "@om/platform-api";
import { DecoderMismatch, openFrameDecoder } from "./native";
import type { FrameDecoder } from "./native";

export type DecodeRequest =
  | { readonly type: "open"; readonly bytes: Uint8Array<ArrayBuffer>; readonly facts: ValidatedExpressiveAsset["facts"] }
  | { readonly type: "frame"; readonly index: number };

export type DecodeReply =
  | { readonly state: "frame"; readonly frame: ExpressiveFrame; readonly facts: ExpressiveDecodeFacts }
  | ExpressiveDecodeFallback;

const worker = self as unknown as DedicatedWorkerGlobalScope;
let decoder: FrameDecoder | undefined;

// The adapter sends one request at a time and terminates the worker on timeout or disposal.
async function handle(request: DecodeRequest): Promise<void> {
  function reply(result: DecodeReply): void {
    if (result.state === "frame") worker.postMessage(result, [result.frame.rgba.buffer]);
    else worker.postMessage(result);
  }
  try {
    if (request.type === "open") decoder = await openFrameDecoder(request.bytes, request.facts);
    if (decoder === undefined) {
      reply({ state: "fallback", reason: "unavailable" });
      return;
    }
    const result = await decoder.read(request.type === "open" ? 0 : request.index);
    if (decoder.frameCount === null) {
      decoder.close();
      decoder = undefined;
    }
    reply({ state: "frame", ...result });
  } catch (error) {
    decoder?.close();
    decoder = undefined;
    reply({ state: "fallback", reason: error instanceof DecoderMismatch ? "decoder_mismatch" : "decode_failed" });
  }
}

worker.onmessage = (event: MessageEvent<DecodeRequest>) => { void handle(event.data); };
