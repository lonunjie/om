// SPDX-License-Identifier: MPL-2.0
// Shared by the classic Worker and its owner frame. Every destination is a local test sink.
globalThis.observeCsp = () => {
  const events = [];
  addEventListener("securitypolicyviolation", (event) => {
    events.push({ blockedURI: event.blockedURI, effectiveDirective: event.effectiveDirective,
      violatedDirective: event.violatedDirective, disposition: event.disposition });
  });
  return events;
};

globalThis.networkProbe = async (api, url) => {
  const start = performance.now();
  const controller = new AbortController();
  let timer;
  const implementations = {
    fetch: [globalThis.fetch, async () => {
      const response = await fetch(url, { signal: controller.signal });
      return { outcome: response.ok ? "completed" : "http-error", status: response.status };
    }],
    websocket: [globalThis.WebSocket, () => new Promise((resolve) => {
      const socket = new WebSocket(url.replace(/^http/, "ws"));
      controller.signal.addEventListener("abort", () => socket.close(), { once: true });
      socket.onopen = () => { resolve({ outcome: "opened" }); socket.close(); };
      socket.onerror = () => resolve({ outcome: "error-event" });
      socket.onclose = (event) => resolve({ outcome: "closed", code: event.code });
    })],
    xhr: [globalThis.XMLHttpRequest, () => new Promise((resolve) => {
      const request = new XMLHttpRequest();
      request.open("GET", url);
      controller.signal.addEventListener("abort", () => request.abort(), { once: true });
      request.onload = () => resolve({ outcome: "completed", status: request.status });
      request.onerror = () => resolve({ outcome: "error-event", status: request.status });
      request.send();
    })],
    importScripts: [globalThis.importScripts, () => {
      delete globalThis.remoteProbeRan;
      importScripts(url);
      return { outcome: globalThis.remoteProbeRan === true ? "executed" : "missing-marker" };
    }],
    sendBeacon: [globalThis.navigator?.sendBeacon, () => ({
      outcome: navigator.sendBeacon(url, "spike") ? "queued" : "rejected",
    })],
  };
  const [available, run] = implementations[api];
  if (typeof available !== "function") return { api, url, outcome: "unavailable" };
  let result;
  try {
    const timeout = new Promise((resolve) => {
      timer = setTimeout(() => { resolve({ outcome: "timeout" }); controller.abort(); }, 2_000);
    });
    result = await Promise.race([run(), timeout]);
  } catch (error) {
    result = { outcome: "threw", error: String(error) };
  } finally {
    clearTimeout(timer);
  }
  return { api, url, ...result, elapsed_ms: performance.now() - start };
};
