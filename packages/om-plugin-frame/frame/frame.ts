// SPDX-License-Identifier: MPL-2.0
import { deadline, decode, entry, keys, packet, receive, token, verdict, wire, workerRequest } from "../protocol";
import type { ToHost } from "../protocol";

declare const APP_ORIGINS: readonly string[];
const app = new URL(location.href).searchParams.get("app");
if (app && APP_ORIGINS.includes(app) && parent !== window) awaitEmbedder(app);

function awaitEmbedder(appOrigin: string) {
  function bind(event: MessageEvent<unknown>) {
    if (!event.isTrusted || event.origin !== appOrigin || event.source !== parent || event.ports.length !== 1) return;
    if (typeof event.data !== "string" || event.data.length > 128) return;
    const data = decode(event.data);
    const port = event.ports[0];
    if (
      !data || data === "oversize" || !keys(data, "type", "binding") || data.type !== "bind" ||
      !token(data.binding) || !port
    ) return;
    window.removeEventListener("message", bind);
    serve(port, data.binding);
  }

  window.addEventListener("message", bind);
  parent.postMessage(wire({ type: "ready" }), appOrigin);
}

function serve(port: MessagePort, binding: string) {
  let nextIn = 1;
  let nextOut = 1;
  let pending: number | undefined;
  let worker: Worker | undefined;
  let watchdog: ReturnType<typeof setTimeout> | undefined;
  let used = false;

  function stopWorker() {
    clearTimeout(watchdog);
    worker?.terminate();
    worker = undefined;
  }

  function cleanup() {
    stopWorker();
    port.close();
  }

  function post(body: ToHost) {
    const seq = nextOut++;
    port.postMessage(packet(binding, seq, body));
    return seq;
  }

  function finish(body: ToHost) {
    stopWorker();
    post(body);
    port.close();
  }

  window.addEventListener("pagehide", cleanup, { once: true });
  port.onmessageerror = cleanup;
  port.onmessage = (event) => {
    if (event.target !== port || event.ports.length) return;
    const body = receive(event.data, binding, nextIn);
    if (!body || body === "oversize") return;
    if (nextIn === 1) {
      if (body.type !== "authenticate" || !keys(body, "type")) return;
      nextIn++;
      post({ type: "bound" });
      return;
    }
    if (
      body.type === "run" && keys(body, "type", "entrypoint", "wallClockMs") &&
      !used && entry(body.entrypoint) && deadline(body.wallClockMs)
    ) {
      nextIn++;
      used = true;
      start(body.entrypoint, body.wallClockMs);
      return;
    }
    if (
      body.type === "verdict" && keys(body, "type", "replyTo", "verdict") &&
      pending !== undefined && body.replyTo === pending && verdict(body.verdict)
    ) {
      nextIn++;
      pending = undefined;
      if ("refused" in body.verdict && body.verdict.refused === "resource_budget_exceeded") {
        finish({ type: "done" });
        return;
      }
      worker?.postMessage(wire(body.verdict));
    }
  };

  function start(entrypoint: string, wallClockMs: number) {
    const fail = () => finish({ type: "stop", reason: "worker_error" });
    try {
      // A classic dedicated Worker loaded over HTTP, so its own response enforces the spike's CSP.
      worker = new Worker("/worker.js");
    } catch {
      fail();
      return;
    }
    watchdog = setTimeout(() => finish({ type: "stop", reason: "watchdog" }), wallClockMs);
    worker.onerror = fail;
    worker.onmessageerror = fail;
    worker.onmessage = (event: MessageEvent<unknown>) => {
      if (event.target !== worker || event.ports.length || pending !== undefined) return;
      const body = decode(event.data);
      if (body === "oversize") {
        pending = post({ type: "malformed" });
        return;
      }
      if (!body || !workerRequest(body)) return;
      if (body.type === "done" || body.type === "fault") {
        finish(body);
        return;
      }
      const serialized = packet(binding, nextOut, body);
      pending = decode(serialized) === "oversize" ? post({ type: "malformed" }) : post(body);
    };
    worker.postMessage(wire({ type: "run", entrypoint }));
  }
}
