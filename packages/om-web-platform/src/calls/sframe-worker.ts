import { SFrameReceiver, SFrameSender } from "./sframe";

export type SFrameInstall = {
  readonly generation: 0;
  readonly send: Uint8Array<ArrayBuffer>;
  readonly receive: Uint8Array<ArrayBuffer>;
};

export type SFrameWorkerFact = "ready" | "keyed" | "received" | "failed";

interface EncodedFrame { data: ArrayBuffer }
interface TransformEvent extends Event {
  readonly transformer: {
    readonly options: { direction: "send" | "receive" };
    readonly readable: ReadableStream<EncodedFrame>;
    readonly writable: WritableStream<EncodedFrame>;
  };
}

const sender = new SFrameSender();
const receiver = new SFrameReceiver();
let keyed = false;
let received = false;
let failed = false;

function fail(): void {
  if (failed) return;
  failed = true;
  keyed = false;
  sender.close();
  receiver.close();
  postMessage("failed" satisfies SFrameWorkerFact);
}

addEventListener("message", (event: MessageEvent<SFrameInstall>) => {
  const { generation, send, receive } = event.data;
  if (failed || generation !== 0 || send.length !== 32 || receive.length !== 32) {
    send.fill(0);
    receive.fill(0);
    fail();
    return;
  }
  void Promise.all([sender.install(0n, send), receiver.install(0n, receive)]).then(() => {
    if (failed) return;
    keyed = true;
    postMessage("keyed" satisfies SFrameWorkerFact);
  }).catch(fail);
});

addEventListener("rtctransform", (event) => {
  const { options, readable, writable } = (event as TransformEvent).transformer;
  const transform = new TransformStream<EncodedFrame, EncodedFrame>({
    async transform(frame, controller) {
      if (!keyed) return;
      const data = new Uint8Array(frame.data);
      const output = options.direction === "send" ? (await sender.seal(data))?.buffer : await receiver.open(data);
      if (output === undefined || failed) return;
      frame.data = output;
      controller.enqueue(frame);
      if (options.direction === "receive" && !received) {
        received = true;
        postMessage("received" satisfies SFrameWorkerFact);
      }
    },
  });
  void readable.pipeThrough(transform).pipeTo(writable).catch(fail);
});

postMessage("ready" satisfies SFrameWorkerFact);
