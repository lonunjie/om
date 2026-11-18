// SPDX-License-Identifier: AGPL-3.0-or-later
import { execFileSync } from "node:child_process";
import { request, type IncomingHttpHeaders, type IncomingMessage, type Server as HttpServer } from "node:http";
import { createRequire } from "node:module";
import { duplexPair, type Duplex } from "node:stream";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

interface Options {
  persist?: string;
  port?: number;
  vars?: Record<string, string>;
  blobs?: false;
  recovery?: false;
  usage?: false;
  direct?: boolean;
}

export interface SocketTransport {
  socket(url: string, protocols: string[]): WebSocket;
  request(path: string, method: string, headers: Record<string, string>): Promise<{ status: number; headers: IncomingHttpHeaders }>;
}
interface Server {
  readonly url: string;
  fetch(path: string, init?: RequestInit): Promise<Response>;
  dispose(): Promise<void>;
  readonly transport?: SocketTransport;
}

function inMemoryTransport(server: HttpServer, url: string): SocketTransport & Pick<Server, "fetch"> {
  const require = createRequire(import.meta.url);
  const Ws = require("ws") as new (url: string, protocols: string[], options: { createConnection: () => Duplex }) => WebSocket;
  const createConnection = (): Duplex => {
    const [client, listener] = duplexPair();
    // node:http and ws use these TCP socket methods. The in-process connection has no TCP socket.
    const methods = { setNoDelay() {}, setKeepAlive() {}, setTimeout() {} };
    Object.assign(client, methods);
    Object.assign(listener, methods);
    server.emit("connection", listener);
    return client;
  };
  return {
    async fetch(path, init) {
      const call = new Request(url + path, init);
      const headers = Object.fromEntries(call.headers);
      const body = call.body === null ? undefined : Buffer.from(await call.arrayBuffer());
      if (body !== undefined) headers["content-length"] = String(body.length);
      const response = await new Promise<IncomingMessage>((resolve, reject) => {
        const req = request(call.url, { method: call.method, headers, createConnection, signal: call.signal }, resolve);
        req.on("error", reject);
        req.end(body);
      });
      const bytes: Buffer[] = [];
      for await (const chunk of response as AsyncIterable<Buffer>) bytes.push(chunk);
      return new Response(bytes.length === 0 ? null : Buffer.concat(bytes), {
        status: response.statusCode ?? 500,
        headers: response.rawHeaders.flatMap<[string, string]>((name, index, values) => index % 2 === 0 ? [[name, values[index + 1] ?? ""]] : []),
      });
    },
    socket: (url, protocols) => new Ws(url, protocols, { createConnection }),
    request: (path, method, headers) => new Promise((resolve, reject) => {
      const req = request(url + path, { method, headers, createConnection }, (res) => {
        res.resume();
        resolve({ status: res.statusCode ?? 0, headers: res.headers });
      });
      req.on("upgrade", (res, socket) => {
        socket.destroy();
        resolve({ status: res.statusCode ?? 0, headers: res.headers });
      });
      req.on("error", reject);
      req.end();
    }),
  };
}

let build: string | undefined;
let instance = 0;
function bundle(): string {
  if (build) return build;
  const directory = mkdtempSync(join(tmpdir(), "om-home-node-bundle-"));
  const out = join(directory, "home.mjs");
  process.once("exit", () => { rmSync(directory, { recursive: true, force: true }); });
  execFileSync(process.execPath, [fileURLToPath(new URL("../build.mjs", import.meta.url)), out]);
  build = out;
  return out;
}

export async function startSqliteHome(options: Options = {}): Promise<Server> {
  const url = pathToFileURL(bundle());
  url.searchParams.set("instance", String(instance++));
  const module = await import(/* @vite-ignore */ url.href) as {
    startHome(options: Omit<Options, "persist"> & { directory: string; hostname: string }): Promise<Omit<Server, "fetch">>;
    openListener(options: Omit<Options, "persist"> & { directory: string }): Promise<{
      server: HttpServer; dispose(): Promise<void>;
    }>;
  };
  const directory = options.persist ?? mkdtempSync(join(tmpdir(), "om-home-node-test-"));
  const cleanup = (): void => { if (options.persist === undefined) rmSync(directory, { recursive: true, force: true }); };
  try {
    const config = {
      ...options, directory, hostname: "127.0.0.1", port: options.port ?? 0,
      vars: { OM_PROFILE: "default", OM_ADMISSION_REQUIRED: "0", OM_PROVIDER_ID: "om-test-home", ...options.vars },
    };
    if (options.direct) {
      const home = await module.openListener(config);
      // eslint-disable-next-line no-restricted-syntax -- synthetic origin for in-process tests
      const url = "http://127.0.0.1";
      const transport = inMemoryTransport(home.server, url);
      return {
        url, fetch: transport.fetch, transport,
        dispose: async () => { await home.dispose(); cleanup(); },
      };
    }
    const home = await module.startHome(config);
    return { url: home.url, fetch: (path, init) => fetch(home.url + path, init), dispose: async () => { await home.dispose(); cleanup(); } };
  } catch (error) {
    cleanup();
    throw error;
  }
}
