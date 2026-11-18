// SPDX-License-Identifier: AGPL-3.0-or-later
import { createServer, request as socketRequest, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { Readable, type Duplex } from "node:stream";
import { ReadableStream } from "node:stream/web";
import { once } from "node:events";
import { isAbsolute } from "node:path";
import { MEDIA_MAX_REPLY_BYTES } from "@om/home-wire";
import worker, { BlobSpool, DescriptorStore, IntroGate, KeyPool, MailboxQueue, RecoveryStore, UsageMeter } from "../../cloudflare-home/src/index";
import type { Env } from "../../cloudflare-home/src/env";
import { fail } from "../../cloudflare-home/src/http";
import { LocalRuntime } from "./runtime";
import { WakeTransport, upgradePeer } from "./wake";

interface Options {
  directory: string;
  hostname: string;
  port: number;
  vars?: Readonly<Record<string, string | undefined>>;
  blobs?: false;
  recovery?: false;
  usage?: false;
  media?(request: Request): Promise<Response>;
}

function makeRequest(req: IncomingMessage, upgrading = false): Request {
  const path = req.url ?? "/";
  if (!path.startsWith("/") || path.startsWith("//") || path.includes("\\")) throw new Error("invalid request target");
  const headers = new Headers();
  for (const [name, value] of Object.entries(req.headers)) {
    if (value !== undefined) headers.set(name, Array.isArray(value) ? value.join(", ") : value);
  }
  const method = req.method ?? "GET";
  const init: RequestInit & { duplex?: "half" } = { method, headers };
  if (!upgrading && method !== "GET" && method !== "HEAD") {
    // Cancelling a bounded read must not destroy the socket before the typed response is written.
    const input = req.iterator({ destroyOnReturn: false }) as AsyncIterator<Buffer>;
    const stream = new ReadableStream<Uint8Array>({
      async pull(controller) {
        const chunk = await input.next();
        if (chunk.done) controller.close();
        else controller.enqueue(chunk.value);
      },
      async cancel() { await input.return?.(); },
    }, { highWaterMark: 0 });
    init.body = stream as unknown as BodyInit;
    init.duplex = "half";
  }
  // eslint-disable-next-line no-restricted-syntax -- internal origin for the host-independent request router
  return new Request(`http://localhost${path}`, init);
}

/** The most unread request body the Home reads and discards so the client receives its typed reply instead of a reset. */
const DRAIN_LIMIT = 16 * 1024 * 1024;

/** Resolves true when the request body is fully consumed, false when it exceeds the limit (the caller then cuts the connection). */
function drain(req: IncomingMessage): Promise<boolean> {
  if (req.readableEnded || req.destroyed) return Promise.resolve(req.readableEnded);
  const declared = Number(req.headers["content-length"]);
  if (Number.isFinite(declared) && declared > DRAIN_LIMIT) return Promise.resolve(false);
  return new Promise((resolve) => {
    let total = 0;
    const finish = (complete: boolean): void => {
      req.off("data", onData); req.off("end", onEnd); req.off("close", onClose); req.off("error", onClose);
      resolve(complete);
    };
    const onData = (chunk: Buffer): void => { total += chunk.length; if (total > DRAIN_LIMIT) finish(false); };
    const onEnd = (): void => { finish(true); };
    const onClose = (): void => { finish(false); };
    req.on("data", onData); req.on("end", onEnd); req.on("close", onClose); req.on("error", onClose);
    req.resume();
  });
}

function write(res: ServerResponse, req: IncomingMessage, response: Response): void {
  res.statusCode = response.status;
  for (const [name, value] of response.headers) res.setHeader(name, value);
  if (!req.readableEnded && (req.headers["content-length"] !== undefined || req.headers["transfer-encoding"] !== undefined)) {
    res.setHeader("Connection", "close");
  }
  if (!response.body) { res.end(); return; }
  const stream = Readable.fromWeb(response.body as unknown as ReadableStream<Uint8Array>);
  stream.on("error", () => res.destroy());
  res.on("close", () => stream.destroy());
  stream.pipe(res);
}

async function rejectUpgrade(socket: Duplex, response: Response): Promise<void> {
  const body = Buffer.from(await response.arrayBuffer());
  const headers = [...response.headers].map(([name, value]) => `${name}: ${value}\r\n`).join("");
  const head = `HTTP/1.1 ${String(response.status)} Home\r\n${headers}Connection: close\r\nContent-Length: ${String(body.length)}\r\n\r\n`;
  socket.end(Buffer.concat([Buffer.from(head), body]));
}

/** The private media backend runs under the same owner's control, on a permission-restricted Unix socket. */
function mediaSocket(socketPath: string): NonNullable<Options["media"]> {
  if (!isAbsolute(socketPath)) throw new Error("OM_MEDIA_SOCKET must be an absolute path");
  return async (request) => {
    const body = await request.text();
    const response = await new Promise<IncomingMessage>((resolve, reject) => {
      const rpc = socketRequest({
        socketPath, path: "/lease", method: "POST", signal: request.signal,
        headers: { "content-type": "application/json", "content-length": Buffer.byteLength(body) },
      }, resolve);
      rpc.on("error", reject);
      rpc.end(body);
    });
    const chunks: Buffer[] = [];
    let size = 0;
    for await (const chunk of response as AsyncIterable<Buffer>) {
      size += chunk.length;
      if (size > MEDIA_MAX_REPLY_BYTES) throw new Error("Media lease response too large");
      chunks.push(chunk);
    }
    return new Response(size === 0 ? null : Buffer.concat(chunks), { status: response.statusCode ?? 503 });
  };
}

type ApplicationOptions = Omit<Options, "hostname" | "port">;

async function openHome(options: ApplicationOptions): Promise<{
  fetch(request: Request): Promise<Response>; dispose(): Promise<void>;
}> {
  // Binding shapes are supplied by the local runtime. No environment entry may override a storage namespace.
  const v = options.vars ?? {};
  const vars = {
    OM_PROFILE: v["OM_PROFILE"] ?? "personal",
    OM_PROVIDER_ID: v["OM_PROVIDER_ID"] ?? "om-sqlite-home",
    OM_ADMISSION: v["OM_ADMISSION"],
    OM_ADMISSION_REQUIRED: v["OM_ADMISSION_REQUIRED"] ?? "1",
    OM_HARD_FREE_TIER: v["OM_HARD_FREE_TIER"] ?? "1",
    OM_WAKE: v["OM_WAKE"],
    OM_MEDIA_STUN: v["OM_MEDIA_STUN"],
    OM_BLOB_SPOOL: v["OM_BLOB_SPOOL"],
    OM_RECOVERY_STORE: v["OM_RECOVERY_STORE"],
    OM_USAGE_CEILING_WORKER: v["OM_USAGE_CEILING_WORKER"],
    OM_USAGE_CEILING_DO: v["OM_USAGE_CEILING_DO"],
  };
  const env = vars as unknown as Env;
  const socket = v["OM_MEDIA_SOCKET"];
  const media = options.media ?? (socket ? mediaSocket(socket) : undefined);
  const runtime = new LocalRuntime(options.directory, env);
  Object.assign(env, {
    ...(media === undefined ? {} : { MEDIA_TURN: { fetch: media } }),
    QUEUES: runtime.namespace("queue", MailboxQueue, ["create"]),
    KEYS: runtime.namespace("keys", KeyPool, ["publish"]),
    DESCRIPTORS: runtime.namespace("descriptor", DescriptorStore, ["put", "revoke"]),
    INTRO: runtime.namespace("intro", IntroGate, ["issueTickets"]),
    ...(options.blobs !== false ? { BLOBS: runtime.namespace("blob", BlobSpool, ["put"]) } : {}),
    ...(options.recovery !== false ? { RECOVERY: runtime.namespace("recovery", RecoveryStore, ["scopeId", "putRecord", "registerInbox", "getRecord", "fanout", "postVeto"]) } : {}),
    ...(options.usage !== false ? { USAGE: runtime.namespace("usage", UsageMeter, ["note", "charge"]) } : {}),
  });

  try { runtime.start(); } catch (error) { await runtime.close(); throw error; }
  return { fetch: (request) => worker.fetch(request, env), dispose: () => runtime.close() };
}

export async function openListener(options: ApplicationOptions): Promise<{
  server: Server; dispose: () => Promise<void>;
}> {
  const home = await openHome(options);
  const wake = new WakeTransport();
  const dispatch = async (req: IncomingMessage, upgrading = false): Promise<Response> => {
    let request;
    try { request = makeRequest(req, upgrading); } catch { return fail("invalid_request"); }
    return home.fetch(request);
  };
  const server = createServer({ requestTimeout: 30_000, headersTimeout: 10_000 }, (req, res) => {
    // There are no access logs. The proxy forwards the browser's authenticated HTTPS connection.
    void dispatch(req).then(async (response) => {
      // A reply sent while the client is still uploading resets its connection. Read and discard a bounded remainder first.
      if (!(await drain(req))) res.setHeader("Connection", "close");
      write(res, req, response);
    }).catch(() => res.destroy());
  });
  server.maxConnections = 1024;
  server.setTimeout(30_000, (socket) => socket.destroy());
  server.on("upgrade", (req, socket, head) => {
    socket.on("error", () => socket.destroy());
    void dispatch(req, true).then(async (response) => {
      const peer = upgradePeer(response);
      if (!peer) { await rejectUpgrade(socket, response); return; }
      if (socket.destroyed) { peer.terminate(); return; }
      wake.upgrade(req, socket, head, peer);
    }).catch(() => socket.destroy());
  });

  let closing: Promise<void> | undefined;
  return {
    server,
    dispose: () => closing ??= (async () => {
      wake.close();
      await new Promise<void>((resolve) => { server.close(() => { resolve(); }); });
      await home.dispose();
    })(),
  };
}

export async function startHome(options: Options): Promise<{ url: string; dispose(): Promise<void> }> {
  const { server, dispose } = await openListener(options);
  try {
    server.listen(options.port, options.hostname);
    await once(server, "listening");
    const address = server.address();
    if (!address || typeof address === "string") throw new Error("Home has no TCP listener");
    return {
      // eslint-disable-next-line no-restricted-syntax -- local listener address for the operator, not a remote endpoint
      url: `http://${address.family === "IPv6" ? `[${address.address}]` : address.address}:${String(address.port)}`,
      dispose,
    };
  } catch (error) {
    await dispose();
    throw error;
  }
}

async function main(): Promise<void> {
  process.umask(0o077);
  const directory = process.env["OM_DATA_DIR"];
  const port = Number(process.env["OM_PORT"] ?? "8787");
  if (!directory || !Number.isInteger(port) || port < 1 || port > 65535) {
    process.stderr.write("Set OM_DATA_DIR and use a valid OM_PORT (default 8787).\n");
    process.exitCode = 1;
    return;
  }
  try {
    // Personal deployments always require admission for new resources, including while it is unconfigured.
    const home = await startHome({
      directory, port, hostname: process.env["OM_HOST"] ?? "127.0.0.1",
      vars: { ...process.env, OM_ADMISSION_REQUIRED: "1" },
    });
    process.stdout.write(`OM Home listening at ${home.url}\n`);
    const stop = (): void => { void home.dispose().catch(() => { process.exitCode = 1; }); };
    process.once("SIGINT", stop);
    process.once("SIGTERM", stop);
  } catch {
    process.stderr.write("OM Home could not start. Check the listen address, directory permissions and whether another Home is running there.\n");
    process.exitCode = 1;
  }
}

if (import.meta.main) await main();
