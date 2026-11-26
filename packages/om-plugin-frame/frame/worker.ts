// SPDX-License-Identifier: MPL-2.0
import { decode, entry, keys, verdict, wire } from "../protocol";
import type { Call, Verdict, WorkerRequest } from "../protocol";

interface Adapter {
  call(request: Call): Promise<Verdict>;
  sampleMemory(memory: WebAssembly.Memory): Promise<Verdict>;
}

/** Only build-installed, trusted glue sees this adapter. A guest Wasm module sees its explicit imports. */
export function servePlugin(run: (entrypoint: string, adapter: Adapter) => Promise<void>) {
  let started = false;
  let answer: ((value: Verdict) => void) | undefined;
  const post = (body: WorkerRequest) => self.postMessage(wire(body));

  function exchange(body: WorkerRequest) {
    if (answer) throw new Error("Overlapping plugin calls");
    return new Promise<Verdict>((resolve) => {
      answer = resolve;
      post(body);
    });
  }

  self.addEventListener("message", (event: MessageEvent<unknown>) => {
    const data = decode(event.data);
    if (!data || data === "oversize") return;
    if (started) {
      if (!answer || !verdict(data)) return;
      const resolve = answer;
      answer = undefined;
      resolve(data);
      return;
    }
    if (!keys(data, "type", "entrypoint") || data.type !== "run" || !entry(data.entrypoint)) return;
    started = true;
    void run(data.entrypoint, {
      call: (request) => exchange({ type: "call", request }),
      sampleMemory: (memory) => exchange({ type: "account", memoryBytes: memory.buffer.byteLength }),
    }).then(() => post({ type: "done" }), () => post({ type: "fault" }));
  });
}
