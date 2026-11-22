// SPDX-License-Identifier: AGPL-3.0-or-later
import { execFileSync, spawn } from "node:child_process";
import { createServer as createHttpServer, request, type IncomingHttpHeaders, type IncomingMessage, type Server as HttpServer } from "node:http";
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
  /** Run the Home as its own process on a real TCP listener (target `sqlite-process`). */
  process?: boolean;
  media?(request: Request): Promise<Response>;
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

const HOME_PROCESS = fileURLToPath(new URL("../../../conformance/load/home-process.mjs", import.meta.url));

/**
 * The Home as its own operating-system process on a real TCP listener, started from the built bundle the way a service
 * manager starts it. Nothing is shared with the test process but the network: a restart is a process exit and a new process.
 * A media backend function is offered over the permission-restricted Unix socket the Home already uses for it.
 */
async function startSqliteProcess(options: Options): Promise<Server> {
  const directory = options.persist ?? mkdtempSync(join(tmpdir(), "om-home-proc-"));
  const scratch = mkdtempSync(join(tmpdir(), "om-home-sock-"));
  const vars: Record<string, string> = { OM_PROFILE: "default", OM_ADMISSION_REQUIRED: "0", OM_PROVIDER_ID: "om-test-home", ...options.vars };
  let backend: HttpServer | undefined;
  if (options.media) {
    const media = options.media.bind(options);
    const socketPath = join(scratch, "media.sock");
    backend = createHttpServer((req, res) => {
      const chunks: Buffer[] = [];
      req.on("data", (c: Buffer) => chunks.push(c));
      req.on("end", () => {
        // eslint-disable-next-line no-restricted-syntax -- synthetic origin: the media backend is a function in the test process
        void media(new Request("http://media.invalid/lease", { method: "POST", headers: { "content-type": "application/json" }, body: Buffer.concat(chunks) }))
          .then(async (r) => { res.statusCode = r.status; res.end(Buffer.from(await r.arrayBuffer())); }, () => { res.statusCode = 503; res.end(); });
      });
    });
    await new Promise<void>((resolve) => { backend?.listen(socketPath, resolve); });
    vars["OM_MEDIA_SOCKET"] = socketPath;
  }
  const child = spawn(process.execPath, [HOME_PROCESS, bundle(), directory, JSON.stringify(vars), JSON.stringify({ blobs: options.blobs, recovery: options.recovery, usage: options.usage })], { stdio: ["ignore", "pipe", "inherit"] });
  const cleanup = (): void => {
    backend?.close();
    rmSync(scratch, { recursive: true, force: true });
    if (options.persist === undefined) rmSync(directory, { recursive: true, force: true });
  };
  try {
    const url = await new Promise<string>((resolve, reject) => {
      let seen = "";
      child.stdout.on("data", (d: Buffer) => {
        seen += d.toString();
        const m = /LISTENING (\S+)/.exec(seen);
        if (m?.[1]) resolve(m[1]);
      });
      child.once("exit", (code) => { reject(new Error(`Home process exited with ${String(code)}`)); });
    });
    return {
      url, fetch: (path, init) => fetch(url + path, init),
      dispose: async () => {
        if (child.exitCode === null && child.signalCode === null) {
          const done = new Promise<void>((resolve) => child.once("exit", () => { resolve(); }));
          child.kill("SIGTERM");
          await done;
        }
        cleanup();
      },
    };
  } catch (error) {
    child.kill("SIGKILL");
    cleanup();
    throw error;
  }
}

export async function startSqliteHome(options: Options = {}): Promise<Server> {
  if (options.process) return startSqliteProcess(options);
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
