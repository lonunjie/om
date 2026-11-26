// SPDX-License-Identifier: MPL-2.0
import { createHash } from "node:crypto";
import { once } from "node:events";
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { bundle, frameAssets } from "../build.mjs";

const html = (script) => `<!doctype html><meta charset="utf-8"><script defer src="${script}"></script>`;
const isolate = {
  "Cross-Origin-Opener-Policy": "same-origin",
  "Cross-Origin-Embedder-Policy": "require-corp",
  "Cross-Origin-Resource-Policy": "cross-origin",
  "Cache-Control": "no-store",
  "X-Content-Type-Options": "nosniff",
};
const js = "text/javascript; charset=utf-8";
const htmlType = "text/html; charset=utf-8";

async function listen(server) {
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  return server.address().port;
}

function end(response, headers, contentType, body) {
  response.writeHead(200, { ...headers, "Content-Type": contentType }).end(body);
}

function close(server) {
  return new Promise((resolve) => {
    server.close(resolve);
    server.closeAllConnections();
  });
}

export async function startServer(wasm) {
  const traffic = [];
  const sockets = new Set();
  let client;
  let plugin;
  let sink;
  let assets;
  let app;
  let probe;
  let network;
  let policy;
  let controlPolicy;

  const host = createServer((request, response) => {
    const url = new URL(request.url, client);
    if (request.headers.host === new URL(client).host) {
      if (url.pathname === "/") {
        return end(
          response,
          {
            ...isolate,
            "Permissions-Policy": `cross-origin-isolated=(self "${plugin}")`,
            "Content-Security-Policy":
              `default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; frame-src ${plugin} ${sink}`,
          },
          htmlType,
          html("/app.js"),
        );
      }
      if (url.pathname === "/app.js") return end(response, isolate, js, app);
      if (url.pathname === "/scope.wasm") return end(response, isolate, "application/wasm", wasm);
    }

    if (request.headers.host === new URL(plugin).host) {
      const headers = url.searchParams.get("mode") === "control" ? controlPolicy : policy;
      switch (url.pathname) {
        case "/":
          return end(response, policy, htmlType, assets["index.html"]);
        case "/frame.js":
          return end(response, policy, js, assets["frame.js"]);
        case "/worker.js":
          return end(response, policy, js, assets["worker.js"]);
        case "/network":
          return end(response, headers, htmlType, html("/probe.js"));
        case "/probe.js":
          return end(response, policy, js, probe);
        case "/network-worker.js":
          return end(response, headers, js, network);
        default:
          break;
      }
    }
    response.writeHead(404).end();
  });

  const remote = createServer((request, response) => {
    const { pathname } = new URL(request.url, sink);
    if (pathname === "/quiet") return end(response, isolate, htmlType, "<!doctype html>");
    if (pathname === "/foreign") return end(response, isolate, htmlType, html("/foreign.js"));
    if (pathname === "/foreign.js") {
      return end(response, isolate, js, `parent.postMessage('{"type":"ready"}', ${JSON.stringify(client)});`);
    }
    if (pathname.startsWith("/probe/")) {
      traffic.push({
        transport: "http",
        path: pathname,
        method: request.method,
        referrerPresent: Boolean(request.headers.referer),
      });
      request.resume();
      return end(response, { ...isolate, "Access-Control-Allow-Origin": "*" }, js, "globalThis.remoteProbeRan = true;");
    }
    response.writeHead(404).end();
  });

  // Working WebSocket control, following the measured spike's local sink.
  remote.on("upgrade", (request, socket) => {
    const { pathname } = new URL(request.url, sink);
    const key = request.headers["sec-websocket-key"];
    if (!pathname.startsWith("/probe/") || !key) {
      socket.destroy();
      return;
    }
    traffic.push({
      transport: "websocket",
      path: pathname,
      method: request.method,
      referrerPresent: Boolean(request.headers.referer),
    });
    sockets.add(socket);
    socket.on("close", () => sockets.delete(socket));
    socket.on("error", () => socket.destroy());
    socket.setTimeout(3_000, () => socket.destroy());
    const accept = createHash("sha1").update(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`).digest("base64");
    socket.write(
      `HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`,
    );
    socket.once("data", () => socket.end(Buffer.from([0x88, 0x00])));
  });

  async function shutdown() {
    for (const socket of sockets) socket.destroy();
    await Promise.all([close(host), close(remote)]);
  }

  try {
    sink = `http://127.0.0.1:${await listen(remote)}`;
    const port = await listen(host);
    client = `http://127.0.0.1:${port}`;
    plugin = `http://localhost:${port}`;
    [assets, app, probe, network] = await Promise.all([
      frameAssets([client], "browser/hostile.ts"),
      bundle("browser/app.ts", { PLUGIN_ORIGIN: JSON.stringify(plugin), SINK_ORIGIN: JSON.stringify(sink) }),
      readFile(new URL("../../../verification/prototypes/plugins-runtime-spike/browser/probe.js", import.meta.url)),
      readFile(new URL("./network-worker.js", import.meta.url)),
    ]);
    policy = Object.fromEntries(
      assets._headers.trimEnd().split("\n").slice(1).map((line) => {
        const colon = line.indexOf(":");
        return [line.slice(0, colon).trim(), line.slice(colon + 1).trim()];
      }),
    );
    controlPolicy = {
      ...policy,
      "Content-Security-Policy": policy["Content-Security-Policy"]
        .replace("script-src 'self'", `script-src 'self' ${sink}`)
        .replace("connect-src 'none'", `connect-src ${sink} ${sink.replace(/^http/, "ws")}`),
    };
    return { client, plugin, sink, traffic, policy, controlPolicy, close: shutdown };
  } catch (error) {
    await shutdown();
    throw error;
  }
}
