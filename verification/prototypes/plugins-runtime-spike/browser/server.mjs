// SPDX-License-Identifier: MPL-2.0
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { once } from "node:events";

const scripts = ["client.js", "frame.js", "probe.js", "worker.js"];
const html = (...scripts) => `<!doctype html><meta charset="utf-8"><body>${scripts.map((src) => `<script src="${src}"></script>`).join("")}`;
const wrongScript = `const query = new URL(location.href).searchParams;
parent.postMessage({ type: "ready", claimedOrigin: query.get("claims") }, query.get("client"));`;
const isolate = {
  "Cross-Origin-Opener-Policy": "same-origin",
  "Cross-Origin-Embedder-Policy": "require-corp",
  "Cross-Origin-Resource-Policy": "cross-origin",
};

async function listen(server) {
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  return server.address().port;
}

export async function startServer(assets, scriptOverrides = {}) {
  const sources = Object.fromEntries(await Promise.all(scripts.map(async (name) =>
    [name, scriptOverrides[name] ?? await readFile(new URL(`./${name}`, import.meta.url))])));
  const traffic = [];
  const sockets = new Set();
  const sink = createServer((request, response) => {
    const path = new URL(request.url, sinkOrigin).pathname;
    if (path === "/wrong") {
      response.writeHead(200, { ...isolate, "Content-Type": "text/html; charset=utf-8" });
      response.end(html("/wrong.js"));
    } else if (path === "/wrong.js") {
      response.writeHead(200, { ...isolate, "Content-Type": "text/javascript" });
      response.end(wrongScript);
    } else if (path.startsWith("/probe/")) {
      traffic.push({ transport: "http", method: request.method, path });
      request.resume();
      response.writeHead(200, { ...isolate, "Access-Control-Allow-Origin": "*", "Content-Type": "text/javascript" });
      response.end("globalThis.remoteProbeRan = true;");
    } else {
      response.writeHead(404).end();
    }
  });
  sink.on("upgrade", (request, socket) => {
    const path = new URL(request.url, sinkOrigin).pathname;
    const key = request.headers["sec-websocket-key"];
    if (!path.startsWith("/probe/") || !key) { socket.destroy(); return; }
    traffic.push({ transport: "websocket", method: request.method, path });
    sockets.add(socket);
    socket.on("close", () => sockets.delete(socket));
    socket.on("error", () => socket.destroy());
    const accept = createHash("sha1").update(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`).digest("base64");
    socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
    socket.setTimeout(3_000, () => socket.destroy());
    socket.once("data", () => socket.end(Buffer.from([0x88, 0x00]))); // Reply to the probe's close frame.
  });
  const sinkOrigin = `http://127.0.0.1:${await listen(sink)}`;

  function policy(confined) {
    const scripts = confined ? "'self'" : `'self' ${sinkOrigin}`;
    const connections = confined ? "'none'" : `${sinkOrigin} ${sinkOrigin.replace(/^http/, "ws")}`;
    return `default-src 'none'; script-src ${scripts} 'wasm-unsafe-eval'; connect-src ${connections}; worker-src 'self'; base-uri 'none'`;
  }

  const host = createServer((request, response) => {
    const { pathname, searchParams } = new URL(request.url, clientOrigin);
    const phase = searchParams.get("phase");
    const headers = { ...isolate, "Cache-Control": "no-store", "X-Content-Type-Options": "nosniff" };
    let body;
    if (pathname === "/") {
      headers["Content-Type"] = "text/html; charset=utf-8";
      headers["Permissions-Policy"] = `cross-origin-isolated=(self "${pluginOrigin}")`;
      headers["Content-Security-Policy"] = `default-src 'none'; script-src 'self'; frame-src ${pluginOrigin} ${sinkOrigin}`;
      body = html("/client.js");
    } else if (pathname === "/frame" && ["control", "confined"].includes(phase)) {
      headers["Content-Type"] = "text/html; charset=utf-8";
      headers["Content-Security-Policy"] = `${policy(phase === "confined")}; frame-ancestors ${clientOrigin}`;
      body = html("/probe.js", "/frame.js");
    } else if (pathname === "/worker.js" && ["control", "confined"].includes(phase)) {
      headers["Content-Security-Policy"] = policy(phase === "confined");
      body = sources["worker.js"];
    } else if (pathname.startsWith("/bundle/")) {
      body = Object.hasOwn(assets, pathname.slice(8)) ? assets[pathname.slice(8)] : undefined;
    } else if (scripts.includes(pathname.slice(1)) && pathname !== "/worker.js") {
      body = sources[pathname.slice(1)];
    }
    if (body === undefined) { response.writeHead(404).end(); return; }
    headers["Content-Type"] ??= "text/javascript; charset=utf-8";
    response.writeHead(200, headers).end(body);
  });
  let port;
  try { port = await listen(host); } catch (error) { sink.close(); throw error; }
  const clientOrigin = `http://127.0.0.1:${port}`;
  const pluginOrigin = `http://localhost:${port}`;
  return { clientOrigin, pluginOrigin, sinkOrigin, traffic, policy,
    close: () => {
      for (const socket of sockets) socket.destroy();
      return Promise.all([host, sink].map((server) => new Promise((resolve) => {
        server.close(resolve);
        server.closeAllConnections();
      })));
    } };
}
