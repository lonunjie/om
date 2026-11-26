// SPDX-License-Identifier: MPL-2.0
// Parent and plugin bootstrap over Window messages, then use only the bound port.
const parameters = new URL(location.href).searchParams;
const pluginOrigin = parameters.get("plugin");
const sinkOrigin = parameters.get("sink");

function navigate(frame, url, expectedOrigin) {
  return new Promise((resolve) => {
    const finish = (result) => {
      removeEventListener("message", onMessage);
      clearTimeout(timer);
      resolve(result);
    };
    const timer = setTimeout(() => finish({ decision: "no-ready-message" }), 8_000);
    function onMessage(event) {
      if (event.source !== frame.contentWindow || event.data?.type !== "ready") return;
      finish({ actual_origin: event.origin, claimed_origin: event.data.claimedOrigin,
        same_window: true, decision: event.origin === expectedOrigin ? "accept" : "reject-origin" });
    }
    addEventListener("message", onMessage);
    frame.src = url;
  });
}

function exchange(frame, expectedOrigin, request) {
  return new Promise((resolve) => {
    const { port1, port2 } = new MessageChannel();
    const binding = crypto.randomUUID();
    const received = [];
    const finish = (result) => {
      clearTimeout(timer);
      port1.close();
      resolve({ ...result, received });
    };
    const timer = setTimeout(() => finish({ error: "bound channel deadline" }), 65_000);
    let bound = false;
    port1.onmessage = (event) => {
      const message = event.data;
      const validEndpoint = event.target === port1 && message?.binding === binding;
      received.push({ type: message?.type, sequence: message?.sequence, valid_endpoint: validEndpoint });
      if (!validEndpoint) return;
      if (!bound && message.sequence === 0 && message.type === "bound") {
        bound = true;
        port1.postMessage({ ...request, binding, type: "measure", sequence: 1 });
      } else if (bound && message.sequence === 1 && message.type === "result") {
        finish({ result: message.result });
      }
    };
    port1.onmessageerror = () => finish({ error: "channel messageerror" });
    frame.contentWindow.postMessage({ type: "bind", binding }, expectedOrigin, [port2]);
  });
}

window.measurePwa = async ({ phase, ...request }) => {
  const frame = document.createElement("iframe");
  frame.setAttribute("sandbox", "allow-scripts allow-same-origin");
  frame.allow = `cross-origin-isolated ${pluginOrigin}`;
  document.body.append(frame);
  const result = { client_origin: location.origin, secure_context: isSecureContext,
    cross_origin_isolated: crossOriginIsolated, component_api: typeof WebAssembly.Component, ports_transferred: 0 };
  try {
    if (phase === "confined") {
      const wrong = new URL("/wrong", sinkOrigin);
      wrong.search = new URLSearchParams({ client: location.origin, claims: pluginOrigin });
      // Use the same WindowProxy so this challenges event.origin independently of event.source.
      result.wrong_origin = await navigate(frame, wrong, pluginOrigin);
      result.wrong_origin.ports_transferred = result.ports_transferred;
    }
    const target = new URL("/frame", pluginOrigin);
    target.search = new URLSearchParams({ client: location.origin, phase });
    result.plugin_origin = await navigate(frame, target, pluginOrigin);
    if (result.plugin_origin.decision === "accept") {
      result.ports_transferred += 1;
      result.channel = await exchange(frame, pluginOrigin, request);
    }
  } catch (error) {
    result.error = String(error);
  } finally {
    frame.remove();
  }
  return result;
};
