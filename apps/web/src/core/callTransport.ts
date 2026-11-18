import { browserCallEnvironment, CallTransport, type CallMediaFact } from "@om/web-platform/calls";
import workerAsset from "@om/web-platform/sframe-worker?worker&url";

const workerUrl = new URL(workerAsset, import.meta.url);
type WorkerPolicy = { createScriptURL: (url: string) => unknown };
let policy: WorkerPolicy | undefined;

function startWorker(): Worker {
  const types = (globalThis as { trustedTypes?: { createPolicy(name: string, rules: WorkerPolicy): WorkerPolicy } }).trustedTypes;
  if (workerUrl.origin !== location.origin) throw new Error("Invalid media worker origin");
  policy ??= types?.createPolicy("om-call-worker", {
    createScriptURL(url) {
      if (url !== workerUrl.href) throw new Error("Invalid media worker URL");
      return url;
    },
  });
  return new Worker((policy?.createScriptURL(workerUrl.href) ?? workerUrl.href) as string);
}

export function createCallTransport(state: (fact: CallMediaFact) => void, stream: (remote: MediaStream) => void): CallTransport {
  return new CallTransport(browserCallEnvironment(startWorker), { state, stream });
}
