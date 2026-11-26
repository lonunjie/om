// SPDX-License-Identifier: MPL-2.0
// Classic Worker so importScripts can be exercised. CSP is on this script's HTTP response.
importScripts("/probe.js");

const violations = observeCsp();
let plugin;

function call({ name, args }) {
  try {
    const value = plugin[name](...args.map((arg) => Array.isArray(arg) ? new Uint8Array(arg) : arg));
    return { ok: value === undefined ? null : ArrayBuffer.isView(value) ? [...value] : value };
  } catch (error) {
    const payload = typeof error === "string" ? error : error?.payload;
    return typeof payload === "string" ? { err: payload } : { exception: String(error) };
  }
}

self.onmessage = async ({ data }) => {
  try {
    switch (data.type) {
      case "instantiate": {
        const start = performance.now();
        const [{ instantiate }, { default: modules }] = await Promise.all([
          import("/bundle/fixture.js"), import("/bundle/fixture-bytes.js"),
        ]);
        const loaded = performance.now();
        plugin = await instantiate((name) => {
          const encoded = modules[name];
          if (typeof encoded !== "string") throw new Error(`Unexpected core module ${name}`);
          return WebAssembly.compile(Uint8Array.from(atob(encoded), (char) => char.charCodeAt(0)));
        }, Object.freeze({}));
        self.postMessage({ type: "ready", load_glue_and_bytes_ms: loaded - start,
          compile_instantiate_ms: performance.now() - loaded, exports: Object.keys(plugin),
          secure_context: isSecureContext, cross_origin_isolated: crossOriginIsolated });
        break;
      }
      case "calls":
        self.postMessage({ type: "calls", results: Object.fromEntries(data.cases.map((test) => [test.id, call(test)])) });
        break;
      case "network": {
        const results = await Promise.all(Object.entries(data.urls).map(([api, url]) => networkProbe(api, url)));
        // The CSP event can arrive as a separate task, including after sendBeacon returns.
        await new Promise((resolve) => setTimeout(resolve, 250));
        self.postMessage({ type: "network", results, violations });
        break;
      }
      case "spin":
        self.postMessage({ type: "spin-started" });
        self.postMessage({ type: "spin-returned", result: call({ name: "onEvent", args: [[255]] }) });
        break;
      case "ping":
        self.postMessage({ type: "pong" });
        break;
    }
  } catch (error) {
    self.postMessage({ type: "error", error: String(error), violations });
  }
};
