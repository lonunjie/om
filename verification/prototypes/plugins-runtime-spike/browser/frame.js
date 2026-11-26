// SPDX-License-Identifier: MPL-2.0
const parameters = new URL(location.href).searchParams;
const clientOrigin = parameters.get("client");
const phase = parameters.get("phase");
const violations = observeCsp();

function exchange(worker, message, timeoutMs = 8_000) {
  return new Promise((resolve) => {
    const finish = (result) => {
      clearTimeout(timer);
      worker.onmessage = worker.onerror = worker.onmessageerror = null;
      resolve(result);
    };
    const timer = setTimeout(() => finish({ type: "timeout", request: message.type }), timeoutMs);
    worker.onmessage = (event) => finish(event.data);
    worker.onerror = (event) => finish({ type: "worker-error", error: event.message });
    worker.onmessageerror = () => finish({ type: "worker-messageerror" });
    worker.postMessage(message);
  });
}

function watchdog(worker) {
  return new Promise((resolve) => {
    const limitMs = 500;
    const messages = [];
    const sent = performance.now();
    let started;
    let timer = setTimeout(() => finish("no-start-signal"), 3_000);
    function finish(reason) {
      clearTimeout(timer);
      worker.terminate();
      worker.onmessage = worker.onerror = null;
      resolve({ reason, limit_ms: limitMs, start_signal_ms: started === undefined ? null : started - sent,
        call_to_terminate_ms: started === undefined ? null : performance.now() - started, messages });
    }
    worker.onmessage = ({ data }) => {
      messages.push(data);
      if (data.type !== "spin-started") { finish("returned-before-watchdog"); return; }
      started = performance.now();
      clearTimeout(timer);
      timer = setTimeout(() => finish("watchdog-terminated"), limitMs);
      worker.postMessage({ type: "ping" });
    };
    worker.onerror = (event) => {
      messages.push({ type: "worker-error", error: event.message });
      finish("worker-error");
    };
    worker.postMessage({ type: "spin" });
  });
}

async function measure({ cases, urls, samples }) {
  const out = { startup: [], frame: { origin: location.origin, secure_context: isSecureContext,
    cross_origin_isolated: crossOriginIsolated } };
  for (let sample = 0; sample < samples; sample += 1) {
    const start = performance.now();
    const worker = new Worker(`/worker.js?phase=${phase}`);
    try {
      const ready = await exchange(worker, { type: "instantiate" });
      out.startup.push({ ...ready, worker_round_trip_ms: performance.now() - start });
      if (sample !== 0) continue;
      if (ready.type === "ready") out.calls = await exchange(worker, { type: "calls", cases });
      out.worker_network = await exchange(worker, { type: "network", urls: urls.worker });
      if (phase === "confined" && ready.type === "ready") {
        out.spin = await watchdog(worker);
        // A timer task on the owner frame must still be serviced after termination.
        await new Promise((resolve) => setTimeout(resolve, 0));
        out.frame_alive_after_spin = true;
      }
    } finally {
      worker.terminate();
    }
  }
  out.frame_beacon = await networkProbe("sendBeacon", urls.frameBeacon);
  await new Promise((resolve) => setTimeout(resolve, 250));
  return { ...out, frame_violations: violations };
}

function bind(event) {
  if (event.origin !== clientOrigin || event.source !== parent || event.data?.type !== "bind" ||
      typeof event.data.binding !== "string" || event.ports.length !== 1) return;
  removeEventListener("message", bind);
  const { binding } = event.data;
  const [port] = event.ports;
  let next = 1;
  port.onmessage = async (delivery) => {
    const data = delivery.data;
    if (delivery.target !== port || data?.binding !== binding || data.sequence !== next || data.type !== "measure") return;
    next += 1;
    let result;
    try { result = await measure(data); } catch (error) { result = { error: String(error), frame_violations: violations }; }
    port.postMessage({ type: "result", binding, sequence: data.sequence, result });
  };
  port.postMessage({ type: "bound", binding, sequence: 0 });
}

addEventListener("message", bind);
parent.postMessage({ type: "ready", claimedOrigin: location.origin }, clientOrigin);
